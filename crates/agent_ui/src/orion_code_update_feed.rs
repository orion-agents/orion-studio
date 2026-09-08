use std::{collections::HashSet, sync::Arc, time::Duration};

use chrono::Utc;
use db::kvp::KeyValueStore;
use futures::{AsyncReadExt as _, FutureExt as _};
use http_client::{
    AsyncBody, HttpClient, HttpRequestExt as _, Method, RedirectPolicy, Request, StatusCode,
    http::header::{ACCEPT, ACCEPT_ENCODING, CONTENT_LENGTH, ETAG, IF_NONE_MATCH, LOCATION},
};
use orion_code_update::{
    MAX_INDEX_BYTES, MAX_SIGNATURE_ENVELOPE_BYTES, OrionCodeIndexVerifier,
    OrionCodeUpdateContractError,
};
use thiserror::Error;
use url::Url;

use crate::{
    orion_code_update::{
        OrionCodeUpdateErrorKind, read_orion_code_cached_index, write_orion_code_cached_index,
    },
    orion_code_update_coordinator::{
        OrionCodeUpdateCheckRequest, OrionCodeUpdateFeed as CoordinatorFeed,
        OrionCodeUpdateFeedError as CoordinatorFeedError, OrionCodeUpdateFeedResponse,
    },
};

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_REDIRECT_LIMIT: u8 = 3;

#[derive(Clone, Debug)]
pub struct OrionCodeUpdateFeedConfiguration {
    index_url: Url,
    signature_url: Url,
    allowed_hosts: HashSet<String>,
    request_timeout: Duration,
    redirect_limit: u8,
}

