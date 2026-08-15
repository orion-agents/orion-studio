use anyhow::{Context as _, anyhow};
use axum::headers::HeaderMapExt;
use axum::{
    Extension, Router,
    extract::MatchedPath,
    http::{Request, Response},
    routing::get,
};

use collab::api::CloudflareIpCountryHeader;
use collab::{
    AppState, Config, Result, api::fetch_extensions_from_blob_store_periodically, db, env,
    executor::Executor,
};
use collab::{REVISION, ServiceMode, VERSION};
use db::Database;
use std::{
    collections::BTreeMap,
    env::args,
    net::{SocketAddr, TcpListener},
    sync::Arc,
    time::Duration,
};
#[cfg(unix)]
use tokio::signal::unix::SignalKind;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{
    Layer, filter::EnvFilter, fmt::format::JsonFields, util::SubscriberInitExt,
};
use util::ResultExt as _;

const LEGACY_CONFIG_ENVIRONMENT_VARIABLES: [(&str, &str); 3] = [
    ("ORION_STUDIO_ENVIRONMENT", "ZED_ENVIRONMENT"),
    (
        "ORION_STUDIO_CLOUD_INTERNAL_API_KEY",
        "ZED_CLOUD_INTERNAL_API_KEY",
    ),
    (
        "ORION_STUDIO_CLIENT_CHECKSUM_SEED",
        "ZED_CLIENT_CHECKSUM_SEED",
    ),
];

fn config_from_environment() -> Result<Config, envy::Error> {
    config_from_environment_variables(std::env::vars())
}

fn config_from_environment_variables(
    variables: impl IntoIterator<Item = (String, String)>,
) -> Result<Config, envy::Error> {
    let mut variables = variables.into_iter().collect::<BTreeMap<_, _>>();
    let legacy_environment = if variables.contains_key("ORION_STUDIO_ENVIRONMENT") {
        None
    } else {
        variables.get("ZED_ENVIRONMENT").cloned()
    };

    for (canonical_name, legacy_name) in LEGACY_CONFIG_ENVIRONMENT_VARIABLES {
        if !variables.contains_key(canonical_name) {
            if let Some(legacy_value) = variables.get(legacy_name).cloned() {
                variables.insert(canonical_name.to_owned(), legacy_value);
            }
        }
        variables.remove(legacy_name);
    }

    if let Some(legacy_environment) = legacy_environment {
        let web_url_missing = !variables.contains_key("ORION_STUDIO_WEB_URL");
        let cloud_url_missing = !variables.contains_key("ORION_STUDIO_CLOUD_URL");

        if web_url_missing && cloud_url_missing {
            let endpoints = match legacy_environment.as_str() {
                "development" => Some(("http://localhost:3000", "http://localhost:8787")),
                "staging" => Some(("https://staging.orion.dev", "https://cloud.orion.dev")),
                "production" => Some(("https://orion.dev", "https://cloud.orion.dev")),
                _ => None,
            };

            if let Some((web_url, cloud_url)) = endpoints {
                variables.insert("ORION_STUDIO_WEB_URL".to_owned(), web_url.to_owned());
                variables.insert("ORION_STUDIO_CLOUD_URL".to_owned(), cloud_url.to_owned());
            }
        }
    }

    envy::from_iter(variables)
}

