pub mod api;
pub mod auth;
pub mod db;
pub mod entities;
pub mod env;
pub mod executor;
pub mod rpc;
pub mod services;

use anyhow::{Context as _, ensure};
use aws_config::{BehaviorVersion, Region};
use axum::{
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use db::Database;
use executor::Executor;
use serde::Deserialize;
use std::{ops::Deref, sync::Arc};
use util::ResultExt;

use crate::services::{CloudUserService, UserService};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REVISION: Option<&'static str> = option_env!("GITHUB_SHA");

// Orion clients still use these names on the wire. Removing or renaming them
// before all supported clients negotiate Orion headers would break upgrades.
pub const LEGACY_ZED_CHECKSUM_HEADER: &str = "x-zed-checksum";
pub const LEGACY_ZED_SYSTEM_ID_HEADER: &str = "x-zed-system-id";
pub const LEGACY_ZED_PROTOCOL_VERSION_HEADER: &str = "x-zed-protocol-version";
pub const LEGACY_ZED_APP_VERSION_HEADER: &str = "x-zed-app-version";
pub const LEGACY_ZED_RELEASE_CHANNEL_HEADER: &str = "x-zed-release-channel";
pub const LEGACY_ZED_HEADER_RETIREMENT_CONTRACT: &str =
    "Retire only after every supported Orion client sends negotiated Orion headers";

pub type LegacyEditorChecksumHeader = api::events::ZedChecksumHeader;
pub type LegacyEditorSystemIdHeader = api::SystemIdHeader;
pub type LegacyEditorProtocolVersionHeader = rpc::ProtocolVersion;
pub type LegacyEditorAppVersionHeader = rpc::AppVersionHeader;
pub type LegacyEditorReleaseChannelHeader = rpc::ReleaseChannelHeader;
pub type LegacyEditorVersion = rpc::ZedVersion;

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub enum Error {
    Http(StatusCode, String, HeaderMap),
    Database(sea_orm::error::DbErr),
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for Error {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}

impl From<sea_orm::error::DbErr> for Error {
    fn from(error: sea_orm::error::DbErr) -> Self {
        Self::Database(error)
    }
}

impl From<axum::Error> for Error {
    fn from(error: axum::Error) -> Self {
        Self::Internal(error.into())
    }
}

impl From<axum::http::Error> for Error {
    fn from(error: axum::http::Error) -> Self {
        Self::Internal(error.into())
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Internal(error.into())
    }
}

impl Error {
    fn http(code: StatusCode, message: String) -> Self {
        Self::Http(code, message, HeaderMap::default())
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        match self {
            Error::Http(code, message, headers) => {
                log::error!("HTTP error {code}: {message}");
                (code, headers, message).into_response()
            }
            Error::Database(error) => {
                log::error!(
                    "HTTP error {}: {error:?}",
                    StatusCode::INTERNAL_SERVER_ERROR
                );
                (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}")).into_response()
            }
            Error::Internal(error) => {
                log::error!(
                    "HTTP error {}: {error:?}",
                    StatusCode::INTERNAL_SERVER_ERROR
                );
                (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}")).into_response()
            }
        }
    }
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Http(code, message, _headers) => (code, message).fmt(f),
            Error::Database(error) => error.fmt(f),
            Error::Internal(error) => error.fmt(f),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Http(code, message, _) => write!(f, "{code}: {message}"),
            Error::Database(error) => error.fmt(f),
            Error::Internal(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OrionStudioEnvironment {
    Development,
    Staging,
    Production,
    #[cfg(feature = "test-support")]
    Test,
}

impl OrionStudioEnvironment {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Staging => "staging",
            Self::Production => "production",
            #[cfg(feature = "test-support")]
            Self::Test => "test",
        }
    }
}

impl AsRef<str> for OrionStudioEnvironment {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for OrionStudioEnvironment {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl std::fmt::Display for OrionStudioEnvironment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Deserialize)]
pub struct Config {
    pub http_port: u16,
    pub database_url: String,
    pub database_max_connections: u32,
    pub livekit_server: Option<String>,
    pub livekit_key: Option<String>,
    pub livekit_secret: Option<String>,
    pub rust_log: Option<String>,
    pub log_json: Option<bool>,
    pub blob_store_url: Option<String>,
    pub blob_store_region: Option<String>,
    pub blob_store_access_key: Option<String>,
    pub blob_store_secret_key: Option<String>,
    pub blob_store_bucket: Option<String>,
    pub kinesis_region: Option<String>,
    pub kinesis_stream: Option<String>,
    pub kinesis_access_key: Option<String>,
    pub kinesis_secret_key: Option<String>,
    pub orion_studio_environment: OrionStudioEnvironment,
    pub orion_studio_web_url: String,
    pub orion_studio_cloud_url: String,
    pub orion_studio_cloud_internal_api_key: String,
    pub orion_studio_client_checksum_seed: Option<String>,
}

impl Config {
    pub fn is_development(&self) -> bool {
        self.orion_studio_environment == OrionStudioEnvironment::Development
    }

    /// Returns the base `orion.dev` URL.
    pub fn orion_dev_url(&self) -> &str {
        &self.orion_studio_web_url
    }

    /// Returns the base Orion Cloud URL.
    pub fn orion_cloud_url(&self) -> &str {
        &self.orion_studio_cloud_url
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        let web_url = validate_base_url("ORION_STUDIO_WEB_URL", &self.orion_studio_web_url)?;
        let cloud_url = validate_base_url("ORION_STUDIO_CLOUD_URL", &self.orion_studio_cloud_url)?;
        ensure!(
            self.orion_studio_web_url != self.orion_studio_cloud_url,
            "ORION_STUDIO_WEB_URL and ORION_STUDIO_CLOUD_URL must identify different services"
        );
        if !self.is_development() {
            ensure!(
                web_url.scheme() == "https" && cloud_url.scheme() == "https",
                "staging and production Orion service URLs must use https"
            );
        }
        Ok(())
    }

    #[cfg(feature = "test-support")]
    pub fn test() -> Self {
        Self {
            http_port: 0,
            database_url: "".into(),
            database_max_connections: 0,
            livekit_server: None,
            livekit_key: None,
            livekit_secret: None,
            rust_log: None,
            log_json: None,
            orion_studio_environment: OrionStudioEnvironment::Test,
            orion_studio_web_url: "http://orion.test".into(),
            orion_studio_cloud_url: "http://cloud.orion.test".into(),
            orion_studio_cloud_internal_api_key: "test-internal-api-key".into(),
            blob_store_url: None,
            blob_store_region: None,
            blob_store_access_key: None,
            blob_store_secret_key: None,
            blob_store_bucket: None,
            orion_studio_client_checksum_seed: None,
            kinesis_region: None,
            kinesis_access_key: None,
            kinesis_secret_key: None,
            kinesis_stream: None,
        }
    }
}

fn validate_base_url(variable_name: &str, value: &str) -> anyhow::Result<reqwest::Url> {
    let parsed = reqwest::Url::parse(value)
        .with_context(|| format!("{variable_name} must be an absolute HTTP(S) URL"))?;
    ensure!(
        matches!(parsed.scheme(), "http" | "https"),
        "{variable_name} must use http or https"
    );
    ensure!(
        parsed.host_str().is_some(),
        "{variable_name} must include a host"
    );
    ensure!(
        parsed.username().is_empty() && parsed.password().is_none(),
        "{variable_name} must not contain credentials"
    );
    ensure!(
        parsed.query().is_none() && parsed.fragment().is_none(),
        "{variable_name} must not contain a query or fragment"
    );
    ensure!(
        parsed.path() == "/",
        "{variable_name} must be an origin without a path"
    );
    ensure!(
        !value.ends_with('/'),
        "{variable_name} must not end with a slash"
    );
    Ok(parsed)
}

/// The service mode that collab should run in.
#[derive(Debug, PartialEq, Eq, Clone, Copy, strum::Display)]
#[strum(serialize_all = "snake_case")]
pub enum ServiceMode {
    Api,
    Collab,
    All,
}

impl ServiceMode {
    pub fn is_collab(&self) -> bool {
        matches!(self, Self::Collab | Self::All)
    }

    pub fn is_api(&self) -> bool {
        matches!(self, Self::Api | Self::All)
    }
}

pub struct AppState {
    pub db: Arc<Database>,
    pub http_client: Option<reqwest::Client>,
    pub livekit_client: Option<Arc<dyn livekit_api::Client>>,
    pub blob_store_client: Option<aws_sdk_s3::Client>,
    pub executor: Executor,
    pub kinesis_client: Option<::aws_sdk_kinesis::Client>,
    pub user_service: Arc<dyn UserService>,
    pub config: Config,
}

impl AppState {
    pub async fn new(config: Config, executor: Executor) -> Result<Arc<Self>> {
        config.validate()?;
        let mut db_options = db::ConnectOptions::new(config.database_url.clone());
        db_options.max_connections(config.database_max_connections);
        let mut db = Database::new(db_options).await?;
        db.initialize_notification_kinds().await?;

        let livekit_client = if let Some(((server, key), secret)) = config
            .livekit_server
            .as_ref()
            .zip(config.livekit_key.as_ref())
            .zip(config.livekit_secret.as_ref())
        {
            Some(Arc::new(livekit_api::LiveKitClient::new(
                server.clone(),
                key.clone(),
                secret.clone(),
            )) as Arc<dyn livekit_api::Client>)
        } else {
            None
        };

        let user_agent = format!("Collab/{VERSION} ({})", REVISION.unwrap_or("unknown"));
        let http_client = reqwest::Client::builder()
            .user_agent(user_agent)
            .build()
            .context("failed to construct HTTP client")?;

        let db = Arc::new(db);
        let this = Self {
            db: db.clone(),
            http_client: Some(http_client.clone()),
            livekit_client,
            blob_store_client: build_blob_store_client(&config).await.log_err(),
            executor,
            kinesis_client: if config.kinesis_access_key.is_some() {
                build_kinesis_client(&config).await.log_err()
            } else {
                None
            },
            user_service: Arc::new(CloudUserService::new(
                http_client,
                config.orion_cloud_url().to_string(),
                config.orion_studio_cloud_internal_api_key.clone(),
            )),
            config,
        };
        Ok(Arc::new(this))
    }
}

async fn build_blob_store_client(config: &Config) -> anyhow::Result<aws_sdk_s3::Client> {
    let keys = aws_sdk_s3::config::Credentials::new(
        config
            .blob_store_access_key
            .clone()
            .context("missing blob_store_access_key")?,
        config
            .blob_store_secret_key
            .clone()
            .context("missing blob_store_secret_key")?,
        None,
        None,
        "env",
    );

    let s3_config = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(
            config
                .blob_store_url
                .as_ref()
                .context("missing blob_store_url")?,
        )
        .region(Region::new(
            config
                .blob_store_region
                .clone()
                .context("missing blob_store_region")?,
        ))
        .credentials_provider(keys)
        .load()
        .await;

    Ok(aws_sdk_s3::Client::new(&s3_config))
}

async fn build_kinesis_client(config: &Config) -> anyhow::Result<aws_sdk_kinesis::Client> {
    let keys = aws_sdk_s3::config::Credentials::new(
        config
            .kinesis_access_key
            .clone()
            .context("missing kinesis_access_key")?,
        config
            .kinesis_secret_key
            .clone()
            .context("missing kinesis_secret_key")?,
        None,
        None,
        "env",
    );

    let kinesis_config = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(
            config
                .kinesis_region
                .clone()
                .context("missing kinesis_region")?,
        ))
        .credentials_provider(keys)
        .load()
        .await;

    Ok(aws_sdk_kinesis::Client::new(&kinesis_config))
}
