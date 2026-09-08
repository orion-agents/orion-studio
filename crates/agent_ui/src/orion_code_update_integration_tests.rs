use std::{
    io,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context as TaskContext, Poll},
};

use agent_settings::AgentSettings;
use chrono::{DateTime, Utc};
use db::kvp::KeyValueStore;
use futures::{AsyncRead, FutureExt as _, future::BoxFuture, task::AtomicWaker};
use gpui::{Entity, TestAppContext, UpdateGlobal as _};
use http_client::{
    AsyncBody, FakeHttpClient, HttpClient, Response, StatusCode, http::header::CONTENT_LENGTH,
};
use orion_code_update::{OrionCodeArtifactManifestV1, OrionCodeSigningRequirement};
use parking_lot::Mutex;
use project::agent_server_store::{AllAgentServersSettings, orion_code_managed_archive_runtime};
use semver::Version;
use settings::{Settings as _, SettingsStore};
use sha2::{Digest as _, Sha256};
use tempfile::TempDir;

use crate::{
    OrionCodeIndexVerifier,
    agent_connection_store::{
        OrionCodeActivityKind, OrionCodeActivityLease, OrionCodeUpdateActivity,
        orion_code_new_connections_are_blocked,
    },
    orion_code_bootstrap::{OrionCodeBootstrap, OrionCodeBootstrapChoice},
    orion_code_update::{
        OrionCodeCandidate, OrionCodeInstallReceiptV2, OrionCodeInstallSource,
        OrionCodeStagedReceiptV2, OrionCodeUpdateChannel, OrionCodeUpdateErrorKind,
        OrionCodeUpdateRecordV2, begin_orion_code_activation, read_orion_code_update_record,
        write_orion_code_update_record,
    },
    orion_code_update_activation::{
        OrionCodeActivationCoordinator, OrionCodeActivationHealthCheck,
        OrionCodeActivationHealthError, OrionCodeActivationStatus,
    },
    orion_code_update_coordinator::{
        OrionCodeUpdateCheckRequest, OrionCodeUpdateCoordinator, OrionCodeUpdateFeed,
        OrionCodeUpdateFeedError, OrionCodeUpdateFeedResponse,
    },
    orion_code_update_installer::{
        OrionCodeArchiveInstaller, OrionCodePreflightError, OrionCodeStagedIdentity,
        OrionCodeUpdatePreflight,
    },
    orion_code_update_pipeline::{
        OrionCodeArchiveInstallerHandle, OrionCodeUpdatePipeline, OrionCodeUpdatePipelineStatus,
    },
    orion_code_update_platform::{OrionCodePlatformVerificationError, OrionCodePlatformVerifier},
};

const TEST_KEY_ID: &str = "up-int-01-test-key";
const TEST_ARCHIVE_HOST: &str = "updates.example.invalid";
const TEST_TARGET: &str = "darwin-aarch64";
const TEST_COMMAND: &str = "OrionCodeSidecar.app/Contents/MacOS/orion-code-acp";
const TEST_VERIFYING_KEY: [u8; 32] = [
    135, 227, 133, 164, 82, 54, 161, 5, 252, 128, 242, 104, 118, 17, 112, 152, 94, 104, 34, 61,
    225, 122, 67, 36, 143, 37, 52, 191, 39, 140, 148, 206,
];
const TEST_SIGNATURE_ENVELOPE: &[u8] = br#"{
  "schema_version": 1,
  "algorithm": "ed25519",
  "key_id": "up-int-01-test-key",
  "signature": "cqCNz3m20VRnVixdPzNCciW16FbIZs0PA6GgcUwshEJk1cEEeVR9/iPAPi5BW0gYoA6Rn4KX8etuOtg/qZp2DQ=="
}"#;
const TEST_INDEX_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../orion_code_update/contracts/orion-code-update-v1/golden/valid-index.json"
));

const E2E_INDEX_SEQUENCE: u64 = 43;
const E2E_DOWNLOAD_GATE_BYTES: usize = 256 * 1024;
const E2E_ARCHIVE_BYTES: usize = 329_503;
const E2E_ARCHIVE_SHA256: &str = "053d8a2bea01a66bcafe1b6032f8b809ea8cb443b40b0338593d102d8ae0c5d5";
const E2E_MANIFEST_SHA256: &str =
    "675b7fafaefe7211464371eec4475c0500f95f502d5a86e54b593cc3ad5e6249";
