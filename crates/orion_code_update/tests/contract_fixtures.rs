use std::{
    error::Error,
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signer as _, SigningKey};
use orion_code_update::{
    OrionCodeIndexVerifier, OrionCodeUpdateContractError, OrionCodeUpdateSignatureEnvelopeV1,
    SIGNATURE_ENVELOPE_SCHEMA_VERSION,
};
use sha2::{Digest as _, Sha256};

const CONTRACT_SHA256: &str = "5066d476b9c47cca4491b6e8374f7fe533758d9bf0259c66f78c9aadff65831a";
const EXPECTED_MANIFEST: &str = concat!(
    "00b722f03d14460c753090bf0734a6d5eeeca6a1b69818cd67fc45661e98338a  artifact-manifest-v1.schema.json\n",
    "e81af4f630a47be22b0293e46befe3286d9945f9c0e645cccd03544eb78542ab  golden/valid-index.json\n",
    "cf57bf252975ee1a423299b4e00c2304eb9edf1980e73362bc5502ba89c17d80  golden/valid-signature-envelope.json\n",
    "eafd7f31ef00225cf70fb342cb38b470ee292276361c994c4cfad651cc78a57b  install-receipt-v2.schema.json\n",
    "5cf280f62bdf33ea510354d4cff6357c53ca97b7e503fd1d8569f736f027444b  invalid/duplicate-version.json\n",
    "e3dc095b03aa92b38447c17ec9d01a245a1947b6946f9f5effb550a02f79f034  invalid/http-url.json\n",
    "ae85bb9721d5fad7b3c22c48b79d23636c6ef4508311abb7e784fea632064140  invalid/missing-target.json\n",
    "79e54f6447788f85d4c9a245413824872fa992d2588eaa8448c73153104ffda5  invalid/path-traversal.json\n",
    "167140ad04b6265ce2e5640db29c3e9330f285e9b5f3a27350f99221cd089cab  invalid/rollback-abuse.json\n",
    "8e7d3d490fc17926c68014a1ad622f876992e1f83664f4e732a1d0212a7acf6b  invalid/rollout-10001.json\n",
    "8aa52f74cd67ac064fab830e9a39ea7a08bc47f87068d9df4612a9b24cd6fb20  invalid/stable-prerelease.json\n",
    "029428c3c56f866431f978c6cf1df410b2a60f6425154947dd517c16e50aaab3  invalid/unknown-field.json\n",
    "6645bfa0ec88f76d7ed2e22c17a0db9c37783b03fed0c59e02c4dc8d4d76adce  signature-envelope-v1.schema.json\n",
    "587a248b78bd1cd959e9631d97ee205af41f29da2f2c53ceb8af5d1141230eb6  update-index-v1.schema.json\n",
);
const TEST_KEY_ID: &str = "up-con-01-runtime-test-key";
const TEST_ARCHIVE_HOST: &str = "updates.example.invalid";

#[derive(Clone, Copy)]
enum ExpectedContractError {
    InvalidIndexJson,
    InvalidRelease,
}

const INVALID_INDEX_FIXTURES: [(&str, ExpectedContractError); 8] = [
    (
        "invalid/duplicate-version.json",
        ExpectedContractError::InvalidRelease,
    ),
    (
        "invalid/http-url.json",
        ExpectedContractError::InvalidRelease,
    ),
    (
        "invalid/missing-target.json",
        ExpectedContractError::InvalidRelease,
    ),
    (
        "invalid/path-traversal.json",
        ExpectedContractError::InvalidRelease,
    ),
    (
        "invalid/rollback-abuse.json",
        ExpectedContractError::InvalidRelease,
    ),
    (
        "invalid/rollout-10001.json",
        ExpectedContractError::InvalidRelease,
    ),
    (
        "invalid/stable-prerelease.json",
        ExpectedContractError::InvalidRelease,
    ),
    (
        "invalid/unknown-field.json",
        ExpectedContractError::InvalidIndexJson,
    ),
];

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn contract_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("contracts/orion-code-update-v1")
}

fn contract_fixture(relative_path: &str) -> PathBuf {
    contract_root().join(relative_path)
}

fn collect_json_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
) -> TestResult {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_json_files(root, &path, files)?;
        } else if file_type.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == OsStr::new("json"))
        {
            let relative_path = path.strip_prefix(root)?;
            let mut components = Vec::new();
            for component in relative_path.components() {
                let component = component.as_os_str().to_str().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("contract path is not UTF-8: {}", path.display()),
                    )
                })?;
                components.push(component);
            }
            files.push((components.join("/"), fs::read(path)?));
        }
    }
    Ok(())
}

fn contract_manifest() -> TestResult<(String, usize)> {
    let root = contract_root();
    let mut files = Vec::new();
    collect_json_files(&root, &root, &mut files)?;
    files.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));

    let mut manifest = String::new();
    for (relative_path, bytes) in &files {
        manifest.push_str(&format!("{}  {relative_path}\n", lowercase_sha256(bytes)));
    }
    Ok((manifest, files.len()))
}

