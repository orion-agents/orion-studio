//! Contains helper functions for constructing URLs to Orion Studio pages.
//!
//! These URLs will adapt to the configured server URL in order to construct
//! links appropriate for the environment (e.g., by linking to a local copy of
//! orion.dev in development).

use gpui::App;
use settings::Settings;

use crate::ClientSettings;

const PREVIEW_LIMITATIONS_URL: &str = "https://github.com/orion-agents/orion-studio/blob/main/docs/releases/preview-macos-arm64.md#known-limitations";

fn server_url(cx: &App) -> &str {
    &ClientSettings::get_global(cx).server_url
}

/// Returns the URL to the Orion account page.
pub fn account_url(cx: &App) -> String {
    if release_channel::hosted_services_available(cx) {
        format!("{server_url}/account", server_url = server_url(cx))
    } else {
        PREVIEW_LIMITATIONS_URL.to_string()
    }
}

/// Returns the URL to the Orion trial page.
pub fn start_trial_url(cx: &App) -> String {
    if release_channel::hosted_services_available(cx) {
        format!(
            "{server_url}/account/start-trial",
            server_url = server_url(cx)
        )
    } else {
        PREVIEW_LIMITATIONS_URL.to_string()
    }
}

/// Returns the URL to the Orion upgrade page.
pub fn upgrade_to_orion_pro_url(cx: &App) -> String {
    if release_channel::hosted_services_available(cx) {
        format!("{server_url}/account/upgrade", server_url = server_url(cx))
    } else {
        PREVIEW_LIMITATIONS_URL.to_string()
    }
}

/// Returns the URL to Orion's terms of service.
pub fn terms_of_service(cx: &App) -> String {
    if release_channel::hosted_services_available(cx) {
        format!("{server_url}/terms-of-service", server_url = server_url(cx))
    } else {
        "https://github.com/orion-agents/orion-studio/blob/main/legal/terms.md".to_string()
    }
}

/// Returns the URL to Orion AI's privacy and security docs.
pub fn ai_privacy_and_security(cx: &App) -> String {
    release_channel::docs_url("ai/privacy-and-security", cx)
}

/// Returns the URL to Orion's edit prediction documentation.
pub fn edit_prediction_docs(cx: &App) -> String {
    release_channel::docs_url("ai/edit-prediction", cx)
}

pub fn skills_docs(cx: &App) -> String {
    release_channel::docs_url("ai/skills", cx)
}

/// Returns the URL to Orion's Agent sandboxing documentation.
///
/// Pass `section` to deep-link to a specific section anchor on the page (for
/// example, `Some("installing-bubblewrap")`); pass `None` to link to the top of
/// the page.
///
/// Unlike the account/app links above, this uses the public source-document
/// location rather than the configured `server_url`.
pub fn sandboxing_docs(section: Option<&str>, cx: &App) -> String {
    let base = release_channel::docs_url("ai/sandboxing", cx);
    match section {
        Some(section) => format!("{base}#{}", sandboxing_github_anchor(section)),
        None => base,
    }
}

fn sandboxing_github_anchor(section: &str) -> &str {
    match section {
        "installing-bubblewrap" => "installing-bubblewrap-installing-bubblewrap",
        "installing-bubblewrap-ubuntu" => {
            "ubuntu-specific-requirements-installing-bubblewrap-ubuntu"
        }
        "linux" => "linux-linux",
        "persistent-sandbox-permissions" => {
            "persistent-sandbox-permissions-persistent-sandbox-permissions"
        }
        "windows" => "windows-windows",
        section => section,
    }
}
pub fn llm_provider_docs(cx: &App) -> String {
    release_channel::docs_url("ai/llm-providers", cx)
}

/// Returns the URL to Orion's ACP registry blog post.
pub fn acp_registry_blog(cx: &App) -> String {
    if release_channel::hosted_services_available(cx) {
        format!(
            "{server_url}/blog/acp-registry",
            server_url = server_url(cx)
        )
    } else {
        release_channel::docs_url("ai/external-agents", cx)
    }
}

pub fn shared_agent_thread_url(session_id: &str) -> String {
    format!("orion://agent/shared/{}", session_id)
}

#[cfg(test)]
mod tests {
    use super::{sandboxing_github_anchor, shared_agent_thread_url};

    #[test]
    fn shared_agent_thread_uses_canonical_orion_scheme() {
        assert_eq!(
            shared_agent_thread_url("session-123"),
            "orion://agent/shared/session-123"
        );
    }

    #[test]
    fn sandboxing_sections_use_github_heading_anchors() {
        assert_eq!(
            sandboxing_github_anchor("installing-bubblewrap"),
            "installing-bubblewrap-installing-bubblewrap"
        );
        assert_eq!(
            sandboxing_github_anchor("installing-bubblewrap-ubuntu"),
            "ubuntu-specific-requirements-installing-bubblewrap-ubuntu"
        );
        assert_eq!(sandboxing_github_anchor("linux"), "linux-linux");
        assert_eq!(sandboxing_github_anchor("windows"), "windows-windows");
        assert_eq!(sandboxing_github_anchor("future-section"), "future-section");
    }
}
