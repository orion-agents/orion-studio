use std::{net::IpAddr, sync::Arc};

use db::kvp::KeyValueStore;
use gpui::App;
use http_client::HttpClient;
use release_channel::AppVersion;
use thiserror::Error;

use ::orion_code_update::{
    ED25519_PUBLIC_KEY_BYTES, OrionCodeIndexVerifier, OrionCodeUpdateContractError,
};

use crate::{
    orion_code_update_activation::OrionCodeActivationCoordinator,
    orion_code_update_coordinator::OrionCodeUpdateCoordinator,
    orion_code_update_feed::{
        OrionCodeHttpFeedError, OrionCodeHttpUpdateFeed, OrionCodeUpdateFeedConfiguration,
        OrionCodeVerifiedUpdateFeed,
    },
    orion_code_update_installer::{OrionCodeArchiveInstaller, OrionCodeArchiveInstallerError},
    orion_code_update_pipeline::{OrionCodeArchiveInstallerHandle, OrionCodeUpdatePipeline},
    orion_code_update_platform::{OrionCodeMacPlatformTrust, OrionCodeMacPlatformVerifier},
    orion_code_update_preflight::OrionCodeManagedPreflight,
};

#[derive(Clone, Debug)]
pub struct OrionCodeManagedUpdateConfiguration {
    index_url: String,
    signature_url: String,
    feed_hosts: Vec<String>,
    archive_hosts: Vec<String>,
    trusted_keys: Vec<(String, [u8; ED25519_PUBLIC_KEY_BYTES])>,
    mac_platform_trust: OrionCodeMacPlatformTrust,
}

pub(crate) fn configure_embedded_orion_code_managed_update(
    cx: &mut App,
) -> Result<bool, OrionCodeManagedUpdateConfigurationError> {
    let Some(configuration) = embedded_orion_code_managed_update_configuration() else {
        return Ok(false);
    };
    configuration.configure(cx)?;
    Ok(true)
}

fn embedded_orion_code_managed_update_configuration() -> Option<OrionCodeManagedUpdateConfiguration>
{
    None
}

impl OrionCodeManagedUpdateConfiguration {
    pub fn new(
        index_url: impl Into<String>,
        signature_url: impl Into<String>,
        feed_hosts: impl IntoIterator<Item = String>,
        archive_hosts: impl IntoIterator<Item = String>,
        trusted_keys: impl IntoIterator<Item = (String, [u8; ED25519_PUBLIC_KEY_BYTES])>,
        mac_platform_trust: OrionCodeMacPlatformTrust,
    ) -> Result<Self, OrionCodeManagedUpdateConfigurationError> {
        let feed_hosts = validate_production_hosts(feed_hosts, "update feed")?;
        let archive_hosts = validate_production_hosts(archive_hosts, "archive")?;
        let trusted_keys = trusted_keys.into_iter().collect::<Vec<_>>();
        if trusted_keys
            .iter()
            .any(|(key_id, _)| is_test_key_id(key_id))
        {
            return Err(OrionCodeManagedUpdateConfigurationError::TestTrustMaterial);
        }
        let index_url = index_url.into();
        let signature_url = signature_url.into();
        OrionCodeUpdateFeedConfiguration::new(
            &index_url,
            &signature_url,
            feed_hosts.iter().cloned(),
        )?;
        OrionCodeIndexVerifier::new(trusted_keys.iter().cloned(), archive_hosts.iter().cloned())?;
        Ok(Self {
            index_url,
            signature_url,
            feed_hosts,
            archive_hosts,
            trusted_keys,
            mac_platform_trust,
        })
    }

    pub fn configure(self, cx: &mut App) -> Result<(), OrionCodeManagedUpdateConfigurationError> {
        let http_client = cx.http_client();
        self.configure_with_http_client(http_client, cx)
    }