fn lowercase_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn generate_test_signing_key() -> SigningKey {
    let mut seed_hasher = Sha256::new();
    seed_hasher.update(b"UP-CON-01 Studio consumer contract test");
    seed_hasher.update(std::process::id().to_le_bytes());
    let secret_key_bytes: [u8; 32] = seed_hasher.finalize().into();
    SigningKey::from_bytes(&secret_key_bytes)
}

fn test_verifier(
    signing_key: &SigningKey,
) -> Result<OrionCodeIndexVerifier, OrionCodeUpdateContractError> {
    OrionCodeIndexVerifier::new(
        [(
            TEST_KEY_ID.to_string(),
            signing_key.verifying_key().to_bytes(),
        )],
        [TEST_ARCHIVE_HOST.to_string()],
    )
}

fn sign_index(signing_key: &SigningKey, index_bytes: &[u8]) -> Result<Vec<u8>, serde_json::Error> {
    let envelope = OrionCodeUpdateSignatureEnvelopeV1 {
        schema_version: SIGNATURE_ENVELOPE_SCHEMA_VERSION,
        algorithm: "ed25519".to_string(),
        key_id: TEST_KEY_ID.to_string(),
        signature: BASE64_STANDARD.encode(signing_key.sign(index_bytes).to_bytes()),
    };
    serde_json::to_vec(&envelope)
}

fn test_now() -> Result<DateTime<Utc>, chrono::ParseError> {
    Ok(DateTime::parse_from_rfc3339("2026-09-01T00:00:00Z")?.with_timezone(&Utc))
}

#[test]
fn vendored_contract_manifest_matches_producer_digest() -> TestResult {
    let (manifest, file_count) = contract_manifest()?;
    assert_eq!(file_count, 14);
    assert_eq!(manifest, EXPECTED_MANIFEST);
    assert_eq!(lowercase_sha256(manifest.as_bytes()), CONTRACT_SHA256);
    Ok(())
}

#[test]
fn valid_index_accepts_a_runtime_generated_signature() -> TestResult {
    let index_bytes = fs::read(contract_fixture("golden/valid-index.json"))?;
    let signing_key = generate_test_signing_key();
    let verifier = test_verifier(&signing_key)?;
    let signature_envelope = sign_index(&signing_key, &index_bytes)?;

    let verified = verifier.verify(&index_bytes, &signature_envelope, test_now()?, 0)?;
    assert_eq!(verified.index.sequence, 42);
    assert_eq!(verified.key_id, TEST_KEY_ID);
    assert_eq!(verified.payload_sha256, lowercase_sha256(&index_bytes));
    Ok(())
}

#[test]
fn schema_only_signature_fixture_does_not_authenticate_index() -> TestResult {
    let index_bytes = fs::read(contract_fixture("golden/valid-index.json"))?;
    let schema_envelope_bytes = fs::read(contract_fixture("golden/valid-signature-envelope.json"))?;
    let signing_key = generate_test_signing_key();
    let verifier = test_verifier(&signing_key)?;

    let unknown_key_result = verifier.verify(&index_bytes, &schema_envelope_bytes, test_now()?, 0);
    assert!(
        matches!(
            &unknown_key_result,
            Err(OrionCodeUpdateContractError::UnknownSigningKey(key_id))
                if key_id == "schema-fixture-not-a-real-key"
        ),
        "schema-only envelope was not rejected as an untrusted key: {unknown_key_result:?}"
    );

    let mut schema_envelope: OrionCodeUpdateSignatureEnvelopeV1 =
        serde_json::from_slice(&schema_envelope_bytes)?;
    schema_envelope.key_id = TEST_KEY_ID.to_string();
    let retargeted_envelope = serde_json::to_vec(&schema_envelope)?;
    let invalid_signature_result =
        verifier.verify(&index_bytes, &retargeted_envelope, test_now()?, 0);
    assert!(
        matches!(
            &invalid_signature_result,
            Err(OrionCodeUpdateContractError::InvalidSignature)
        ),
        "schema-only signature unexpectedly authenticated the golden index: {invalid_signature_result:?}"
    );
    Ok(())
}

#[test]
fn runtime_signed_invalid_indexes_reach_contract_validation_and_are_rejected() -> TestResult {
    let signing_key = generate_test_signing_key();
    let verifier = test_verifier(&signing_key)?;

    for (relative_path, expected_error) in INVALID_INDEX_FIXTURES {
        let index_bytes = fs::read(contract_fixture(relative_path))?;
        let signature_envelope = sign_index(&signing_key, &index_bytes)?;
        let verification = verifier.verify(&index_bytes, &signature_envelope, test_now()?, 0);
        let rejected_at_expected_layer = match expected_error {
            ExpectedContractError::InvalidIndexJson => matches!(
                &verification,
                Err(OrionCodeUpdateContractError::InvalidIndexJson(_))
            ),
            ExpectedContractError::InvalidRelease => matches!(
                &verification,
                Err(OrionCodeUpdateContractError::InvalidRelease { .. })
            ),
        };
        assert!(
            rejected_at_expected_layer,
            "{relative_path} was accepted or rejected before index contract validation: {verification:?}"
        );
    }
    Ok(())
}