const E2E_SBOM_SHA256: &str = "20b5397e8c658dd61fca0ec2ea9a3841f979716daace2ab4ff747648d9bf9d18";
const E2E_VERIFYING_KEY: [u8; 32] = [
    234, 74, 108, 99, 226, 156, 82, 10, 190, 245, 80, 123, 19, 46, 197, 249, 149, 71, 118, 174,
    190, 190, 123, 146, 66, 30, 234, 105, 20, 70, 210, 44,
];
const E2E_SIGNATURE_ENVELOPE_BYTES: &[u8] = br#"{"schema_version":1,"algorithm":"ed25519","key_id":"e2e-test-key","signature":"5QvwmVyEK4ySeA8zcl/yi6+pFxA7VJrWOl+cyqcr+TK4IMqf0djNGrrLCbB0DUxew6KGO0ZiWh7VqTS87QgMBQ=="}"#;
const E2E_INDEX_BYTES: &[u8] = br#"{"schema_version":1,"sequence":43,"generated_at":"2026-09-01T00:00:00Z","expires_at":"2099-09-08T00:00:00Z","releases":[{"version":"0.4.0","channel":"stable","status":"active","published_at":"2026-09-01T00:00:00Z","studio_version_requirement":">=0.0.0","acp_protocol":1,"rollout_basis_points":10000,"rollout_salt":"deterministic-e2e-rollout","rollback_to":null,"release_notes_url":"https://updates.example.invalid/releases/0.4.0","targets":{"darwin-aarch64":{"archive_url":"https://updates.example.invalid/orion-code/0.4.0/darwin-aarch64.zip","archive_sha256":"053d8a2bea01a66bcafe1b6032f8b809ea8cb443b40b0338593d102d8ae0c5d5","archive_bytes":329503,"format":"zip","command":"OrionCodeSidecar.app/Contents/MacOS/orion-code-acp","manifest_sha256":"675b7fafaefe7211464371eec4475c0500f95f502d5a86e54b593cc3ad5e6249","sbom_sha256":"20b5397e8c658dd61fca0ec2ea9a3841f979716daace2ab4ff747648d9bf9d18","signing_requirement":"developer_id_and_notarized"}}}]}"#;
const E2E_MANIFEST_BYTES: &[u8] = br#"{"schema_version":1,"version":"0.4.0","git_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","target":"darwin-aarch64","built_at":"2026-09-01T00:00:00Z","acp_protocol":1,"studio_version_requirement":">=0.0.0","node_version":"22.22.0","node_abi":"127","native_modules":[],"command":"OrionCodeSidecar.app/Contents/MacOS/orion-code-acp","sbom_path":"SBOM.cdx.json","sbom_sha256":"20b5397e8c658dd61fca0ec2ea9a3841f979716daace2ab4ff747648d9bf9d18","notices_path":"THIRD_PARTY_NOTICES","notices_sha256":"b76e89dac4925efd3133d84d96aeb4c74de3cab8b38c40d4c910d624b4319431","files":[{"path":"LICENSE","mode":420,"bytes":8,"sha256":"a36ed5e8128a6ce14d4811f1f8ba9dc688f604efa7c288580ccd183db61eefe0"},{"path":"OrionCodeSidecar.app/Contents/MacOS/orion-code-acp","mode":493,"bytes":327698,"sha256":"b51d3112e4b83256e8d9a350dd5f6ee8af5430e7e730ac72cd65a3f037cb2f78"},{"path":"SBOM.cdx.json","mode":420,"bytes":45,"sha256":"20b5397e8c658dd61fca0ec2ea9a3841f979716daace2ab4ff747648d9bf9d18"},{"path":"THIRD_PARTY_NOTICES","mode":420,"bytes":42,"sha256":"b76e89dac4925efd3133d84d96aeb4c74de3cab8b38c40d4c910d624b4319431"}]}"#;

struct ExactBytesSignedFeed {
    calls: Arc<AtomicUsize>,
    verifier: Arc<OrionCodeIndexVerifier>,
    verified_payloads: Arc<Mutex<Vec<String>>>,
}

impl OrionCodeUpdateFeed for ExactBytesSignedFeed {
    fn disabled_diagnostic_code(&self) -> Option<&'static str> {
        None
    }

    fn check(
        &self,
        request: OrionCodeUpdateCheckRequest,
    ) -> BoxFuture<'static, Result<OrionCodeUpdateFeedResponse, OrionCodeUpdateFeedError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let verifier = self.verifier.clone();
        let verified_payloads = self.verified_payloads.clone();
        async move {
            if request.channel != OrionCodeUpdateChannel::Stable {
                return Err(OrionCodeUpdateFeedError::new(
                    OrionCodeUpdateErrorKind::Incompatible,
                    "unexpected_e2e_update_channel",
                ));
            }
            let index = verifier
                .verify(
                    E2E_INDEX_BYTES,
                    E2E_SIGNATURE_ENVELOPE_BYTES,
                    test_time(),
                    request.highest_accepted_sequence,
                )
                .map_err(|_| {
                    OrionCodeUpdateFeedError::new(
                        OrionCodeUpdateErrorKind::IndexSignature,
                        "e2e_exact_bytes_signature_rejected",
                    )
                })?;
            verified_payloads.lock().push(index.payload_sha256.clone());
            Ok(OrionCodeUpdateFeedResponse::VerifiedIndex {
                index,
                etag: Some("\"e2e-sequence-43\"".to_string()),
            })
        }
        .boxed()
    }
}

struct FixturePlatformVerifier {
    calls: Arc<AtomicUsize>,
}

impl OrionCodePlatformVerifier for FixturePlatformVerifier {
    fn verify(
        &self,
        artifact_root: PathBuf,
        manifest: OrionCodeArtifactManifestV1,
        signing_requirement: OrionCodeSigningRequirement,
    ) -> BoxFuture<'static, Result<(), OrionCodePlatformVerificationError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(manifest.version, "0.4.0");
        assert_eq!(manifest.target, TEST_TARGET);
        assert_eq!(manifest.sbom_sha256, E2E_SBOM_SHA256);
        assert!(artifact_root.join(&manifest.command).is_file());
        assert_eq!(
            signing_requirement,
            OrionCodeSigningRequirement::DeveloperIdAndNotarized
        );
        async { Ok(()) }.boxed()
    }
}

struct FixtureAcpPreflight {
    calls: Arc<AtomicUsize>,
}

impl OrionCodeUpdatePreflight for FixtureAcpPreflight {
    fn verify(
        &self,
        artifact_root: PathBuf,
        candidate: OrionCodeCandidate,
    ) -> BoxFuture<'static, Result<(), OrionCodePreflightError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        async move {
            let command = artifact_root.join(&candidate.target.command);
            let command_bytes =
                std::fs::read(command).map_err(|_| OrionCodePreflightError::Failed {
                    diagnostic_code: "fixture_acp_command_unreadable".to_string(),
                })?;
            if !command_bytes.starts_with(b"ORION-ACP-FIXTURE\n") {
                return Err(OrionCodePreflightError::Failed {
                    diagnostic_code: "fixture_acp_handshake_rejected".to_string(),
                });
            }
            Ok(())
        }
        .boxed()
    }
}