    fn configure_with_http_client(
        self,
        http_client: Arc<dyn HttpClient>,
        cx: &mut App,
    ) -> Result<(), OrionCodeManagedUpdateConfigurationError> {
        let feed_configuration = OrionCodeUpdateFeedConfiguration::new(
            &self.index_url,
            &self.signature_url,
            self.feed_hosts,
        )?;
        let verifier = Arc::new(OrionCodeIndexVerifier::new(
            self.trusted_keys,
            self.archive_hosts.iter().cloned(),
        )?);
        let managed_root = paths::external_agents_dir()
            .join("registry")
            .join(project::agent_registry_store::ORION_CODE_AGENT_ID);

        let root_preparer = OrionCodeArchiveInstaller::new(
            &managed_root,
            http_client.clone(),
            self.archive_hosts.iter().cloned(),
            Arc::new(OrionCodeMacPlatformVerifier::new(
                self.mac_platform_trust.clone(),
            )),
            None,
        )?;
        drop(root_preparer);

        let preflight = Arc::new(
            OrionCodeManagedPreflight::new(
                &managed_root,
                cx.background_executor().clone(),
                AppVersion::global(cx).to_string(),
            )
            .map_err(OrionCodeManagedUpdateConfigurationError::Preflight)?,
        );
        let installer = Arc::new(OrionCodeArchiveInstaller::new(
            managed_root,
            http_client.clone(),
            self.archive_hosts,
            Arc::new(OrionCodeMacPlatformVerifier::new(self.mac_platform_trust)),
            Some(preflight.clone()),
        )?);
        let feed = Arc::new(OrionCodeVerifiedUpdateFeed::new(
            Arc::new(OrionCodeHttpUpdateFeed::new(
                http_client,
                feed_configuration,
            )),
            verifier,
            KeyValueStore::global(cx).into(),
        ));

        if let Some(coordinator) = OrionCodeUpdateCoordinator::try_global(cx) {
            coordinator.update(cx, |coordinator, cx| coordinator.configure_feed(feed, cx));
        } else {
            OrionCodeUpdateCoordinator::init_global_with_feed(feed, cx);
        };
        if let Some(activation) = OrionCodeActivationCoordinator::try_global(cx) {
            activation.update(cx, |activation, cx| {
                activation.configure_health_check(preflight, cx)
            });
        } else {
            OrionCodeActivationCoordinator::init_global_with_health_check(preflight, cx);
        };
        if OrionCodeUpdatePipeline::try_global(cx).is_some() {
            OrionCodeUpdatePipeline::global(cx).update(cx, |pipeline, cx| {
                pipeline.configure_installer(
                    Arc::new(OrionCodeArchiveInstallerHandle::new(installer)),
                    cx,
                )
            });
        } else {
            OrionCodeUpdatePipeline::init_global_with_installer(
                Some(Arc::new(OrionCodeArchiveInstallerHandle::new(installer))),
                cx,
            );
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum OrionCodeManagedUpdateConfigurationError {
    #[error("Orion Code production {kind} host is invalid")]
    InvalidProductionHost { kind: &'static str },
    #[error("Orion Code production configuration cannot contain test trust material")]
    TestTrustMaterial,
    #[error(transparent)]
    Feed(#[from] OrionCodeHttpFeedError),
    #[error(transparent)]
    Contract(#[from] OrionCodeUpdateContractError),
    #[error(transparent)]
    Installer(#[from] OrionCodeArchiveInstallerError),
    #[error("failed to configure isolated Orion Code ACP preflight: {0}")]
    Preflight(String),
}

fn validate_production_hosts(
    hosts: impl IntoIterator<Item = String>,
    kind: &'static str,
) -> Result<Vec<String>, OrionCodeManagedUpdateConfigurationError> {
    let mut hosts = hosts
        .into_iter()
        .map(|host| host.to_ascii_lowercase())
        .collect::<Vec<_>>();
    hosts.sort();
    hosts.dedup();
    if hosts.is_empty() || hosts.iter().any(|host| !is_production_host(host)) {
        return Err(OrionCodeManagedUpdateConfigurationError::InvalidProductionHost { kind });
    }
    Ok(hosts)
}

fn is_production_host(host: &str) -> bool {
    if host.is_empty()
        || host.parse::<IpAddr>().is_ok()
        || matches!(
            host,
            "localhost" | "example.com" | "example.net" | "example.org"
        )
    {
        return false;
    }
    ![
        ".localhost",
        ".invalid",
        ".test",
        ".example",
        ".example.com",
        ".example.net",
        ".example.org",
    ]
    .iter()
    .any(|suffix| host.ends_with(suffix))
}

fn is_test_key_id(key_id: &str) -> bool {
    key_id
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|segment| {
            segment.eq_ignore_ascii_case("test") || segment.eq_ignore_ascii_case("fixture")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_VERIFYING_KEY: [u8; ED25519_PUBLIC_KEY_BYTES] = [
        135, 227, 133, 164, 82, 54, 161, 5, 252, 128, 242, 104, 118, 17, 112, 152, 94, 104, 34, 61,
        225, 122, 67, 36, 143, 37, 52, 191, 39, 140, 148, 206,
    ];

    #[test]
    fn production_configuration_rejects_placeholder_hosts_and_test_keys() {
        let trust = OrionCodeMacPlatformTrust::new("ABCDE12345", "com.orion.code.sidecar")
            .expect("valid platform trust");
        assert!(
            OrionCodeManagedUpdateConfiguration::new(
                "https://updates.example.invalid/index.json",
                "https://updates.example.invalid/index.json.sig",
                ["updates.example.invalid".to_string()],
                ["downloads.example.invalid".to_string()],
                [("release-2026-01".to_string(), [1; ED25519_PUBLIC_KEY_BYTES])],
                trust.clone(),
            )
            .is_err()
        );
        assert!(
            OrionCodeManagedUpdateConfiguration::new(
                "https://updates.orion-agents.com/index.json",
                "https://updates.orion-agents.com/index.json.sig",
                ["updates.orion-agents.com".to_string()],
                ["downloads.orion-agents.com".to_string()],
                [(
                    "fixture-release-key".to_string(),
                    [1; ED25519_PUBLIC_KEY_BYTES]
                )],
                trust,
            )
            .is_err()
        );
        assert!(
            OrionCodeManagedUpdateConfiguration::new(
                "https://updates.orion-agents.com/index.json",
                "https://updates.orion-agents.com/index.json.sig",
                ["updates.orion-agents.com".to_string()],
                ["downloads.orion-agents.com".to_string()],
                [("orion-release-2026-01".to_string(), VALID_VERIFYING_KEY)],
                OrionCodeMacPlatformTrust::new("ABCDE12345", "com.orion.code.sidecar",)
                    .expect("valid platform trust"),
            )
            .is_ok()
        );
    }

    #[test]
    fn embedded_release_configuration_is_fail_closed_until_values_are_frozen() {
        assert!(embedded_orion_code_managed_update_configuration().is_none());
    }
}
