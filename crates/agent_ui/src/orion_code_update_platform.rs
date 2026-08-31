use std::{
    collections::HashSet,
    fs::File,
    io::Read as _,
    path::{Path, PathBuf},
    sync::Arc,
};

use futures::{FutureExt as _, future::BoxFuture};
use orion_code_update::{OrionCodeArtifactManifestV1, OrionCodeSigningRequirement};
use thiserror::Error;
use util::command::new_command;

const MAX_TOOL_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrionCodeMacPlatformTrust {
    pub team_id: String,
    pub bundle_id: String,
}

impl OrionCodeMacPlatformTrust {
    pub fn new(team_id: impl Into<String>, bundle_id: impl Into<String>) -> Result<Self, String> {
        let team_id = team_id.into();
        if team_id.len() != 10
            || !team_id
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        {
            return Err("Developer ID Team ID must contain 10 uppercase letters or digits".into());
        }
        let bundle_id = bundle_id.into();
        if bundle_id.is_empty()
            || bundle_id.len() > 255
            || !bundle_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
            || !bundle_id.contains('.')
        {
            return Err("sidecar bundle identifier is invalid".into());
        }
        Ok(Self { team_id, bundle_id })
    }
}

#[derive(Debug, Error)]
pub enum OrionCodePlatformVerificationError {
    #[error("Orion Code platform verification is unsupported for this target")]
    UnsupportedPlatform,
    #[error("invalid Orion Code platform verification input: {0}")]
    InvalidInput(String),
    #[error("Orion Code platform verification tool {tool} failed with exit code {exit_code:?}")]
    ToolFailed {
        tool: &'static str,
        exit_code: Option<i32>,
    },
    #[error("Orion Code code-signing metadata was rejected for {component}: {reason}")]
    TrustMismatch { component: String, reason: String },
    #[error("failed to inspect Orion Code executable {component}: {reason}")]
    Inspection { component: String, reason: String },
}

pub trait OrionCodePlatformVerifier: Send + Sync {
    fn verify(
        &self,
        artifact_root: PathBuf,
        manifest: OrionCodeArtifactManifestV1,
        signing_requirement: OrionCodeSigningRequirement,
    ) -> BoxFuture<'static, Result<(), OrionCodePlatformVerificationError>>;
}

pub struct OrionCodeMacPlatformVerifier {
    trust: OrionCodeMacPlatformTrust,
    runner: Arc<dyn PlatformCommandRunner>,
}

impl OrionCodeMacPlatformVerifier {
    pub fn new(trust: OrionCodeMacPlatformTrust) -> Self {
        Self {
            trust,
            runner: Arc::new(SystemPlatformCommandRunner),
        }
    }

    #[cfg(test)]
    fn with_runner(
        trust: OrionCodeMacPlatformTrust,
        runner: Arc<dyn PlatformCommandRunner>,
    ) -> Self {
        Self { trust, runner }
    }

    async fn verify_mac_artifact(
        &self,
        artifact_root: &Path,
        manifest: &OrionCodeArtifactManifestV1,
    ) -> Result<(), OrionCodePlatformVerificationError> {
        let bundle_relative = bundle_path_from_command(&manifest.command)?;
        let bundle_path = artifact_root.join(&bundle_relative);
        if !bundle_path.is_dir() {
            return Err(OrionCodePlatformVerificationError::InvalidInput(
                "sidecar app bundle is missing".to_string(),
            ));
        }

        let mut signed_components = Vec::new();
        let mut observed = HashSet::new();
        for file in &manifest.files {
            let path = artifact_root.join(&file.path);
            if is_mach_o(&path, &file.path)? && observed.insert(file.path.clone()) {
                signed_components.push((file.path.clone(), path));
            }
        }
        if signed_components.is_empty() {
            return Err(OrionCodePlatformVerificationError::InvalidInput(
                "sidecar archive contains no Mach-O executable".to_string(),
            ));
        }
        for (label, component) in signed_components {
            self.verify_signed_component(&component, &label, false)
                .await?;
        }
        self.verify_signed_component(&bundle_path, &bundle_relative, true)
            .await?;
        self.run_checked(
            "spctl",
            "/usr/sbin/spctl",
            &["--assess", "--type", "execute", "--verbose=4"],
            &bundle_path,
        )
        .await?;
        self.run_checked(
            "stapler",
            "/usr/bin/xcrun",
            &["stapler", "validate"],
            &bundle_path,
        )
        .await?;
        Ok(())
    }

