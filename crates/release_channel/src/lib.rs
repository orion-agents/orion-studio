//! Provides constructs for the Orion Studio app version and release channel.

#![deny(missing_docs)]

use std::{env, str::FromStr, sync::LazyLock};

use gpui::{App, Global};
use semver::Version;

const ORION_DOCS_URL: &str = "https://orion.dev/docs";

/// Resolves the release channel name from the environment: canonical
/// `ORION_STUDIO_RELEASE_CHANNEL` first, legacy `ZED_RELEASE_CHANNEL` as a
/// fallback, and the compile-time channel when neither is set. Only consulted
/// in debug builds; release builds always use the compile-time channel.
fn release_channel_name_from_env() -> String {
    env::var("ORION_STUDIO_RELEASE_CHANNEL")
        .ok()
        .or_else(|| env::var("ZED_RELEASE_CHANNEL").ok())
        .unwrap_or_else(compile_time_release_channel_name)
}

/// stable | dev | nightly | preview
pub static RELEASE_CHANNEL_NAME: LazyLock<String> = LazyLock::new(|| {
    if cfg!(debug_assertions) {
        release_channel_name_from_env()
    } else {
        compile_time_release_channel_name()
    }
});

/// When a crate in zed is used as a dependency that uses the `crane` nix
/// library, it vendors each crate separately and builds it in isolation, which
/// makes the `include_str!` fail.
///
/// The build script checks for `$ORION_STUDIO_RELEASE_CHANNEL` (canonical) or
/// `$ZED_RELEASE_CHANNEL` (legacy fallback) and emits the `cfg`
#[cfg(__do_not_set_zed_release_channel)]
fn compile_time_release_channel_name() -> String {
    // Canonical first; the legacy `ZED_RELEASE_CHANNEL` is retained as a
    // fallback. The build script only enables this `cfg` when at least one of
    // the two env vars is present, so this `or` is always hit.
    match option_env!("ORION_STUDIO_RELEASE_CHANNEL") {
        Some(channel) => channel.trim().to_string(),
        None => env!("ZED_RELEASE_CHANNEL").trim().to_string(),
    }
}

#[cfg(not(__do_not_set_zed_release_channel))]
fn compile_time_release_channel_name() -> String {
    include_str!("../../zed/RELEASE_CHANNEL").trim().to_string()
}

#[doc(hidden)]
pub static RELEASE_CHANNEL: LazyLock<ReleaseChannel> =
    LazyLock::new(|| match ReleaseChannel::from_str(&RELEASE_CHANNEL_NAME) {
        Ok(channel) => channel,
        _ => panic!("invalid release channel {}", *RELEASE_CHANNEL_NAME),
    });

/// The app identifier for the current release channel, Windows only.
#[cfg(target_os = "windows")]
pub fn app_identifier() -> &'static str {
    match *RELEASE_CHANNEL {
        ReleaseChannel::Dev => "Orion-Studio-Dev",
        ReleaseChannel::Nightly => "Orion-Studio-Nightly",
        ReleaseChannel::Preview => "Orion-Studio-Preview",
        ReleaseChannel::Stable => "Orion-Studio-Stable",
    }
}

/// The Git commit SHA that Orion Studio was built at.
#[derive(Clone, Eq, Debug, PartialEq)]
pub struct AppCommitSha(String);

struct GlobalAppCommitSha(AppCommitSha);

impl Global for GlobalAppCommitSha {}

impl AppCommitSha {
    /// Creates a new [`AppCommitSha`].
    pub fn new(sha: String) -> Self {
        AppCommitSha(sha)
    }

    /// Returns the global [`AppCommitSha`], if one is set.
    pub fn try_global(cx: &App) -> Option<AppCommitSha> {
        cx.try_global::<GlobalAppCommitSha>()
            .map(|sha| sha.0.clone())
    }

    /// Sets the global [`AppCommitSha`].
    pub fn set_global(sha: AppCommitSha, cx: &mut App) {
        cx.set_global(GlobalAppCommitSha(sha))
    }

    /// Returns the full commit SHA.
    pub fn full(&self) -> String {
        self.0.to_string()
    }

    /// Returns the short (7 character) commit SHA.
    pub fn short(&self) -> String {
        self.0.chars().take(7).collect()
    }
}

struct GlobalAppVersion(Version);

impl Global for GlobalAppVersion {}

/// The version of Orion Studio.
pub struct AppVersion;

