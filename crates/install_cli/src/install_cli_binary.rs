use super::register_application_schemes;
use anyhow::Result;
use gpui::{AppContext as _, AsyncApp, Context, PromptLevel, Window, actions};
use release_channel::ReleaseChannel;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use util::ResultExt;
use workspace::notifications::simple_message_notification::MessageNotification;
use workspace::notifications::{DetachAndPromptErr, NotificationId};
use workspace::{Toast, Workspace};

actions!(
    cli,
    [
        /// Installs the Orion Studio CLI tool to the system PATH.
        InstallCliBinary,
    ]
);

const CANT_INSTALL_DOCS_URL: &str = "https://github.com/orion-agents/orion-studio/blob/main/docs/src/macos.md#cant-install-cli-cant-install-cli";
const CANONICAL_CLI_INSTALL_PATH: &str = "/usr/local/bin/orion-studio";

/// Attempts to install the CLI symlink. Returns the installed path on success,
/// or `None` if the user dismissed the macOS administrator authentication
/// prompt. Returns an error if the install could not be completed, most
/// commonly because the user is not an admin.
async fn install_script(cx: &AsyncApp) -> Result<Option<PathBuf>> {
    let cli_path = cx.update(|cx| cx.path_for_auxiliary_executable("cli"))?;
    let link_path = Path::new(CANONICAL_CLI_INSTALL_PATH);
    let bin_dir_path = link_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("CLI install path has no parent directory"))?;

    // Don't re-create symlink if it points to the same CLI binary.
    if smol::fs::read_link(link_path).await.ok().as_ref() == Some(&cli_path) {
        return Ok(Some(link_path.into()));
    }

    // If the symlink is not there or is outdated, first try replacing it
    // without escalating.
    smol::fs::remove_file(link_path).await.log_err();
    if smol::fs::unix::symlink(&cli_path, link_path)
        .await
        .log_err()
        .is_some()
    {
        return Ok(Some(link_path.into()));
    }

    // The symlink could not be created without escalating, so use osascript
    // with admin privileges to create it.
    let output = smol::process::Command::new("/usr/bin/osascript")
        .args([
            "-e",
            &format!(
                "do shell script \" \
                    mkdir -p \'{}\' && \
                    ln -sf \'{}\' \'{}\' \
                \" with administrator privileges",
                bin_dir_path.to_string_lossy(),
                cli_path.to_string_lossy(),
                link_path.to_string_lossy(),
            ),
        ])
        .output()
        .await?;

    if output.status.success() {
        return Ok(Some(link_path.into()));
    }

    // osascript reports "User canceled." (error -128) when the administrator
    // prompt is dismissed. Treat that as a cancellation rather than a failure
    // so we don't show an error the user already chose to avoid.
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("User canceled") || stderr.contains("-128") {
        return Ok(None);
    }

    // The privileged write failed, most commonly because the user is not an
    // admin.
    anyhow::bail!("error running osascript: {}", stderr.trim());
}

pub fn install_cli_binary(window: &mut Window, cx: &mut Context<Workspace>) {
    const LINUX_PROMPT_DETAIL: &str = "If you installed Orion Studio from an official release, add ~/.local/bin to your PATH.\n\nIf you installed Orion Studio from a package manager, you may need to create an alias or symlink manually.\n\nThe canonical CLI is named orion-studio. Release bundles may also provide orion as a short alias and zed as a deprecated compatibility alias.";

    cx.spawn_in(window, async move |workspace, cx| {
        if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            let prompt = cx.prompt(
                PromptLevel::Warning,
                "CLI should already be installed",
                Some(LINUX_PROMPT_DETAIL),
                &["OK"],
            );
            cx.background_spawn(prompt).detach();
            return Ok(());
        }
        let path = match install_script(cx.deref()).await {
            Ok(Some(path)) => path,
            // The user dismissed the administrator prompt; nothing to do.
            Ok(None) => return Ok(()),
            Err(error) => {
                log::error!("failed to install Orion Studio CLI: {error:#}");
                workspace.update(cx, |workspace, cx| {
                    struct CliInstallFailed;

                    workspace.show_notification(
                        NotificationId::unique::<CliInstallFailed>(),
                        cx,
                        |cx| {
                            cx.new(|cx| {
                                MessageNotification::new(
                                    "You can add `orion-studio` to your PATH manually.",
                                    cx,
                                )
                                .with_title("Couldn't install the Orion Studio CLI")
                                .more_info_message("Show me how")
                                .more_info_url(CANT_INSTALL_DOCS_URL)
                            })
                        },
                    );
                })?;
                return Ok(());
            }
        };

        workspace.update_in(cx, |workspace, _, cx| {
            struct InstalledOrionCli;

            workspace.show_toast(
                Toast::new(
                    NotificationId::unique::<InstalledOrionCli>(),
                    format!(
                        "Installed `orion-studio` to {}. You can launch {} from your terminal.",
                        path.to_string_lossy(),
                        ReleaseChannel::global(cx).display_name()
                    ),
                ),
                cx,
            )
        })?;
        // Keep opening zed:// links created before the Orion Studio migration.
        register_application_schemes(cx).await.log_err();
        Ok(())
    })
    .detach_and_prompt_err(
        "Cannot install the Orion Studio CLI",
        window,
        cx,
        |_, _, _| None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_cli_install_path_uses_orion_studio() {
        let install_path = Path::new(CANONICAL_CLI_INSTALL_PATH);

        assert_eq!(
            install_path.file_name().and_then(|name| name.to_str()),
            Some("orion-studio")
        );
    }
}