    async fn verify_signed_component(
        &self,
        path: &Path,
        component: &str,
        require_bundle_id: bool,
    ) -> Result<(), OrionCodePlatformVerificationError> {
        self.run_checked(
            "codesign",
            "/usr/bin/codesign",
            &["--verify", "--strict", "--verbose=4"],
            path,
        )
        .await?;
        let metadata = self
            .run_checked(
                "codesign",
                "/usr/bin/codesign",
                &["--display", "--verbose=4"],
                path,
            )
            .await?;
        let metadata = String::from_utf8_lossy(&metadata);
        require_metadata_value(&metadata, "TeamIdentifier", &self.trust.team_id, component)?;
        if require_bundle_id {
            require_metadata_value(&metadata, "Identifier", &self.trust.bundle_id, component)?;
        }
        if !metadata.lines().any(|line| {
            line.strip_prefix("flags=")
                .is_some_and(|flags| flags.contains("runtime"))
        }) {
            return Err(trust_mismatch(
                component,
                "Hardened Runtime flag is missing",
            ));
        }
        if !metadata
            .lines()
            .any(|line| line.starts_with("Timestamp=") || line.starts_with("Signed Time="))
        {
            return Err(trust_mismatch(component, "secure timestamp is missing"));
        }

        let entitlements = self
            .run_checked(
                "codesign",
                "/usr/bin/codesign",
                &["--display", "--entitlements", ":-"],
                path,
            )
            .await?;
        if String::from_utf8_lossy(&entitlements).contains("com.apple.security.get-task-allow") {
            return Err(trust_mismatch(
                component,
                "get-task-allow entitlement is forbidden",
            ));
        }
        Ok(())
    }

    async fn run_checked(
        &self,
        tool: &'static str,
        program: &'static str,
        arguments: &[&str],
        path: &Path,
    ) -> Result<Vec<u8>, OrionCodePlatformVerificationError> {
        let output = self
            .runner
            .run(
                program,
                arguments
                    .iter()
                    .map(|argument| argument.to_string())
                    .collect(),
                path.to_path_buf(),
            )
            .await
            .map_err(|reason| OrionCodePlatformVerificationError::Inspection {
                component: tool.to_string(),
                reason,
            })?;
        if !output.success {
            return Err(OrionCodePlatformVerificationError::ToolFailed {
                tool,
                exit_code: output.exit_code,
            });
        }
        let combined_length = output.stdout.len().saturating_add(output.stderr.len());
        if combined_length > MAX_TOOL_OUTPUT_BYTES {
            return Err(OrionCodePlatformVerificationError::Inspection {
                component: tool.to_string(),
                reason: "verification output exceeded its byte limit".to_string(),
            });
        }
        let mut combined = output.stdout;
        combined.extend_from_slice(&output.stderr);
        Ok(combined)
    }
}

impl OrionCodePlatformVerifier for OrionCodeMacPlatformVerifier {
    fn verify(
        &self,
        artifact_root: PathBuf,
        manifest: OrionCodeArtifactManifestV1,
        signing_requirement: OrionCodeSigningRequirement,
    ) -> BoxFuture<'static, Result<(), OrionCodePlatformVerificationError>> {
        let verifier = Self {
            trust: self.trust.clone(),
            runner: self.runner.clone(),
        };
        async move {
            if signing_requirement != OrionCodeSigningRequirement::DeveloperIdAndNotarized {
                return Err(OrionCodePlatformVerificationError::InvalidInput(
                    "macOS Stable requires Developer ID and notarization".to_string(),
                ));
            }
            #[cfg(target_os = "macos")]
            {
                verifier
                    .verify_mac_artifact(&artifact_root, &manifest)
                    .await
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (verifier, artifact_root, manifest);
                Err(OrionCodePlatformVerificationError::UnsupportedPlatform)
            }
        }
        .boxed()
    }
}