#[expect(clippy::result_large_err)]
#[tokio::main]
async fn main() -> Result<()> {
    if let Err(error) = env::load_dotenv() {
        eprintln!(
            "error loading .env.toml (this is expected in production): {}",
            error
        );
    }

    let mut args = args().skip(1);
    match args.next().as_deref() {
        Some("version") => {
            println!("collab v{} ({})", VERSION, REVISION.unwrap_or("unknown"));
        }
        Some("serve") => {
            let mode = match args.next().as_deref() {
                Some("collab") => ServiceMode::Collab,
                Some("api") => ServiceMode::Api,
                Some("all") => ServiceMode::All,
                _ => {
                    return Err(anyhow!("usage: collab <version | serve <api|collab|all>>"))?;
                }
            };

            let config = config_from_environment().context("error loading config")?;
            config.validate()?;
            init_tracing(&config);
            init_panic_hook();

            let mut app = Router::new()
                .route("/", get(handle_root))
                .route("/healthz", get(handle_liveness_probe))
                .layer(Extension(mode));

            let listener = TcpListener::bind(format!("0.0.0.0:{}", config.http_port))
                .expect("failed to bind TCP listener");

            let mut on_shutdown = None;

            if mode.is_collab() || mode.is_api() {
                setup_app_database(&config).await?;

                let state = AppState::new(config, Executor::Production).await?;

                if mode.is_collab() {
                    let epoch = state
                        .db
                        .create_server(&state.config.orion_studio_environment)
                        .await?;
                    let rpc_server = collab::rpc::Server::new(epoch, state.clone());
                    rpc_server.start().await?;

                    app = app.merge(collab::rpc::routes(rpc_server.clone()));

                    on_shutdown = Some(Box::new(move || rpc_server.teardown()));
                }

                if mode.is_api() {
                    fetch_extensions_from_blob_store_periodically(state.clone());

                    app = app
                        .merge(collab::api::events::router())
                        .merge(collab::api::extensions::router())
                }

                app = app.layer(Extension(state.clone()));
            }

            app = app.layer(
                TraceLayer::new_for_http()
                    .make_span_with(|request: &Request<_>| {
                        let matched_path = request
                            .extensions()
                            .get::<MatchedPath>()
                            .map(MatchedPath::as_str);

                        let geoip_country_code = request
                            .headers()
                            .typed_get::<CloudflareIpCountryHeader>()
                            .map(|header| header.to_string());

                        tracing::info_span!(
                            "http_request",
                            method = ?request.method(),
                            matched_path,
                            geoip_country_code,
                            user_id = tracing::field::Empty,
                            login = tracing::field::Empty,
                            authn.jti = tracing::field::Empty,
                            is_staff = tracing::field::Empty
                        )
                    })
                    .on_response(
                        |response: &Response<_>, latency: Duration, _: &tracing::Span| {
                            let duration_ms = latency.as_micros() as f64 / 1000.;
                            tracing::info!(
                                duration_ms,
                                status = response.status().as_u16(),
                                "finished processing request"
                            );
                        },
                    ),
            );

            #[cfg(unix)]
            let signal = async move {
                let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate())
                    .expect("failed to listen for interrupt signal");
                let mut sigint = tokio::signal::unix::signal(SignalKind::interrupt())
                    .expect("failed to listen for interrupt signal");
                let sigterm = sigterm.recv();
                let sigint = sigint.recv();
                futures::pin_mut!(sigterm, sigint);
                futures::future::select(sigterm, sigint).await;
            };

            #[cfg(windows)]
            let signal = async move {
                // todo(windows):
                // `ctrl_close` does not work well, because tokio's signal handler always returns soon,
                // but system terminates the application soon after returning CTRL+CLOSE handler.
                // So we should implement blocking handler to treat CTRL+CLOSE signal.
                let mut ctrl_break = tokio::signal::windows::ctrl_break()
                    .expect("failed to listen for interrupt signal");
                let mut ctrl_c = tokio::signal::windows::ctrl_c()
                    .expect("failed to listen for interrupt signal");
                let ctrl_break = ctrl_break.recv();
                let ctrl_c = ctrl_c.recv();
                futures::pin_mut!(ctrl_break, ctrl_c);
                futures::future::select(ctrl_break, ctrl_c).await;
            };

            axum::Server::from_tcp(listener)
                .map_err(|e| anyhow!(e))?
                .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                .with_graceful_shutdown(async move {
                    signal.await;
                    tracing::info!("Received interrupt signal");

                    if let Some(on_shutdown) = on_shutdown {
                        on_shutdown();
                    }
                })
                .await
                .map_err(|e| anyhow!(e))?;
        }
        _ => {
            Err(anyhow!(
                "usage: collab <version | migrate | seed | serve <api|collab|llm|all>>"
            ))?;
        }
    }
    Ok(())
}

async fn setup_app_database(config: &Config) -> Result<()> {
    let db_options = db::ConnectOptions::new(config.database_url.clone());
    let mut db = Database::new(db_options).await?;

    db.initialize_notification_kinds().await?;

    Ok(())
}

async fn handle_root(Extension(mode): Extension<ServiceMode>) -> String {
    format!(
        "orion-studio:{mode} v{VERSION} ({})",
        REVISION.unwrap_or("unknown")
    )
}

async fn handle_liveness_probe(app_state: Option<Extension<Arc<AppState>>>) -> Result<String> {
    if let Some(state) = app_state {
        state.db.project_count_excluding_admins().await?;
    }

    Ok("ok".to_string())
}