#[derive(Default)]
struct DownloadGate {
    open: AtomicBool,
    waker: AtomicWaker,
}

impl DownloadGate {
    fn release(&self) {
        self.open.store(true, Ordering::SeqCst);
        self.waker.wake();
    }
}

struct GatedArchiveReader {
    archive: Arc<Vec<u8>>,
    offset: usize,
    gate_after: usize,
    gate: Arc<DownloadGate>,
}

impl AsyncRead for GatedArchiveReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut TaskContext<'_>,
        buffer: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        if self.offset >= self.archive.len() {
            return Poll::Ready(Ok(0));
        }
        if self.offset >= self.gate_after && !self.gate.open.load(Ordering::SeqCst) {
            self.gate.waker.register(context.waker());
            if !self.gate.open.load(Ordering::SeqCst) {
                return Poll::Pending;
            }
        }

        let count = buffer
            .len()
            .min(32 * 1024)
            .min(self.archive.len() - self.offset);
        let end = self.offset + count;
        buffer[..count].copy_from_slice(&self.archive[self.offset..end]);
        self.offset = end;
        Poll::Ready(Ok(count))
    }
}

fn e2e_archive_http_client(
    archive: Vec<u8>,
    gate: Arc<DownloadGate>,
    calls: Arc<AtomicUsize>,
) -> Arc<dyn HttpClient> {
    let archive = Arc::new(archive);
    FakeHttpClient::create(move |request| {
        assert_eq!(
            request.uri().to_string(),
            "https://updates.example.invalid/orion-code/0.4.0/darwin-aarch64.zip"
        );
        calls.fetch_add(1, Ordering::SeqCst);
        let archive = archive.clone();
        let gate = gate.clone();
        async move {
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_LENGTH, archive.len().to_string())
                .body(AsyncBody::from_reader(GatedArchiveReader {
                    archive,
                    offset: 0,
                    gate_after: E2E_DOWNLOAD_GATE_BYTES,
                    gate,
                }))
                .expect("e2e in-memory archive response must be valid"))
        }
    }) as Arc<dyn HttpClient>
}

fn e2e_archive_bytes() -> Vec<u8> {
    let command_bytes = [
        b"ORION-ACP-FIXTURE\n".as_slice(),
        vec![b'Z'; 320 * 1024].as_slice(),
    ]
    .concat();
    let entries = vec![
        ("LICENSE".to_string(), b"GPL-3.0\n".to_vec(), 0o100644),
        (
            "SBOM.cdx.json".to_string(),
            br#"{"bomFormat":"CycloneDX","specVersion":"1.6"}"#.to_vec(),
            0o100644,
        ),
        (
            "THIRD_PARTY_NOTICES".to_string(),
            b"deterministic integration fixture notices\n".to_vec(),
            0o100644,
        ),
        (TEST_COMMAND.to_string(), command_bytes, 0o100755),
        (
            "manifest.json".to_string(),
            E2E_MANIFEST_BYTES.to_vec(),
            0o100644,
        ),
    ];
    let archive = stored_zip(&entries);
    assert_eq!(archive.len(), E2E_ARCHIVE_BYTES);
    assert_eq!(sha256(&archive), E2E_ARCHIVE_SHA256);
    assert_eq!(sha256(E2E_MANIFEST_BYTES), E2E_MANIFEST_SHA256);
    archive
}

fn stored_zip(entries: &[(String, Vec<u8>, u32)]) -> Vec<u8> {
    struct CentralEntry {
        name: Vec<u8>,
        crc32: u32,
        bytes: u32,
        mode: u32,
        offset: u32,
    }

    let mut output = Vec::new();
    let mut central_entries = Vec::new();
    for (name, bytes, mode) in entries {
        let name = name.as_bytes().to_vec();
        let offset = u32::try_from(output.len()).expect("e2e ZIP offset must fit u32");
        let bytes_length = u32::try_from(bytes.len()).expect("e2e ZIP entry must fit u32");
        let crc32 = crc32(bytes);
        push_u32(&mut output, 0x0403_4b50);
        push_u16(&mut output, 20);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u32(&mut output, crc32);
        push_u32(&mut output, bytes_length);
        push_u32(&mut output, bytes_length);
        push_u16(
            &mut output,
            u16::try_from(name.len()).expect("e2e ZIP name must fit u16"),
        );
        push_u16(&mut output, 0);
        output.extend_from_slice(&name);
        output.extend_from_slice(bytes);
        central_entries.push(CentralEntry {
            name,
            crc32,
            bytes: bytes_length,
            mode: *mode,
            offset,
        });
    }

    let central_offset = u32::try_from(output.len()).expect("e2e ZIP offset must fit u32");
    for entry in &central_entries {
        push_u32(&mut output, 0x0201_4b50);
        push_u16(&mut output, 0x0314);
        push_u16(&mut output, 20);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u32(&mut output, entry.crc32);
        push_u32(&mut output, entry.bytes);
        push_u32(&mut output, entry.bytes);
        push_u16(
            &mut output,
            u16::try_from(entry.name.len()).expect("e2e ZIP name must fit u16"),
        );
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u32(&mut output, entry.mode << 16);
        push_u32(&mut output, entry.offset);
        output.extend_from_slice(&entry.name);
    }
    let central_bytes =
        u32::try_from(output.len()).expect("e2e ZIP length must fit u32") - central_offset;
    let entry_count =
        u16::try_from(central_entries.len()).expect("e2e ZIP entry count must fit u16");
    push_u32(&mut output, 0x0605_4b50);
    push_u16(&mut output, 0);
    push_u16(&mut output, 0);
    push_u16(&mut output, entry_count);
    push_u16(&mut output, entry_count);
    push_u32(&mut output, central_bytes);
    push_u32(&mut output, central_offset);
    push_u16(&mut output, 0);
    output
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

struct NoNetworkFeed {
    calls: Arc<AtomicUsize>,
}

impl OrionCodeUpdateFeed for NoNetworkFeed {
    fn disabled_diagnostic_code(&self) -> Option<&'static str> {
        None
    }

    fn check(
        &self,
        _request: OrionCodeUpdateCheckRequest,
    ) -> BoxFuture<'static, Result<OrionCodeUpdateFeedResponse, OrionCodeUpdateFeedError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        futures::future::ready(Err(OrionCodeUpdateFeedError::new(
            OrionCodeUpdateErrorKind::Network,
            "unexpected_test_feed_call",
        )))
        .boxed()
    }
}