struct PlatformCommandOutput {
    success: bool,
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

trait PlatformCommandRunner: Send + Sync {
    fn run(
        &self,
        program: &'static str,
        arguments: Vec<String>,
        path: PathBuf,
    ) -> BoxFuture<'static, Result<PlatformCommandOutput, String>>;
}

struct SystemPlatformCommandRunner;

impl PlatformCommandRunner for SystemPlatformCommandRunner {
    fn run(
        &self,
        program: &'static str,
        arguments: Vec<String>,
        path: PathBuf,
    ) -> BoxFuture<'static, Result<PlatformCommandOutput, String>> {
        async move {
            let output = new_command(program)
                .args(arguments)
                .arg(path)
                .output()
                .await
                .map_err(|error| error.to_string())?;
            Ok(PlatformCommandOutput {
                success: output.status.success(),
                exit_code: output.status.code(),
                stdout: output.stdout,
                stderr: output.stderr,
            })
        }
        .boxed()
    }
}

fn bundle_path_from_command(command: &str) -> Result<String, OrionCodePlatformVerificationError> {
    let components = command.split('/').collect::<Vec<_>>();
    let first = components.first().copied().unwrap_or_default();
    if !first.ends_with(".app") || first.is_empty() {
        return Err(OrionCodePlatformVerificationError::InvalidInput(
            "macOS command must be inside an app-like sidecar bundle".to_string(),
        ));
    }
    if components.get(1..3) != Some(["Contents", "MacOS"].as_slice())
        || components
            .get(3)
            .is_none_or(|executable| executable.is_empty())
    {
        return Err(OrionCodePlatformVerificationError::InvalidInput(
            "macOS command must be inside Contents/MacOS".to_string(),
        ));
    }
    Ok(first.to_string())
}

fn is_mach_o(path: &Path, component: &str) -> Result<bool, OrionCodePlatformVerificationError> {
    let mut file =
        File::open(path).map_err(|error| OrionCodePlatformVerificationError::Inspection {
            component: component.to_string(),
            reason: error.to_string(),
        })?;
    let mut magic = [0_u8; 4];
    let count =
        file.read(&mut magic)
            .map_err(|error| OrionCodePlatformVerificationError::Inspection {
                component: component.to_string(),
                reason: error.to_string(),
            })?;
    if count < magic.len() {
        return Ok(false);
    }
    Ok(matches!(
        magic,
        [0xfe, 0xed, 0xfa, 0xce]
            | [0xce, 0xfa, 0xed, 0xfe]
            | [0xfe, 0xed, 0xfa, 0xcf]
            | [0xcf, 0xfa, 0xed, 0xfe]
            | [0xca, 0xfe, 0xba, 0xbe]
            | [0xbe, 0xba, 0xfe, 0xca]
            | [0xca, 0xfe, 0xba, 0xbf]
            | [0xbf, 0xba, 0xfe, 0xca]
    ))
}

fn require_metadata_value(
    metadata: &str,
    key: &str,
    expected: &str,
    component: &str,
) -> Result<(), OrionCodePlatformVerificationError> {
    let actual = metadata
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .ok_or_else(|| trust_mismatch(component, format!("{key} is missing")))?;
    if actual != expected {
        return Err(trust_mismatch(component, format!("{key} does not match")));
    }
    Ok(())
}