pub fn init_tracing(config: &Config) -> Option<()> {
    use std::str::FromStr;
    use tracing_subscriber::layer::SubscriberExt;

    let filter = EnvFilter::from_str(config.rust_log.as_deref()?).log_err()?;

    tracing_subscriber::registry()
        .with(if config.log_json.unwrap_or(false) {
            Box::new(
                tracing_subscriber::fmt::layer()
                    .fmt_fields(JsonFields::default())
                    .event_format(
                        tracing_subscriber::fmt::format()
                            .json()
                            .flatten_event(true)
                            .with_span_list(false),
                    )
                    .with_filter(filter),
            ) as Box<dyn Layer<_> + Send + Sync>
        } else {
            Box::new(
                tracing_subscriber::fmt::layer()
                    .event_format(tracing_subscriber::fmt::format().pretty())
                    .with_filter(filter),
            )
        })
        .init();

    None
}

fn init_panic_hook() {
    std::panic::set_hook(Box::new(move |panic_info| {
        let panic_message = match panic_info.payload().downcast_ref::<&'static str>() {
            Some(message) => *message,
            None => match panic_info.payload().downcast_ref::<String>() {
                Some(message) => message.as_str(),
                None => "Box<Any>",
            },
        };
        let backtrace = std::backtrace::Backtrace::force_capture();
        let location = panic_info
            .location()
            .map(|loc| format!("{}:{}", loc.file(), loc.line()));
        tracing::error!(panic = true, ?location, %panic_message, %backtrace, "Server Panic");
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use collab::{
        LEGACY_ZED_APP_VERSION_HEADER, LEGACY_ZED_CHECKSUM_HEADER,
        LEGACY_ZED_HEADER_RETIREMENT_CONTRACT, LEGACY_ZED_PROTOCOL_VERSION_HEADER,
        LEGACY_ZED_RELEASE_CHANNEL_HEADER, LEGACY_ZED_SYSTEM_ID_HEADER, LegacyZedAppVersionHeader,
        LegacyZedChecksumHeader, LegacyZedProtocolVersionHeader, LegacyZedReleaseChannelHeader,
        LegacyZedSystemIdHeader, OrionStudioEnvironment,
    };

    fn config_for(
        environment: &str,
        web_url: &str,
        cloud_url: &str,
    ) -> Result<Config, envy::Error> {
        config_from_environment_variables(
            [
                ("HTTP_PORT", "8080"),
                ("DATABASE_URL", "postgres://database.test/collab"),
                ("DATABASE_MAX_CONNECTIONS", "5"),
                ("ORION_STUDIO_ENVIRONMENT", environment),
                ("ORION_STUDIO_WEB_URL", web_url),
                ("ORION_STUDIO_CLOUD_URL", cloud_url),
                ("ORION_STUDIO_CLOUD_INTERNAL_API_KEY", "test-key"),
            ]
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned())),
        )
    }

    fn legacy_only_config(environment: &str) -> Result<Config, envy::Error> {
        config_from_environment_variables(
            [
                ("HTTP_PORT", "8080"),
                ("DATABASE_URL", "postgres://database.test/collab"),
                ("DATABASE_MAX_CONNECTIONS", "5"),
                ("ZED_ENVIRONMENT", environment),
                ("ZED_CLOUD_INTERNAL_API_KEY", "legacy-key"),
                ("ZED_CLIENT_CHECKSUM_SEED", "legacy-seed"),
            ]
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned())),
        )
    }

    fn dotenv_variables(contents: &str) -> Vec<(String, String)> {
        let variables: toml::Table =
            toml::from_str(contents).expect("default .env.toml should contain valid TOML");

        variables
            .into_iter()
            .map(|(name, value)| {
                let value = match value {
                    toml::Value::String(value) => value,
                    toml::Value::Integer(value) => value.to_string(),
                    toml::Value::Float(value) => value.to_string(),
                    toml::Value::Boolean(value) => value.to_string(),
                    _ => panic!("default .env.toml values must be scalars"),
                };
                (name, value)
            })
            .collect()
    }

    #[test]
    fn canonical_environment_variables_take_precedence_and_legacy_values_fall_back() {
        let canonical = config_from_environment_variables(
            [
                ("HTTP_PORT", "8080"),
                ("DATABASE_URL", "postgres://database.test/collab"),
                ("DATABASE_MAX_CONNECTIONS", "5"),
                ("ORION_STUDIO_ENVIRONMENT", "development"),
                ("ZED_ENVIRONMENT", "production"),
                ("ORION_STUDIO_WEB_URL", "http://orion.localhost:3000"),
                (
                    "ORION_STUDIO_CLOUD_URL",
                    "http://cloud.orion.localhost:8787",
                ),
                ("ORION_STUDIO_CLOUD_INTERNAL_API_KEY", "canonical-key"),
                ("ZED_CLOUD_INTERNAL_API_KEY", "legacy-key"),
                ("ORION_STUDIO_CLIENT_CHECKSUM_SEED", "canonical-seed"),
                ("ZED_CLIENT_CHECKSUM_SEED", "legacy-seed"),
            ]
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned())),
        )
        .expect("canonical config should deserialize");

        assert_eq!(
            canonical.orion_studio_environment,
            OrionStudioEnvironment::Development
        );
        assert_eq!(
            canonical.orion_studio_cloud_internal_api_key,
            "canonical-key"
        );
        assert_eq!(
            canonical.orion_studio_client_checksum_seed.as_deref(),
            Some("canonical-seed")
        );

        let empty_canonical = config_from_environment_variables(
            [
                ("HTTP_PORT", "8080"),
                ("DATABASE_URL", "postgres://database.test/collab"),
                ("DATABASE_MAX_CONNECTIONS", "5"),
                ("ORION_STUDIO_ENVIRONMENT", "development"),
                ("ORION_STUDIO_WEB_URL", "http://orion.localhost:3000"),
                (
                    "ORION_STUDIO_CLOUD_URL",
                    "http://cloud.orion.localhost:8787",
                ),
                ("ORION_STUDIO_CLOUD_INTERNAL_API_KEY", ""),
                ("ZED_CLOUD_INTERNAL_API_KEY", "legacy-key"),
                ("ORION_STUDIO_CLIENT_CHECKSUM_SEED", ""),
                ("ZED_CLIENT_CHECKSUM_SEED", "legacy-seed"),
            ]
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned())),
        )
        .expect("present canonical values should not fall back");

        assert!(
            empty_canonical
                .orion_studio_cloud_internal_api_key
                .is_empty()
        );
        assert_eq!(
            empty_canonical.orion_studio_client_checksum_seed.as_deref(),
            Some("")
        );

        let legacy = legacy_only_config("development")
            .expect("legacy config should fall back into canonical fields");

        assert_eq!(
            legacy.orion_studio_environment,
            OrionStudioEnvironment::Development
        );
        assert_eq!(legacy.orion_studio_cloud_internal_api_key, "legacy-key");
        assert_eq!(
            legacy.orion_studio_client_checksum_seed.as_deref(),
            Some("legacy-seed")
        );
    }

    #[test]
    fn legacy_only_development_derives_orion_endpoints() {
        let config =
            legacy_only_config("development").expect("legacy development config should load");

        assert_eq!(
            config.orion_studio_environment,
            OrionStudioEnvironment::Development
        );
        assert_eq!(config.orion_dev_url(), "http://localhost:3000");
        assert_eq!(config.orion_cloud_url(), "http://localhost:8787");
        config
            .validate()
            .expect("derived development endpoints should validate");
    }

    #[test]
    fn legacy_only_staging_derives_orion_endpoints() {
        let config = legacy_only_config("staging").expect("legacy staging config should load");

        assert_eq!(
            config.orion_studio_environment,
            OrionStudioEnvironment::Staging
        );
        assert_eq!(config.orion_dev_url(), "https://staging.orion.dev");
        assert_eq!(config.orion_cloud_url(), "https://cloud.orion.dev");
        config
            .validate()
            .expect("derived staging endpoints should validate");
    }

    #[test]
    fn legacy_only_production_derives_orion_endpoints() {
        let config =
            legacy_only_config("production").expect("legacy production config should load");

        assert_eq!(
            config.orion_studio_environment,
            OrionStudioEnvironment::Production
        );
        assert_eq!(config.orion_dev_url(), "https://orion.dev");
        assert_eq!(config.orion_cloud_url(), "https://cloud.orion.dev");
        config
            .validate()
            .expect("derived production endpoints should validate");
    }

    #[test]
    fn default_dotenv_deserializes_and_validates() {
        let config =
            config_from_environment_variables(dotenv_variables(include_str!("../.env.toml")))
                .expect("default .env.toml should deserialize into Config");

        assert_eq!(
            config.orion_studio_environment,
            OrionStudioEnvironment::Development
        );
        assert_eq!(config.orion_dev_url(), "http://localhost:3000");
        assert_eq!(config.orion_cloud_url(), "http://localhost:8787");
        config
            .validate()
            .expect("default .env.toml should pass startup validation");
    }

    #[test]
    fn environments_and_cloud_urls_are_explicit() {
        let development = config_for(
            "development",
            "http://orion.localhost:3000",
            "http://cloud.orion.localhost:8787",
        )
        .expect("development config should deserialize");
        assert_eq!(
            development.orion_studio_environment,
            OrionStudioEnvironment::Development
        );
        development
            .validate()
            .expect("development URLs should validate");

        let staging = config_for(
            "staging",
            "https://studio.staging.example.test",
            "https://cloud.staging.example.test",
        )
        .expect("staging config should deserialize");
        assert_eq!(
            staging.orion_studio_environment,
            OrionStudioEnvironment::Staging
        );
        assert_eq!(
            staging.orion_cloud_url(),
            "https://cloud.staging.example.test"
        );
        staging.validate().expect("staging URLs should validate");

        let production = config_for(
            "production",
            "https://studio.example.test",
            "https://cloud.example.test",
        )
        .expect("production config should deserialize");
        assert_eq!(
            production.orion_studio_environment,
            OrionStudioEnvironment::Production
        );
        production
            .validate()
            .expect("production URLs should validate");

        let insecure_staging = config_for(
            "staging",
            "http://studio.staging.example.test",
            "http://cloud.staging.example.test",
        )
        .expect("insecure staging config should deserialize before validation");
        assert!(insecure_staging.validate().is_err());
    }

    #[test]
    fn unknown_environment_and_missing_staging_url_fail_closed() {
        assert!(
            config_for(
                "qa",
                "https://web.example.test",
                "https://cloud.example.test"
            )
            .is_err()
        );
        assert!(
            envy::from_iter::<_, Config>(
                [
                    ("HTTP_PORT", "8080"),
                    ("DATABASE_URL", "postgres://database.test/collab"),
                    ("DATABASE_MAX_CONNECTIONS", "5"),
                    ("ORION_STUDIO_ENVIRONMENT", "staging"),
                    (
                        "ORION_STUDIO_WEB_URL",
                        "https://studio.staging.example.test"
                    ),
                    ("ORION_STUDIO_CLOUD_INTERNAL_API_KEY", "test-key"),
                ]
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned())),
            )
            .is_err()
        );
    }

    #[test]
    fn canonical_environment_without_urls_fails_closed() {
        let result = config_from_environment_variables(
            [
                ("HTTP_PORT", "8080"),
                ("DATABASE_URL", "postgres://database.test/collab"),
                ("DATABASE_MAX_CONNECTIONS", "5"),
                ("ORION_STUDIO_ENVIRONMENT", "development"),
                ("ZED_ENVIRONMENT", "production"),
                ("ORION_STUDIO_CLOUD_INTERNAL_API_KEY", "canonical-key"),
            ]
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned())),
        );

        assert!(result.is_err());
    }

    #[test]
    fn legacy_environment_with_only_one_canonical_url_fails_closed() {
        for (url_name, url) in [
            ("ORION_STUDIO_WEB_URL", "https://staging.orion.dev"),
            ("ORION_STUDIO_CLOUD_URL", "https://cloud.orion.dev"),
        ] {
            let result = config_from_environment_variables(
                [
                    ("HTTP_PORT", "8080"),
                    ("DATABASE_URL", "postgres://database.test/collab"),
                    ("DATABASE_MAX_CONNECTIONS", "5"),
                    ("ZED_ENVIRONMENT", "staging"),
                    (url_name, url),
                    ("ZED_CLOUD_INTERNAL_API_KEY", "legacy-key"),
                ]
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned())),
            );

            assert!(
                result.is_err(),
                "{url_name} must not be mixed with fallback"
            );
        }
    }

    #[tokio::test]
    async fn root_identifies_orion_studio() {
        let response = handle_root(Extension(ServiceMode::Collab)).await;
        assert!(response.starts_with("orion-studio:collab "));
        assert!(!response.to_ascii_lowercase().contains("zed:"));
    }

    #[test]
    fn legacy_zed_headers_remain_an_explicit_upgrade_contract() {
        assert_eq!(
            <LegacyZedChecksumHeader as axum::headers::Header>::name().as_str(),
            LEGACY_ZED_CHECKSUM_HEADER
        );
        assert_eq!(
            <LegacyZedSystemIdHeader as axum::headers::Header>::name().as_str(),
            LEGACY_ZED_SYSTEM_ID_HEADER
        );
        assert_eq!(
            <LegacyZedProtocolVersionHeader as axum::headers::Header>::name().as_str(),
            LEGACY_ZED_PROTOCOL_VERSION_HEADER
        );
        assert_eq!(
            <LegacyZedAppVersionHeader as axum::headers::Header>::name().as_str(),
            LEGACY_ZED_APP_VERSION_HEADER
        );
        assert_eq!(
            <LegacyZedReleaseChannelHeader as axum::headers::Header>::name().as_str(),
            LEGACY_ZED_RELEASE_CHANNEL_HEADER
        );
        assert!(LEGACY_ZED_HEADER_RETIREMENT_CONTRACT.contains("every supported Orion client"));
    }
}