struct FakeHealthCheck {
    calls: Arc<AtomicUsize>,
    succeeds: bool,
}

impl OrionCodeActivationHealthCheck for FakeHealthCheck {
    fn disabled_diagnostic_code(&self) -> Option<&'static str> {
        None
    }

    fn verify(
        &self,
        _staged: OrionCodeStagedReceiptV2,
    ) -> BoxFuture<'static, Result<(), OrionCodeActivationHealthError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let result = if self.succeeds {
            Ok(())
        } else {
            Err(OrionCodeActivationHealthError::new(
                OrionCodeUpdateErrorKind::Preflight,
                "candidate_health_failed",
            ))
        };
        futures::future::ready(result).boxed()
    }
}

struct UpdateHarness {
    coordinator: Entity<OrionCodeUpdateCoordinator>,
    activation: Entity<OrionCodeActivationCoordinator>,
    activity: Entity<OrionCodeUpdateActivity>,
    feed_calls: Arc<AtomicUsize>,
    health_calls: Arc<AtomicUsize>,
    initial_activity: Option<OrionCodeActivityLease>,
}

fn test_time() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-09-01T00:00:00Z")
        .expect("test timestamp must be valid")
        .with_timezone(&Utc)
}

fn archive_receipt(
    version: &str,
    index_sequence: u64,
    digest_character: char,
) -> OrionCodeInstallReceiptV2 {
    OrionCodeInstallReceiptV2 {
        version: version.to_string(),
        source: OrionCodeInstallSource::Archive,
        legacy_imported: false,
        target: Some(TEST_TARGET.to_string()),
        archive_sha256: Some(digest_character.to_string().repeat(64)),
        command: Some(TEST_COMMAND.to_string()),
        installed_at: Some(test_time()),
        index_sequence: Some(index_sequence),
    }
}

fn base_record() -> OrionCodeUpdateRecordV2 {
    OrionCodeUpdateRecordV2 {
        choice: OrionCodeBootstrapChoice::Accepted,
        current_verified: Some(archive_receipt("0.3.0", 40, 'd')),
        previous_verified: Some(archive_receipt("0.2.0", 39, 'e')),
        highest_index_sequence: 42,
        ..OrionCodeUpdateRecordV2::default()
    }
}

fn signed_candidate() -> (OrionCodeCandidate, u64) {
    let verifier = OrionCodeIndexVerifier::new(
        [(TEST_KEY_ID.to_string(), TEST_VERIFYING_KEY)],
        [TEST_ARCHIVE_HOST.to_string()],
    )
    .expect("test verifier must be valid");
    let verified = verifier
        .verify(TEST_INDEX_BYTES, TEST_SIGNATURE_ENVELOPE, test_time(), 0)
        .expect("test candidate must pass exact-byte signature verification");
    let index_sequence = verified.index.sequence;
    let release = verified
        .index
        .releases
        .into_iter()
        .next()
        .expect("golden index must contain a release");
    let target = release
        .targets
        .get(TEST_TARGET)
        .cloned()
        .expect("golden index must contain the test target");
    let version = Version::parse(&release.version).expect("golden version must be valid");
    (
        OrionCodeCandidate {
            version,
            release,
            target,
            target_name: TEST_TARGET.to_string(),
            is_signed_rollback: false,
        },
        index_sequence,
    )
}

fn staged_identity_from_signed_candidate() -> OrionCodeStagedIdentity {
    let (candidate, index_sequence) = signed_candidate();
    OrionCodeStagedIdentity {
        version: candidate.version,
        target: candidate.target_name,
        archive_sha256: candidate.target.archive_sha256,
        command: candidate.target.command,
        index_sequence,
        artifact_root: PathBuf::from("test-managed/orion-code/artifact"),
        receipt_path: PathBuf::from("test-managed/orion-code/install-receipt.json"),
        reused_existing: false,
    }
}

fn staged_receipt_from_signed_candidate() -> OrionCodeStagedReceiptV2 {
    let staged = staged_identity_from_signed_candidate();
    OrionCodeStagedReceiptV2 {
        version: staged.version.to_string(),
        target: staged.target,
        archive_sha256: staged.archive_sha256,
        command: staged.command,
        staged_at: test_time(),
        index_sequence: staged.index_sequence,
    }
}

async fn init_harness(
    cx: &mut TestAppContext,
    record: OrionCodeUpdateRecordV2,
    health_succeeds: bool,
) -> UpdateHarness {
    init_harness_with_busy_activity(cx, record, health_succeeds, false).await
}