fn trust_mismatch(
    component: &str,
    reason: impl Into<String>,
) -> OrionCodePlatformVerificationError {
    OrionCodePlatformVerificationError::TrustMismatch {
        component: component.to_string(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use chrono::{DateTime, Utc};
    use tempfile::TempDir;

    use super::*;
    use orion_code_update::OrionCodeArtifactFileV1;

    struct FakeRunner {
        calls: Mutex<Vec<(String, Vec<String>)>>,
        metadata: String,
        entitlements: String,
    }

    impl PlatformCommandRunner for FakeRunner {
        fn run(
            &self,
            program: &'static str,
            arguments: Vec<String>,
            _path: PathBuf,
        ) -> BoxFuture<'static, Result<PlatformCommandOutput, String>> {
            self.calls
                .lock()
                .expect("calls lock")
                .push((program.to_string(), arguments.clone()));
            let output = if arguments
                .iter()
                .any(|argument| argument == "--entitlements")
            {
                self.entitlements.as_bytes()
            } else if arguments.iter().any(|argument| argument == "--display") {
                self.metadata.as_bytes()
            } else {
                &[]
            };
            let output = PlatformCommandOutput {
                success: true,
                exit_code: Some(0),
                stdout: output.to_vec(),
                stderr: Vec::new(),
            };
            async move { Ok(output) }.boxed()
        }
    }

    fn manifest(command_path: &str) -> OrionCodeArtifactManifestV1 {
        OrionCodeArtifactManifestV1 {
            schema_version: 1,
            version: "0.4.0".to_string(),
            git_sha: "a".repeat(40),
            target: "darwin-aarch64".to_string(),
            built_at: DateTime::parse_from_rfc3339("2026-09-01T00:00:00Z")
                .expect("timestamp")
                .with_timezone(&Utc),
            acp_protocol: 1,
            studio_version_requirement: ">=0.1.0,<1.0.0".to_string(),
            node_version: "22.22.0".to_string(),
            node_abi: "127".to_string(),
            native_modules: Vec::new(),
            command: command_path.to_string(),
            sbom_path: "SBOM.cdx.json".to_string(),
            sbom_sha256: "a".repeat(64),
            notices_path: "THIRD_PARTY_NOTICES".to_string(),
            notices_sha256: "b".repeat(64),
            files: vec![OrionCodeArtifactFileV1 {
                path: command_path.to_string(),
                mode: 0o755,
                bytes: 4,
                sha256: "c".repeat(64),
            }],
        }
    }

    #[test]
    fn mac_verifier_checks_nested_code_bundle_gatekeeper_and_staple_without_deep() {
        if !cfg!(target_os = "macos") {
            return;
        }
        let temporary = TempDir::new().expect("temporary directory");
        let command_path = "OrionCodeSidecar.app/Contents/MacOS/orion-code-acp";
        let command = temporary.path().join(command_path);
        std::fs::create_dir_all(command.parent().expect("command parent"))
            .expect("create app bundle");
        std::fs::write(&command, [0xcf, 0xfa, 0xed, 0xfe]).expect("write Mach-O marker");
        let runner = Arc::new(FakeRunner {
            calls: Mutex::new(Vec::new()),
            metadata: concat!(
                "Identifier=com.orionagents.code.sidecar\n",
                "TeamIdentifier=ABCDEFGHIJ\n",
                "flags=0x10000(runtime)\n",
                "Timestamp=Sep 1, 2026\n"
            )
            .to_string(),
            entitlements: "<?xml version=\"1.0\"?><plist><dict/></plist>".to_string(),
        });
        let verifier = OrionCodeMacPlatformVerifier::with_runner(
            OrionCodeMacPlatformTrust::new("ABCDEFGHIJ", "com.orionagents.code.sidecar")
                .expect("trust"),
            runner.clone(),
        );
        futures::executor::block_on(verifier.verify(
            temporary.path().to_path_buf(),
            manifest(command_path),
            OrionCodeSigningRequirement::DeveloperIdAndNotarized,
        ))
        .expect("verify fixture");
        let calls = runner.calls.lock().expect("calls lock");
        assert!(
            calls
                .iter()
                .all(|(_, arguments)| !arguments.iter().any(|argument| argument == "--deep"))
        );
        assert!(calls.iter().any(|(program, _)| program.ends_with("spctl")));
        assert!(calls.iter().any(|(_, arguments)| {
            arguments
                .first()
                .is_some_and(|argument| argument == "stapler")
        }));
    }

    #[test]
    fn rejects_get_task_allow_and_wrong_identity() {
        let metadata = concat!(
            "Identifier=com.attacker.sidecar\n",
            "TeamIdentifier=ATTACKER00\n",
            "flags=0x10000(runtime)\n",
            "Timestamp=Sep 1, 2026\n"
        );
        assert!(
            require_metadata_value(metadata, "TeamIdentifier", "ABCDEFGHIJ", "binary").is_err()
        );
        assert!(OrionCodeMacPlatformTrust::new("short", "com.orion.sidecar").is_err());
    }
}
