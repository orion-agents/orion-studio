#![allow(
    clippy::disallowed_methods,
    reason = "build helper used only from build scripts"
)]
#![cfg(target_os = "windows")]

use std::process::Command;

fn environment_variable_from(
    canonical_name: &str,
    legacy_name: &str,
    mut get_environment_variable: impl FnMut(&str) -> Result<String, std::env::VarError>,
) -> Result<String, std::env::VarError> {
    match get_environment_variable(canonical_name) {
        Err(std::env::VarError::NotPresent) => get_environment_variable(legacy_name),
        result => result,
    }
}

fn environment_variable(
    canonical_name: &str,
    legacy_name: &str,
) -> Result<String, std::env::VarError> {
    environment_variable_from(canonical_name, legacy_name, |name| std::env::var(name))
}

fn release_channel_from(
    mut get_environment_variable: impl FnMut(&str) -> Result<String, std::env::VarError>,
) -> String {
    match environment_variable_from(
        "ORION_STUDIO_RELEASE_CHANNEL",
        "ZED_RELEASE_CHANNEL",
        &mut get_environment_variable,
    ) {
        Ok(channel) => channel,
        Err(std::env::VarError::NotPresent) => {
            get_environment_variable("RELEASE_CHANNEL").unwrap_or_else(|_| "dev".to_owned())
        }
        Err(std::env::VarError::NotUnicode(_)) => "dev".to_owned(),
    }
}

fn release_channel() -> String {
    release_channel_from(|name| std::env::var(name))
}

fn resource_identity(channel: &str) -> (&'static str, &'static str) {
    match channel {
        "stable" => ("app-icon.ico", "Orion Studio"),
        "preview" => ("app-icon-preview.ico", "Orion Studio Preview"),
        "nightly" => ("app-icon-nightly.ico", "Orion Studio Nightly"),
        _ => ("app-icon-dev.ico", "Orion Studio Dev"),
    }
}

fn git_sha() -> Option<String> {
    if let Ok(sha) = environment_variable("ORION_STUDIO_COMMIT_SHA", "ZED_COMMIT_SHA") {
        return Some(sha);
    }

    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn product_version() -> String {
    let commit_sha = git_sha();
    let pkg_version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let channel = release_channel();
    let build_id = std::env::var("GITHUB_RUN_NUMBER").ok();

    let mut metadata = channel;
    if let Some(build_id) = &build_id {
        metadata.push('.');
        metadata.push_str(build_id);
    }
    if let Some(sha) = &commit_sha {
        metadata.push('.');
        metadata.push_str(sha);
    }

    format!("{pkg_version}+{metadata}")
}

const ICON_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../zed/resources/windows");
const MANIFEST_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/resources/manifest.xml");

pub fn compile(manifest: bool) -> Result<(), Box<dyn std::error::Error>> {
    for name in [
        "ORION_STUDIO_RELEASE_CHANNEL",
        "ZED_RELEASE_CHANNEL",
        "ORION_STUDIO_COMMIT_SHA",
        "ZED_COMMIT_SHA",
        "ORION_STUDIO_RC_TOOLKIT_PATH",
        "ZED_RC_TOOLKIT_PATH",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }

    let channel = release_channel();
    let (icon_filename, product_name) = resource_identity(&channel);
    let icon = std::path::PathBuf::from(ICON_DIR).join(icon_filename);
    let icon_escaped = icon.to_string_lossy().replace('\\', "\\\\");

    let manifest_line = if manifest {
        let escaped = MANIFEST_PATH.replace('\\', "\\\\");
        format!("1 24 \"{escaped}\"")
    } else {
        String::new()
    };

    let pkg_version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let product_version = product_version();
    let mut version_parts = pkg_version
        .split('.')
        .map(|part| part.parse::<u16>().unwrap_or(0))
        .chain(std::iter::repeat(0));
    let file_version = format!(
        "{},{},{},{}",
        version_parts.next().unwrap_or(0),
        version_parts.next().unwrap_or(0),
        version_parts.next().unwrap_or(0),
        version_parts.next().unwrap_or(0),
    );

    let rc_content = format!(
        r#"1 ICON "{icon_escaped}"
{manifest_line}

1 VERSIONINFO
FILEVERSION {file_version}
PRODUCTVERSION {file_version}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "FileDescription", "{product_name}\0"
            VALUE "FileVersion", "{pkg_version}\0"
            VALUE "ProductName", "{product_name}\0"
            VALUE "ProductVersion", "{product_version}\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#
    );

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    let rc_path = out_dir.join("orion_studio_resources.rc");
    std::fs::write(&rc_path, rc_content)?;

    if let Ok(toolkit_path) =
        environment_variable("ORION_STUDIO_RC_TOOLKIT_PATH", "ZED_RC_TOOLKIT_PATH")
    {
        let rc_exe = std::path::Path::new(&toolkit_path).join("rc.exe");
        unsafe {
            std::env::set_var("RC", rc_exe);
        }
    }

    embed_resource::compile(&rc_path, embed_resource::NONE).manifest_optional()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{release_channel_from, resource_identity};

    #[test]
    fn build_helper_switches_resource_identity_between_channels() {
        let stable_channel = release_channel_from(|name| {
            (name == "ORION_STUDIO_RELEASE_CHANNEL")
                .then(|| "stable".to_owned())
                .ok_or(std::env::VarError::NotPresent)
        });
        let nightly_channel = release_channel_from(|name| {
            (name == "ORION_STUDIO_RELEASE_CHANNEL")
                .then(|| "nightly".to_owned())
                .ok_or(std::env::VarError::NotPresent)
        });

        assert_eq!(stable_channel, "stable");
        assert_eq!(
            resource_identity(&stable_channel),
            ("app-icon.ico", "Orion Studio")
        );
        assert_eq!(nightly_channel, "nightly");
        assert_eq!(
            resource_identity(&nightly_channel),
            ("app-icon-nightly.ico", "Orion Studio Nightly")
        );
    }

    #[test]
    fn canonical_channel_overrides_legacy_build_input() {
        let channel = release_channel_from(|name| match name {
            "ORION_STUDIO_RELEASE_CHANNEL" => Ok("preview".to_owned()),
            "ZED_RELEASE_CHANNEL" => Ok("stable".to_owned()),
            _ => Err(std::env::VarError::NotPresent),
        });

        assert_eq!(channel, "preview");
    }
}