async fn init_harness_with_busy_activity(
    cx: &mut TestAppContext,
    record: OrionCodeUpdateRecordV2,
    health_succeeds: bool,
    initially_busy: bool,
) -> UpdateHarness {
    crate::test_support::init_test(cx);
    cx.update(|cx| {
        AgentSettings::register(cx);
        AllAgentServersSettings::register(cx);
        SettingsStore::update_global(cx, |store, cx| {
            store
                .set_user_settings(r#"{"agent":{"orion_code":{"update_mode":"manual"}}}"#, cx)
                .expect("manual test update settings must be valid");
        });
    });

    let key_value_store = cx.update(|cx| KeyValueStore::global(cx));
    write_orion_code_update_record(&key_value_store, &record)
        .await
        .expect("initial update state must persist");

    let feed_calls = Arc::new(AtomicUsize::new(0));
    let health_calls = Arc::new(AtomicUsize::new(0));
    let (coordinator, activation, activity, initial_activity) = cx.update(|cx| {
        OrionCodeBootstrap::init_global(false, cx);
        let activity = OrionCodeUpdateActivity::init_global(cx);
        let initial_activity = initially_busy
            .then(|| activity.read(cx).acquire(OrionCodeActivityKind::ActiveTurn))
            .transpose()
            .expect("initial test activity must be acquirable");
        let coordinator = OrionCodeUpdateCoordinator::init_global_with_feed(
            Arc::new(NoNetworkFeed {
                calls: feed_calls.clone(),
            }),
            cx,
        );
        let activation = OrionCodeActivationCoordinator::init_global_with_health_check(
            Arc::new(FakeHealthCheck {
                calls: health_calls.clone(),
                succeeds: health_succeeds,
            }),
            cx,
        );
        (coordinator, activation, activity, initial_activity)
    });
    cx.run_until_parked();

    UpdateHarness {
        coordinator,
        activation,
        activity,
        feed_calls,
        health_calls,
        initial_activity,
    }
}

fn durable_record(cx: &mut TestAppContext) -> OrionCodeUpdateRecordV2 {
    cx.update(|cx| read_orion_code_update_record(&KeyValueStore::global(cx)))
        .expect("durable update state must be readable")
        .expect("durable update state must exist")
}

#[gpui::test]
async fn orion_code_update_integration_signed_stage_waits_for_idle_then_commits(
    cx: &mut TestAppContext,
) {
    let harness = init_harness(cx, base_record(), true).await;
    let staged_identity = staged_identity_from_signed_candidate();

    harness.activation.update(cx, |activation, cx| {
        activation.record_staged_identity(staged_identity, cx);
    });
    cx.run_until_parked();

    let staged_record = durable_record(cx);
    assert_eq!(
        staged_record
            .staged
            .as_ref()
            .map(|staged| staged.version.as_str()),
        Some("0.4.0")
    );
    assert_eq!(
        harness
            .coordinator
            .read_with(cx, |coordinator, _cx| coordinator.record().clone()),
        staged_record
    );
    assert!(harness.activation.read_with(cx, |activation, _cx| {
        matches!(
            activation.status(),
            OrionCodeActivationStatus::Staged { version } if version == "0.4.0"
        )
    }));

    let mut busy_owner = harness
        .activity
        .read_with(cx, |activity, _cx| activity.owner());
    busy_owner
        .set_session_activity(1, 0, 0)
        .expect("test turn activity must be acquired");
    harness.activation.update(cx, |activation, cx| {
        activation.activate_when_idle(cx);
    });

    assert!(harness.activation.read_with(cx, |activation, _cx| {
        matches!(
            activation.status(),
            OrionCodeActivationStatus::WaitingForIdle { version } if version == "0.4.0"
        )
    }));
    assert_eq!(harness.health_calls.load(Ordering::SeqCst), 0);
    assert!(harness.activity.read_with(cx, |activity, _cx| {
        let snapshot = activity.snapshot();
        snapshot.active_turns == 1 && snapshot.shutdowns == 0 && snapshot.version_switches == 0
    }));
    let waiting_record = durable_record(cx);
    assert_eq!(
        waiting_record
            .current_verified
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.3.0")
    );
    assert!(waiting_record.activation.is_none());

    busy_owner
        .release()
        .expect("idle wake must release the active turn");
    cx.run_until_parked();

    let committed = durable_record(cx);
    assert_eq!(
        committed
            .current_verified
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.4.0")
    );
    assert_eq!(
        committed
            .previous_verified
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.3.0")
    );
    assert!(committed.staged.is_none());
    assert!(committed.activation.is_none());
    assert_eq!(
        harness
            .coordinator
            .read_with(cx, |coordinator, _cx| coordinator.record().clone()),
        committed
    );
    assert!(harness.activation.read_with(cx, |activation, _cx| {
        matches!(
            activation.status(),
            OrionCodeActivationStatus::Activated { version } if version == "0.4.0"
        )
    }));
    assert_eq!(harness.health_calls.load(Ordering::SeqCst), 1);
    assert_eq!(harness.feed_calls.load(Ordering::SeqCst), 0);
    assert!(
        harness
            .activity
            .read_with(cx, |activity, _cx| activity.is_idle())
    );
    assert_eq!(
        cx.update(|cx| orion_code_managed_archive_runtime(cx))
            .expect("managed runtime must be bound")
            .version(),
        &Version::parse("0.4.0").expect("test version must be valid")
    );
}

#[gpui::test]
async fn orion_code_update_integration_failed_health_restores_verified_state(
    cx: &mut TestAppContext,
) {
    let harness = init_harness(cx, base_record(), false).await;
    harness.activation.update(cx, |activation, cx| {
        activation.record_staged_identity(staged_identity_from_signed_candidate(), cx);
    });
    cx.run_until_parked();
    harness.activation.update(cx, |activation, cx| {
        activation.activate_when_idle(cx);
    });
    cx.run_until_parked();

    let recovered = durable_record(cx);
    assert_eq!(
        recovered
            .current_verified
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.3.0")
    );
    assert_eq!(
        recovered
            .previous_verified
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.2.0")
    );
    assert!(recovered.staged.is_none());
    assert!(recovered.activation.is_none());
    assert_eq!(
        recovered.last_error_kind,
        Some(OrionCodeUpdateErrorKind::Preflight)
    );
    assert_eq!(
        harness
            .coordinator
            .read_with(cx, |coordinator, _cx| coordinator.record().clone()),
        recovered
    );
    assert!(harness.activation.read_with(cx, |activation, _cx| {
        matches!(
            activation.status(),
            OrionCodeActivationStatus::Failed {
                kind: OrionCodeUpdateErrorKind::Preflight,
                diagnostic_code,
            } if diagnostic_code == "candidate_health_failed"
        )
    }));
    assert_eq!(harness.health_calls.load(Ordering::SeqCst), 1);
    assert_eq!(harness.feed_calls.load(Ordering::SeqCst), 0);
    assert!(
        harness
            .activity
            .read_with(cx, |activity, _cx| activity.is_idle())
    );
    assert_eq!(
        cx.update(|cx| orion_code_managed_archive_runtime(cx))
            .expect("previous runtime must be restored")
            .version(),
        &Version::parse("0.3.0").expect("test version must be valid")
    );
}

#[gpui::test]
async fn orion_code_update_integration_init_recovers_interrupted_activation(
    cx: &mut TestAppContext,
) {
    let mut interrupted = base_record();
    interrupted.staged = Some(staged_receipt_from_signed_candidate());
    let interrupted = begin_orion_code_activation(&interrupted, test_time(), 77)
        .expect("interrupted activation fixture must be valid");
    let harness = init_harness(cx, interrupted, true).await;

    let recovered = durable_record(cx);
    assert_eq!(
        recovered
            .current_verified
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.3.0")
    );
    assert_eq!(
        recovered
            .previous_verified
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.2.0")
    );
    assert!(recovered.staged.is_none());
    assert!(recovered.activation.is_none());
    assert_eq!(
        recovered.last_error_kind,
        Some(OrionCodeUpdateErrorKind::Rollback)
    );
    assert_eq!(
        harness
            .coordinator
            .read_with(cx, |coordinator, _cx| coordinator.record().clone()),
        recovered
    );
    assert!(harness.activation.read_with(cx, |activation, _cx| {
        matches!(
            activation.status(),
            OrionCodeActivationStatus::Failed {
                kind: OrionCodeUpdateErrorKind::Rollback,
                diagnostic_code,
            } if diagnostic_code == "interrupted_activation_rolled_back"
        )
    }));
    assert_eq!(harness.health_calls.load(Ordering::SeqCst), 0);
    assert_eq!(harness.feed_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        cx.update(|cx| orion_code_managed_archive_runtime(cx))
            .expect("verified runtime must remain selected")
            .version(),
        &Version::parse("0.3.0").expect("test version must be valid")
    );
}

#[gpui::test]
async fn orion_code_update_integration_revoked_staged_release_never_activates(
    cx: &mut TestAppContext,
) {
    let staged = staged_receipt_from_signed_candidate();
    let mut revoked = base_record();
    revoked.known_revoked_versions = vec![staged.version.clone()];
    revoked.staged = Some(staged.clone());
    let harness = init_harness(cx, revoked, true).await;

    harness.activation.update(cx, |activation, cx| {
        activation.activate_when_idle(cx);
    });
    cx.run_until_parked();

    let durable = durable_record(cx);
    assert_eq!(durable.staged, Some(staged));
    assert!(durable.activation.is_none());
    assert_eq!(
        durable
            .current_verified
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.3.0")
    );
    assert_eq!(
        durable
            .previous_verified
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.2.0")
    );
    assert!(harness.activation.read_with(cx, |activation, _cx| {
        matches!(
            activation.status(),
            OrionCodeActivationStatus::Failed {
                kind: OrionCodeUpdateErrorKind::Rollback,
                diagnostic_code,
            } if diagnostic_code == "revoked_release_cannot_be_activated"
        )
    }));
    assert_eq!(harness.health_calls.load(Ordering::SeqCst), 0);
    assert_eq!(harness.feed_calls.load(Ordering::SeqCst), 0);
    assert!(
        harness
            .activity
            .read_with(cx, |activity, _cx| activity.is_idle())
    );
    assert_eq!(
        cx.update(|cx| orion_code_managed_archive_runtime(cx))
            .expect("revoked staging must not replace the verified runtime")
            .version(),
        &Version::parse("0.3.0").expect("test version must be valid")
    );
}

#[gpui::test]
async fn revoked_current_waits_for_busy_work_then_activates_safe_previous(cx: &mut TestAppContext) {
    let mut record = base_record();
    record.known_revoked_versions = vec!["0.3.0".to_string()];
    let mut harness = init_harness_with_busy_activity(cx, record, true, true).await;

    assert!(harness.activation.read_with(cx, |activation, _cx| {
        matches!(
            activation.status(),
            OrionCodeActivationStatus::WaitingForIdle { version } if version == "0.2.0"
        )
    }));
    assert!(cx.update(|cx| orion_code_new_connections_are_blocked(cx)));
    assert_eq!(
        durable_record(cx)
            .current_verified
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.3.0")
    );
    harness
        .initial_activity
        .as_mut()
        .expect("busy activity must exist")
        .release()
        .expect("busy activity must release");
    harness.initial_activity = None;
    cx.run_until_parked();

    let durable = durable_record(cx);
    assert_eq!(
        durable
            .current_verified
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.2.0")
    );
    assert_eq!(
        durable
            .previous_verified
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.3.0")
    );
    assert_eq!(durable.app_start_sequence, 1);
    assert_eq!(durable.successful_activation_app_start_sequence, Some(1));
    assert_eq!(harness.health_calls.load(Ordering::SeqCst), 1);
    assert!(!cx.update(|cx| orion_code_new_connections_are_blocked(cx)));
}

#[gpui::test]
async fn revoked_current_without_safe_previous_drains_then_disables_managed_runtime(
    cx: &mut TestAppContext,
) {
    let mut record = base_record();
    record.previous_verified = None;
    record.known_revoked_versions = vec!["0.3.0".to_string()];
    let mut harness = init_harness_with_busy_activity(cx, record, true, true).await;

    assert!(harness.activation.read_with(cx, |activation, _cx| {
        matches!(
            activation.status(),
            OrionCodeActivationStatus::WaitingForIdle { version } if version == "0.3.0"
        )
    }));
    assert!(cx.update(|cx| orion_code_new_connections_are_blocked(cx)));
    harness
        .initial_activity
        .as_mut()
        .expect("busy activity must exist")
        .release()
        .expect("busy activity must release");
    harness.initial_activity = None;
    cx.run_until_parked();

    assert!(harness.activation.read_with(cx, |activation, _cx| {
        matches!(
            activation.status(),
            OrionCodeActivationStatus::Disabled { diagnostic_code }
                if diagnostic_code == "current_release_revoked_no_safe_previous"
        )
    }));
    assert!(
        cx.update(|cx| orion_code_managed_archive_runtime(cx))
            .is_none()
    );
    assert_eq!(harness.health_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        durable_record(cx)
            .current_verified
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.3.0")
    );
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[gpui::test]
async fn signed_index_download_stage_busy_barrier_and_activation_commit_end_to_end(
    cx: &mut TestAppContext,
) {
    let archive = e2e_archive_bytes();
    let verifier = Arc::new(
        OrionCodeIndexVerifier::new(
            [("e2e-test-key".to_string(), E2E_VERIFYING_KEY)],
            [TEST_ARCHIVE_HOST.to_string()],
        )
        .expect("e2e verifier configuration must be valid"),
    );
    let directly_verified = verifier
        .verify(
            E2E_INDEX_BYTES,
            E2E_SIGNATURE_ENVELOPE_BYTES,
            test_time(),
            42,
        )
        .expect("the exact e2e index bytes must pass Ed25519 verification");
    assert_eq!(directly_verified.index.sequence, E2E_INDEX_SEQUENCE);
    assert_eq!(directly_verified.payload_sha256, sha256(E2E_INDEX_BYTES));
    let mut altered_index_bytes = E2E_INDEX_BYTES.to_vec();
    altered_index_bytes.push(b'\n');
    assert!(
        verifier
            .verify(
                &altered_index_bytes,
                E2E_SIGNATURE_ENVELOPE_BYTES,
                test_time(),
                42,
            )
            .is_err(),
        "even a trailing newline must invalidate the exact-byte signature"
    );

    let harness = init_harness(cx, base_record(), true).await;
    let signed_feed_calls = Arc::new(AtomicUsize::new(0));
    let verified_payloads = Arc::new(Mutex::new(Vec::new()));
    harness.coordinator.update(cx, |coordinator, cx| {
        coordinator.configure_feed(
            Arc::new(ExactBytesSignedFeed {
                calls: signed_feed_calls.clone(),
                verifier,
                verified_payloads: verified_payloads.clone(),
            }),
            cx,
        );
    });

    let temporary = TempDir::new().expect("e2e managed root must be temporary");
    let managed_root = temporary.path().join("managed");
    let download_gate = Arc::new(DownloadGate::default());
    let archive_http_calls = Arc::new(AtomicUsize::new(0));
    let platform_calls = Arc::new(AtomicUsize::new(0));
    let preflight_calls = Arc::new(AtomicUsize::new(0));
    let archive_installer = Arc::new(
        OrionCodeArchiveInstaller::new(
            &managed_root,
            e2e_archive_http_client(
                archive.clone(),
                download_gate.clone(),
                archive_http_calls.clone(),
            ),
            [TEST_ARCHIVE_HOST.to_string()],
            Arc::new(FixturePlatformVerifier {
                calls: platform_calls.clone(),
            }),
            Some(Arc::new(FixtureAcpPreflight {
                calls: preflight_calls.clone(),
            })),
        )
        .expect("e2e archive installer must be valid"),
    );
    let pipeline = cx.update(|cx| {
        OrionCodeUpdatePipeline::init_global_with_installer(
            Some(Arc::new(OrionCodeArchiveInstallerHandle::new(
                archive_installer,
            ))),
            cx,
        )
    });

    harness.coordinator.update(cx, |coordinator, cx| {
        coordinator.check_now(cx);
    });
    cx.run_until_parked();

    assert_eq!(signed_feed_calls.load(Ordering::SeqCst), 1);
    assert_eq!(harness.feed_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        verified_payloads.lock().as_slice(),
        [sha256(E2E_INDEX_BYTES)].as_slice()
    );
    let candidate = harness
        .coordinator
        .read_with(cx, |coordinator, _cx| {
            coordinator
                .latest_resolution()
                .and_then(|resolution| resolution.candidate.clone())
        })
        .expect("coordinator must resolve the signed e2e candidate");
    assert_eq!(candidate.version, Version::parse("0.4.0").expect("version"));
    assert_eq!(candidate.target_name, TEST_TARGET);
    assert_eq!(candidate.target.archive_sha256, E2E_ARCHIVE_SHA256);
    assert_eq!(candidate.target.manifest_sha256, E2E_MANIFEST_SHA256);
    assert_eq!(candidate.target.sbom_sha256, E2E_SBOM_SHA256);
    assert_eq!(candidate.target.archive_bytes, archive.len() as u64);
    assert_eq!(
        durable_record(cx).highest_index_sequence,
        E2E_INDEX_SEQUENCE
    );

    let mut busy_owner = harness
        .activity
        .read_with(cx, |activity, _cx| activity.owner());
    busy_owner
        .set_session_activity(1, 0, 0)
        .expect("e2e busy barrier must acquire one active turn");
    pipeline.update(cx, |pipeline, cx| pipeline.download_available(cx));
    cx.run_until_parked();

    let partial_progress = pipeline.read_with(cx, |pipeline, _cx| pipeline.status().clone());
    assert!(matches!(
        partial_progress,
        OrionCodeUpdatePipelineStatus::Downloading {
            version,
            archive_bytes,
            received_bytes,
            total_bytes,
        } if version == "0.4.0"
            && archive_bytes == archive.len() as u64
            && received_bytes >= E2E_DOWNLOAD_GATE_BYTES as u64
            && received_bytes < total_bytes
            && total_bytes == archive.len() as u64
    ));
    assert_eq!(archive_http_calls.load(Ordering::SeqCst), 1);
    assert_eq!(platform_calls.load(Ordering::SeqCst), 0);
    assert_eq!(preflight_calls.load(Ordering::SeqCst), 0);
    let downloading_record = durable_record(cx);
    assert_eq!(
        downloading_record
            .current_verified
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.3.0")
    );
    assert!(downloading_record.staged.is_none());

    let download_task = pipeline
        .update(cx, |pipeline, _cx| pipeline.take_active_download_for_test())
        .expect("e2e download task must remain retained by the pipeline");
    download_gate.release();
    let executor = cx.executor();
    executor.allow_parking();
    download_task.await;
    executor.forbid_parking();
    cx.run_until_parked();

    assert_eq!(
        platform_calls.load(Ordering::SeqCst),
        1,
        "pipeline status after releasing download gate: {:?}",
        pipeline.read_with(cx, |pipeline, _cx| pipeline.status().clone())
    );
    assert_eq!(preflight_calls.load(Ordering::SeqCst), 1);
    assert!(pipeline.read_with(cx, |pipeline, _cx| {
        matches!(
            pipeline.status(),
            OrionCodeUpdatePipelineStatus::Staged { version } if version == "0.4.0"
        )
    }));
    let artifact_root = managed_root.join("versions/0.4.0/darwin-aarch64");
    let receipt_path = managed_root.join("receipts/0.4.0-darwin-aarch64.json");
    assert!(artifact_root.join(TEST_COMMAND).is_file());
    let installed_receipt: OrionCodeInstallReceiptV2 = serde_json::from_slice(
        &std::fs::read(&receipt_path).expect("installer receipt must be readable"),
    )
    .expect("installer receipt must be valid JSON");
    assert_eq!(installed_receipt.version, "0.4.0");
    assert_eq!(
        installed_receipt.archive_sha256.as_deref(),
        Some(E2E_ARCHIVE_SHA256)
    );
    assert_eq!(installed_receipt.index_sequence, Some(E2E_INDEX_SEQUENCE));

    let staged_record = durable_record(cx);
    assert_eq!(
        staged_record
            .current_verified
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.3.0")
    );
    assert_eq!(
        staged_record
            .previous_verified
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.2.0")
    );
    assert_eq!(
        staged_record
            .staged
            .as_ref()
            .map(|receipt| receipt.version.as_str()),
        Some("0.4.0")
    );
    assert!(staged_record.activation.is_none());

    harness.activation.update(cx, |activation, cx| {
        activation.activate_when_idle(cx);
    });
    assert!(harness.activation.read_with(cx, |activation, _cx| {
        matches!(
            activation.status(),
            OrionCodeActivationStatus::WaitingForIdle { version } if version == "0.4.0"
        )
    }));
    let waiting_record = durable_record(cx);
    assert_eq!(
        waiting_record.current_verified,
        staged_record.current_verified
    );
    assert_eq!(
        waiting_record.previous_verified,
        staged_record.previous_verified
    );
    assert_eq!(waiting_record.staged, staged_record.staged);
    assert!(waiting_record.activation.is_none());
    assert_eq!(harness.health_calls.load(Ordering::SeqCst), 0);

    busy_owner
        .release()
        .expect("e2e busy barrier must release the active turn");
    cx.run_until_parked();

    let committed = durable_record(cx);
    let current = committed
        .current_verified
        .as_ref()
        .expect("candidate receipt must become current atomically");
    let previous = committed
        .previous_verified
        .as_ref()
        .expect("old current receipt must become previous atomically");
    assert_eq!(current.version, "0.4.0");
    assert_eq!(current.source, OrionCodeInstallSource::Archive);
    assert_eq!(current.archive_sha256.as_deref(), Some(E2E_ARCHIVE_SHA256));
    assert_eq!(current.index_sequence, Some(E2E_INDEX_SEQUENCE));
    assert_eq!(previous.version, "0.3.0");
    assert!(committed.staged.is_none());
    assert!(committed.activation.is_none());
    assert_eq!(harness.health_calls.load(Ordering::SeqCst), 1);
    assert!(
        harness
            .activity
            .read_with(cx, |activity, _cx| activity.is_idle())
    );
    assert!(harness.activation.read_with(cx, |activation, _cx| {
        matches!(
            activation.status(),
            OrionCodeActivationStatus::Activated { version } if version == "0.4.0"
        )
    }));
    assert_eq!(
        harness
            .coordinator
            .read_with(cx, |coordinator, _cx| coordinator.record().clone()),
        committed
    );
}