impl AppVersion {
    /// Load the app version from env.
    pub fn load(
        pkg_version: &str,
        build_id: Option<&str>,
        commit_sha: Option<AppCommitSha>,
    ) -> Version {
        // Canonical env var is `ORION_STUDIO_APP_VERSION`; the legacy
        // `ZED_APP_VERSION` is still read as a compatibility fallback and will
        // be removed after the migration window (S06/S11).
        let mut version: Version = if let Some(from_env) = env::var("ORION_STUDIO_APP_VERSION")
            .ok()
            .or_else(|| env::var("ZED_APP_VERSION").ok())
        {
            from_env.parse().expect("invalid ORION_STUDIO_APP_VERSION")
        } else {
            pkg_version.parse().expect("invalid version in Cargo.toml")
        };
        let mut pre = String::from(RELEASE_CHANNEL.dev_name());

        if let Some(build_id) = build_id {
            pre.push('.');
            pre.push_str(&build_id);
        }

        if let Some(sha) = commit_sha {
            pre.push('.');
            pre.push_str(&sha.0);
        }
        if let Ok(build) = semver::BuildMetadata::new(&pre) {
            version.build = build;
        }

        version
    }

    /// Returns the global version number.
    pub fn global(cx: &App) -> Version {
        if cx.has_global::<GlobalAppVersion>() {
            cx.global::<GlobalAppVersion>().0.clone()
        } else {
            Version::new(0, 0, 0)
        }
    }
}

/// An Orion Studio release channel.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum ReleaseChannel {
    /// The development release channel.
    ///
    /// Used for local debug builds of Orion Studio.
    #[default]
    Dev,

    /// The Nightly release channel.
    Nightly,

    /// The Preview release channel.
    Preview,

    /// The Stable release channel.
    Stable,
}

struct GlobalReleaseChannel(ReleaseChannel);

impl Global for GlobalReleaseChannel {}

/// Initializes the release channel.
pub fn init(app_version: Version, cx: &mut App) {
    cx.set_global(GlobalAppVersion(app_version));
    cx.set_global(GlobalReleaseChannel(*RELEASE_CHANNEL))
}

/// Initializes the release channel for tests that rely on fake release channel.
pub fn init_test(app_version: Version, release_channel: ReleaseChannel, cx: &mut App) {
    cx.set_global(GlobalAppVersion(app_version));
    cx.set_global(GlobalReleaseChannel(release_channel))
}

/// Returns the Orion Studio docs URL for the current release channel for the given
/// `slug`.
pub fn docs_url(slug: &str, cx: &App) -> String {
    ReleaseChannel::try_global(cx)
        .unwrap_or(*RELEASE_CHANNEL)
        .docs_url(slug)
}

impl ReleaseChannel {
    /// All release channels.
    pub const ALL: [ReleaseChannel; 4] = [
        ReleaseChannel::Dev,
        ReleaseChannel::Nightly,
        ReleaseChannel::Preview,
        ReleaseChannel::Stable,
    ];

    /// Returns the global [`ReleaseChannel`].
    pub fn global(cx: &App) -> Self {
        cx.global::<GlobalReleaseChannel>().0
    }

    /// Returns the global [`ReleaseChannel`], if one is set.
    pub fn try_global(cx: &App) -> Option<Self> {
        cx.try_global::<GlobalReleaseChannel>()
            .map(|channel| channel.0)
    }

    /// Returns whether we want to poll for updates for this [`ReleaseChannel`]
    pub fn poll_for_updates(&self) -> bool {
        !matches!(self, ReleaseChannel::Dev)
    }