impl OrionCodeUpdateFeedConfiguration {
    pub fn new(
        index_url: &str,
        signature_url: &str,
        allowed_hosts: impl IntoIterator<Item = String>,
    ) -> Result<Self, OrionCodeHttpFeedError> {
        let allowed_hosts = allowed_hosts
            .into_iter()
            .map(|host| host.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        if allowed_hosts.is_empty() {
            return Err(OrionCodeHttpFeedError::Configuration(
                "at least one trusted update host is required".to_string(),
            ));
        }
        let index_url = validate_feed_url(index_url, &allowed_hosts)?;
        let signature_url = validate_feed_url(signature_url, &allowed_hosts)?;
        Ok(Self {
            index_url,
            signature_url,
            allowed_hosts,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            redirect_limit: DEFAULT_REDIRECT_LIMIT,
        })
    }

    #[cfg(test)]
    fn with_limits(mut self, request_timeout: Duration, redirect_limit: u8) -> Self {
        self.request_timeout = request_timeout;
        self.redirect_limit = redirect_limit;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrionCodeSignedIndexFetch {
    NotModified {
        etag: Option<String>,
    },
    Modified {
        index_bytes: Vec<u8>,
        signature_envelope_bytes: Vec<u8>,
        etag: Option<String>,
    },
}

#[derive(Debug, Error)]
pub enum OrionCodeHttpFeedError {
    #[error("invalid Orion Code update feed configuration: {0}")]
    Configuration(String),
    #[error("failed to build Orion Code update request: {0}")]
    Request(String),
    #[error("Orion Code update transport failed: {0}")]
    Transport(String),
    #[error("Orion Code update redirect was rejected: {0}")]
    Redirect(String),
    #[error("Orion Code update endpoint returned HTTP {0}")]
    HttpStatus(StatusCode),
    #[error("Orion Code update response metadata is invalid: {0}")]
    ResponseMetadata(String),
    #[error("Orion Code update {kind} body exceeded the {limit} byte limit")]
    BodyTooLarge { kind: &'static str, limit: usize },
    #[error(
        "Orion Code update {kind} body was truncated: expected {expected} bytes, read {actual}"
    )]
    TruncatedBody {
        kind: &'static str,
        expected: u64,
        actual: u64,
    },
}

pub struct OrionCodeHttpUpdateFeed {
    http_client: Arc<dyn HttpClient>,
    configuration: OrionCodeUpdateFeedConfiguration,
}

impl OrionCodeHttpUpdateFeed {
    pub fn new(
        http_client: Arc<dyn HttpClient>,
        configuration: OrionCodeUpdateFeedConfiguration,
    ) -> Self {
        Self {
            http_client,
            configuration,
        }
    }

    pub async fn fetch(
        &self,
        cached_etag: Option<&str>,
    ) -> Result<OrionCodeSignedIndexFetch, OrionCodeHttpFeedError> {
        let index = self
            .fetch_object(
                self.configuration.index_url.clone(),
                cached_etag,
                "index",
                MAX_INDEX_BYTES,
            )
            .await?;
        let FetchedObject::Modified {
            bytes: index_bytes,
            etag,
        } = index
        else {
            return Ok(OrionCodeSignedIndexFetch::NotModified {
                etag: index.etag().map(ToOwned::to_owned),
            });
        };
        let signature = self
            .fetch_object(
                self.configuration.signature_url.clone(),
                None,
                "signature envelope",
                MAX_SIGNATURE_ENVELOPE_BYTES,
            )
            .await?;
        let FetchedObject::Modified {
            bytes: signature_envelope_bytes,
            ..
        } = signature
        else {
            return Err(OrionCodeHttpFeedError::ResponseMetadata(
                "signature endpoint returned 304 without a conditional request".to_string(),
            ));
        };
        Ok(OrionCodeSignedIndexFetch::Modified {
            index_bytes,
            signature_envelope_bytes,
            etag,
        })
    }

    async fn fetch_object(
        &self,
        mut current_url: Url,
        etag: Option<&str>,
        kind: &'static str,
        byte_limit: usize,
    ) -> Result<FetchedObject, OrionCodeHttpFeedError> {
        let conditional_etag = etag.map(validated_etag).transpose()?;
        for redirect_count in 0..=self.configuration.redirect_limit {
            validate_url(&current_url, &self.configuration.allowed_hosts)?;
            let mut request = Request::builder()
                .method(Method::GET)
                .uri(current_url.as_str())
                .header(ACCEPT, "application/json, application/octet-stream")
                .header(ACCEPT_ENCODING, "identity")
                .follow_redirects(RedirectPolicy::NoFollow)
                .timeout(self.configuration.request_timeout);
            if let Some(etag) = conditional_etag.as_ref() {
                request = request.header(IF_NONE_MATCH, etag);
            }
            let request = request
                .body(AsyncBody::empty())
                .map_err(|error| OrionCodeHttpFeedError::Request(error.to_string()))?;
            let mut response = self
                .http_client
                .send(request)
                .await
                .map_err(|error| OrionCodeHttpFeedError::Transport(error.to_string()))?;
            if response.status() == StatusCode::NOT_MODIFIED {
                if conditional_etag.is_none() {
                    return Err(OrionCodeHttpFeedError::ResponseMetadata(
                        "received 304 without If-None-Match".to_string(),
                    ));
                }
                let response_etag = response_etag(response.headers())?;
                let etag = response_etag.or_else(|| {
                    conditional_etag
                        .as_ref()
                        .and_then(|etag| etag.to_str().ok())
                        .map(ToOwned::to_owned)
                });
                return Ok(FetchedObject::NotModified { etag });
            }
            if is_followable_redirect(response.status()) {
                if redirect_count == self.configuration.redirect_limit {
                    return Err(OrionCodeHttpFeedError::Redirect(format!(
                        "more than {} redirects",
                        self.configuration.redirect_limit
                    )));
                }
                let location = response
                    .headers()
                    .get(LOCATION)
                    .ok_or_else(|| {
                        OrionCodeHttpFeedError::Redirect(
                            "redirect response omitted Location".to_string(),
                        )
                    })?
                    .to_str()
                    .map_err(|error| OrionCodeHttpFeedError::Redirect(error.to_string()))?;
                current_url = current_url
                    .join(location)
                    .map_err(|error| OrionCodeHttpFeedError::Redirect(error.to_string()))?;
                validate_url(&current_url, &self.configuration.allowed_hosts)?;
                continue;
            }
            if response.status() != StatusCode::OK {
                return Err(OrionCodeHttpFeedError::HttpStatus(response.status()));
            }

            let content_length = response_content_length(response.headers(), byte_limit, kind)?;
            let response_etag = response_etag(response.headers())?;
            let mut body = response.body_mut().take(byte_limit as u64 + 1);
            let mut bytes = Vec::with_capacity(content_length.unwrap_or(0) as usize);
            body.read_to_end(&mut bytes)
                .await
                .map_err(|error| OrionCodeHttpFeedError::Transport(error.to_string()))?;
            if bytes.len() > byte_limit {
                return Err(OrionCodeHttpFeedError::BodyTooLarge {
                    kind,
                    limit: byte_limit,
                });
            }
            if let Some(content_length) = content_length
                && bytes.len() as u64 != content_length
            {
                return Err(OrionCodeHttpFeedError::TruncatedBody {
                    kind,
                    expected: content_length,
                    actual: bytes.len() as u64,
                });
            }
            return Ok(FetchedObject::Modified {
                bytes,
                etag: response_etag,
            });
        }
        Err(OrionCodeHttpFeedError::Redirect(
            "redirect loop terminated unexpectedly".to_string(),
        ))
    }
}

pub struct OrionCodeVerifiedUpdateFeed {
    http_feed: Arc<OrionCodeHttpUpdateFeed>,
    verifier: Arc<OrionCodeIndexVerifier>,
    key_value_store: Arc<KeyValueStore>,
}

impl OrionCodeVerifiedUpdateFeed {
    pub fn new(
        http_feed: Arc<OrionCodeHttpUpdateFeed>,
        verifier: Arc<OrionCodeIndexVerifier>,
        key_value_store: Arc<KeyValueStore>,
    ) -> Self {
        Self {
            http_feed,
            verifier,
            key_value_store,
        }
    }
}

impl CoordinatorFeed for OrionCodeVerifiedUpdateFeed {
    fn disabled_diagnostic_code(&self) -> Option<&'static str> {
        None
    }

    fn check(
        &self,
        request: OrionCodeUpdateCheckRequest,
    ) -> futures::future::BoxFuture<
        'static,
        Result<OrionCodeUpdateFeedResponse, CoordinatorFeedError>,
    > {
        let http_feed = self.http_feed.clone();
        let verifier = self.verifier.clone();
        let key_value_store = self.key_value_store.clone();
        async move {
            match http_feed
                .fetch(request.cached_etag.as_deref())
                .await
                .map_err(map_http_feed_error)?
            {
                OrionCodeSignedIndexFetch::NotModified { etag } => {
                    let cached = read_orion_code_cached_index(&key_value_store)
                        .map_err(|_| persistence_error("index_cache_read_failed"))?
                        .ok_or_else(|| {
                            CoordinatorFeedError::new(
                                OrionCodeUpdateErrorKind::IndexEnvelope,
                                "index_cache_missing_after_304",
                            )
                        })?;
                    if request.highest_accepted_sequence == 0
                        || cached.sequence != request.highest_accepted_sequence
                    {
                        return Err(CoordinatorFeedError::new(
                            OrionCodeUpdateErrorKind::IndexReplay,
                            "index_cache_sequence_mismatch",
                        ));
                    }
                    let index = verifier
                        .verify_cached(
                            &cached.index_bytes,
                            &cached.signature_envelope_bytes,
                            Utc::now(),
                            request.highest_accepted_sequence,
                        )
                        .map_err(map_contract_error)?;
                    Ok(OrionCodeUpdateFeedResponse::NotModified { index, etag })
                }
                OrionCodeSignedIndexFetch::Modified {
                    index_bytes,
                    signature_envelope_bytes,
                    etag,
                } => {
                    let index = verifier
                        .verify(
                            &index_bytes,
                            &signature_envelope_bytes,
                            Utc::now(),
                            request.highest_accepted_sequence,
                        )
                        .map_err(map_contract_error)?;
                    write_orion_code_cached_index(
                        &key_value_store,
                        &index,
                        &index_bytes,
                        &signature_envelope_bytes,
                    )
                    .await
                    .map_err(|_| persistence_error("index_cache_write_failed"))?;
                    Ok(OrionCodeUpdateFeedResponse::VerifiedIndex { index, etag })
                }
            }
        }
        .boxed()
    }
}

fn map_http_feed_error(error: OrionCodeHttpFeedError) -> CoordinatorFeedError {
    match error {
        OrionCodeHttpFeedError::Configuration(_) | OrionCodeHttpFeedError::Request(_) => {
            CoordinatorFeedError::new(
                OrionCodeUpdateErrorKind::Configuration,
                "update_feed_configuration_invalid",
            )
        }
        OrionCodeHttpFeedError::BodyTooLarge { .. } => CoordinatorFeedError::new(
            OrionCodeUpdateErrorKind::IndexEnvelope,
            "update_feed_body_too_large",
        ),
        OrionCodeHttpFeedError::Transport(_)
        | OrionCodeHttpFeedError::Redirect(_)
        | OrionCodeHttpFeedError::HttpStatus(_)
        | OrionCodeHttpFeedError::ResponseMetadata(_)
        | OrionCodeHttpFeedError::TruncatedBody { .. } => CoordinatorFeedError::new(
            OrionCodeUpdateErrorKind::Network,
            "update_feed_transport_failed",
        ),
    }
}

fn map_contract_error(error: OrionCodeUpdateContractError) -> CoordinatorFeedError {
    match error {
        OrionCodeUpdateContractError::UnknownSigningKey(_)
        | OrionCodeUpdateContractError::InvalidSigningKey(_)
        | OrionCodeUpdateContractError::InvalidSignature => CoordinatorFeedError::new(
            OrionCodeUpdateErrorKind::IndexSignature,
            "update_index_signature_rejected",
        ),
        OrionCodeUpdateContractError::SequenceReplay { .. }
        | OrionCodeUpdateContractError::CachedSequenceMismatch { .. } => CoordinatorFeedError::new(
            OrionCodeUpdateErrorKind::IndexReplay,
            "update_index_sequence_rejected",
        ),
        OrionCodeUpdateContractError::GeneratedInFuture
        | OrionCodeUpdateContractError::ExpiredIndex
        | OrionCodeUpdateContractError::InvalidIndexLifetime => CoordinatorFeedError::new(
            OrionCodeUpdateErrorKind::IndexExpired,
            "update_index_lifetime_rejected",
        ),
        OrionCodeUpdateContractError::IndexTooLarge { .. }
        | OrionCodeUpdateContractError::SignatureEnvelopeTooLarge { .. }
        | OrionCodeUpdateContractError::InvalidSignatureEnvelope(_)
        | OrionCodeUpdateContractError::UnsupportedSignatureEnvelopeSchema(_)
        | OrionCodeUpdateContractError::UnsupportedSignatureAlgorithm(_)
        | OrionCodeUpdateContractError::InvalidIndexJson(_)
        | OrionCodeUpdateContractError::UnsupportedIndexSchema(_)
        | OrionCodeUpdateContractError::ZeroSequence
        | OrionCodeUpdateContractError::InvalidRelease { .. } => CoordinatorFeedError::new(
            OrionCodeUpdateErrorKind::IndexEnvelope,
            "update_index_contract_rejected",
        ),
    }
}

fn persistence_error(diagnostic_code: &'static str) -> CoordinatorFeedError {
    CoordinatorFeedError::new(OrionCodeUpdateErrorKind::Persistence, diagnostic_code)
}

enum FetchedObject {
    NotModified {
        etag: Option<String>,
    },
    Modified {
        bytes: Vec<u8>,
        etag: Option<String>,
    },
}

impl FetchedObject {
    fn etag(&self) -> Option<&str> {
        match self {
            Self::NotModified { etag } | Self::Modified { etag, .. } => etag.as_deref(),
        }
    }
}

fn validate_feed_url(
    value: &str,
    allowed_hosts: &HashSet<String>,
) -> Result<Url, OrionCodeHttpFeedError> {
    let url = Url::parse(value)
        .map_err(|error| OrionCodeHttpFeedError::Configuration(error.to_string()))?;
    validate_url(&url, allowed_hosts)?;
    Ok(url)
}

fn validate_url(url: &Url, allowed_hosts: &HashSet<String>) -> Result<(), OrionCodeHttpFeedError> {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return Err(OrionCodeHttpFeedError::Configuration(
            "update URLs must be HTTPS on port 443 without credentials, query, or fragment"
                .to_string(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| OrionCodeHttpFeedError::Configuration("URL has no host".to_string()))?
        .to_ascii_lowercase();
    if !allowed_hosts.contains(&host) {
        return Err(OrionCodeHttpFeedError::Redirect(format!(
            "host {host} is not trusted"
        )));
    }
    Ok(())
}

fn validated_etag(value: &str) -> Result<http_client::http::HeaderValue, OrionCodeHttpFeedError> {
    if value.len() > 512 || value.chars().any(char::is_control) {
        return Err(OrionCodeHttpFeedError::ResponseMetadata(
            "cached ETag is invalid".to_string(),
        ));
    }
    http_client::http::HeaderValue::from_str(value)
        .map_err(|error| OrionCodeHttpFeedError::ResponseMetadata(error.to_string()))
}

fn response_etag(
    headers: &http_client::http::HeaderMap,
) -> Result<Option<String>, OrionCodeHttpFeedError> {
    let Some(etag) = headers.get(ETAG) else {
        return Ok(None);
    };
    let etag = etag
        .to_str()
        .map_err(|error| OrionCodeHttpFeedError::ResponseMetadata(error.to_string()))?;
    validated_etag(etag)?;
    Ok(Some(etag.to_string()))
}

fn response_content_length(
    headers: &http_client::http::HeaderMap,
    byte_limit: usize,
    kind: &'static str,
) -> Result<Option<u64>, OrionCodeHttpFeedError> {
    let Some(value) = headers.get(CONTENT_LENGTH) else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|error| OrionCodeHttpFeedError::ResponseMetadata(error.to_string()))?
        .parse::<u64>()
        .map_err(|error| OrionCodeHttpFeedError::ResponseMetadata(error.to_string()))?;
    if value > byte_limit as u64 {
        return Err(OrionCodeHttpFeedError::BodyTooLarge {
            kind,
            limit: byte_limit,
        });
    }
    Ok(Some(value))
}

fn is_followable_redirect(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use http_client::{FakeHttpClient, Response};

    use super::*;

    fn configuration() -> OrionCodeUpdateFeedConfiguration {
        OrionCodeUpdateFeedConfiguration::new(
            "https://updates.example.invalid/index.json",
            "https://updates.example.invalid/index.json.sig",
            ["updates.example.invalid".to_string()],
        )
        .expect("valid configuration")
        .with_limits(Duration::from_secs(1), 2)
    }

    #[test]
    fn fetches_exact_bounded_bytes_and_sends_conditional_etag() {
        futures::executor::block_on(async {
            let requests = Arc::new(Mutex::new(Vec::new()));
            let client = FakeHttpClient::create({
                let requests = requests.clone();
                move |request| {
                    requests
                        .lock()
                        .expect("request lock")
                        .push((request.uri().to_string(), request.headers().clone()));
                    let is_signature = request.uri().path().ends_with(".sig");
                    async move {
                        let body = if is_signature {
                            b"signature".to_vec()
                        } else {
                            b"index".to_vec()
                        };
                        Ok(Response::builder()
                            .status(StatusCode::OK)
                            .header(CONTENT_LENGTH, body.len())
                            .header(ETAG, "\"sequence-42\"")
                            .body(body.into())
                            .expect("response"))
                    }
                }
            });
            let feed = OrionCodeHttpUpdateFeed::new(client, configuration());
            let result = feed
                .fetch(Some("\"sequence-41\""))
                .await
                .expect("fetch feed");
            assert_eq!(
                result,
                OrionCodeSignedIndexFetch::Modified {
                    index_bytes: b"index".to_vec(),
                    signature_envelope_bytes: b"signature".to_vec(),
                    etag: Some("\"sequence-42\"".to_string()),
                }
            );
            let requests = requests.lock().expect("request lock");
            assert_eq!(requests.len(), 2);
            assert_eq!(
                requests[0]
                    .1
                    .get(IF_NONE_MATCH)
                    .and_then(|value| value.to_str().ok()),
                Some("\"sequence-41\"")
            );
            assert_eq!(
                requests[0]
                    .1
                    .get(ACCEPT_ENCODING)
                    .and_then(|value| value.to_str().ok()),
                Some("identity")
            );
            assert!(requests[1].1.get(IF_NONE_MATCH).is_none());
        });
    }

    #[test]
    fn honors_304_and_rejects_redirect_escape_oversize_and_truncation() {
        futures::executor::block_on(async {
            let not_modified = FakeHttpClient::create(|_| async {
                Ok(Response::builder()
                    .status(StatusCode::NOT_MODIFIED)
                    .header(ETAG, "\"same\"")
                    .body(AsyncBody::empty())
                    .expect("response"))
            });
            let result = OrionCodeHttpUpdateFeed::new(not_modified, configuration())
                .fetch(Some("\"same\""))
                .await
                .expect("304 result");
            assert_eq!(
                result,
                OrionCodeSignedIndexFetch::NotModified {
                    etag: Some("\"same\"".to_string())
                }
            );

            let redirect = FakeHttpClient::create(|_| async {
                Ok(Response::builder()
                    .status(StatusCode::FOUND)
                    .header(LOCATION, "https://attacker.invalid/index.json")
                    .body(AsyncBody::empty())
                    .expect("response"))
            });
            assert!(matches!(
                OrionCodeHttpUpdateFeed::new(redirect, configuration())
                    .fetch(None)
                    .await,
                Err(OrionCodeHttpFeedError::Redirect(_))
            ));

            let oversized = FakeHttpClient::create(|_| async {
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_LENGTH, MAX_INDEX_BYTES + 1)
                    .body(AsyncBody::empty())
                    .expect("response"))
            });
            assert!(matches!(
                OrionCodeHttpUpdateFeed::new(oversized, configuration())
                    .fetch(None)
                    .await,
                Err(OrionCodeHttpFeedError::BodyTooLarge { .. })
            ));

            let truncated = FakeHttpClient::create(|_| async {
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_LENGTH, 10)
                    .body(AsyncBody::from("short"))
                    .expect("response"))
            });
            assert!(matches!(
                OrionCodeHttpUpdateFeed::new(truncated, configuration())
                    .fetch(None)
                    .await,
                Err(OrionCodeHttpFeedError::TruncatedBody { .. })
            ));
        });
    }
}