    /// Returns the display name for this [`ReleaseChannel`].
    pub fn display_name(&self) -> &'static str {
        match self {
            ReleaseChannel::Dev => "Orion Studio Dev",
            ReleaseChannel::Nightly => "Orion Studio Nightly",
            ReleaseChannel::Preview => "Orion Studio Preview",
            ReleaseChannel::Stable => "Orion Studio",
        }
    }

    /// Returns the programmatic name for this [`ReleaseChannel`].
    pub fn dev_name(&self) -> &'static str {
        match self {
            ReleaseChannel::Dev => "dev",
            ReleaseChannel::Nightly => "nightly",
            ReleaseChannel::Preview => "preview",
            ReleaseChannel::Stable => "stable",
        }
    }

    /// Returns the application ID that's used by Wayland as application ID
    /// and WM_CLASS on X11.
    /// This also has to match the bundle identifier for Orion Studio on macOS.
    pub fn app_id(&self) -> &'static str {
        match self {
            ReleaseChannel::Dev => "dev.orion.OrionStudio-Dev",
            ReleaseChannel::Nightly => "dev.orion.OrionStudio-Nightly",
            ReleaseChannel::Preview => "dev.orion.OrionStudio-Preview",
            ReleaseChannel::Stable => "dev.orion.OrionStudio",
        }
    }

    /// Returns the query parameter for this [`ReleaseChannel`].
    pub fn release_query_param(&self) -> Option<&'static str> {
        match self {
            Self::Dev => None,
            Self::Nightly => Some("nightly=1"),
            Self::Preview => Some("preview=1"),
            Self::Stable => None,
        }
    }

    /// Returns the Orion Studio docs URL for this [`ReleaseChannel`] for the given
    /// `slug`.
    pub fn docs_url(&self, slug: &str) -> String {
        let channel_path_segment = match self {
            Self::Dev | Self::Nightly => Some("nightly"),
            Self::Preview => Some("preview"),
            Self::Stable => None,
        };

        match channel_path_segment {
            Some(channel) if slug.is_empty() => format!("{ORION_DOCS_URL}/{channel}"),
            Some(channel) => format!("{ORION_DOCS_URL}/{channel}/{slug}"),
            None if slug.is_empty() => ORION_DOCS_URL.to_string(),
            None => format!("{ORION_DOCS_URL}/{slug}"),
        }
    }
}

/// Error indicating that release channel string does not match any known release channel names.
#[derive(Copy, Clone, Debug, Hash, PartialEq)]
pub struct InvalidReleaseChannel;

impl FromStr for ReleaseChannel {
    type Err = InvalidReleaseChannel;

    fn from_str(channel: &str) -> Result<Self, Self::Err> {
        Ok(match channel {
            "dev" => ReleaseChannel::Dev,
            "nightly" => ReleaseChannel::Nightly,
            "preview" => ReleaseChannel::Preview,
            "stable" => ReleaseChannel::Stable,
            _ => return Err(InvalidReleaseChannel),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ReleaseChannel, release_channel_name_from_env};

    #[test]
    fn canonical_release_channel_env_takes_precedence_over_legacy() {
        unsafe {
            std::env::remove_var("ZED_RELEASE_CHANNEL");
            std::env::remove_var("ORION_STUDIO_RELEASE_CHANNEL");
        }
        // No env override -> falls back to the compile-time `RELEASE_CHANNEL` file ("dev").
        assert_eq!(release_channel_name_from_env(), "dev");

        unsafe {
            std::env::set_var("ZED_RELEASE_CHANNEL", "nightly");
        }
        assert_eq!(release_channel_name_from_env(), "nightly");

        // Canonical overrides legacy when both are present.
        unsafe {
            std::env::set_var("ORION_STUDIO_RELEASE_CHANNEL", "preview");
        }
        assert_eq!(release_channel_name_from_env(), "preview");

        // Removing the canonical one falls back to the still-set legacy value.
        unsafe {
            std::env::remove_var("ORION_STUDIO_RELEASE_CHANNEL");
        }
        assert_eq!(release_channel_name_from_env(), "nightly");

        unsafe {
            std::env::remove_var("ZED_RELEASE_CHANNEL");
        }
    }

    #[test]
    fn test_docs_url_for_release_channel() {
        assert_eq!(
            ReleaseChannel::Dev.docs_url("settings"),
            "https://orion.dev/docs/nightly/settings"
        );
        assert_eq!(
            ReleaseChannel::Nightly.docs_url("settings"),
            "https://orion.dev/docs/nightly/settings"
        );
        assert_eq!(
            ReleaseChannel::Preview.docs_url("settings"),
            "https://orion.dev/docs/preview/settings"
        );
        assert_eq!(
            ReleaseChannel::Stable.docs_url("settings"),
            "https://orion.dev/docs/settings"
        );
    }

    #[test]
    fn test_display_name_for_release_channel() {
        assert_eq!(ReleaseChannel::Dev.display_name(), "Orion Studio Dev");
        assert_eq!(
            ReleaseChannel::Nightly.display_name(),
            "Orion Studio Nightly"
        );
        assert_eq!(
            ReleaseChannel::Preview.display_name(),
            "Orion Studio Preview"
        );
        assert_eq!(ReleaseChannel::Stable.display_name(), "Orion Studio");
    }
}
