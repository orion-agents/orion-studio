use std::{cell::RefCell, ops::Not};

use gh_workflow::{
    Concurrency, Env, Expression, Step, WorkflowCallInput, WorkflowCallSecret,
    WorkflowDispatchInput,
};

use crate::tasks::workflows::{runners::Platform, steps::NamedJob};

macro_rules! secret {
    ($secret_name:ident) => {
        pub const $secret_name: &str = concat!("${{ secrets.", stringify!($secret_name), " }}");
    };
}

macro_rules! var {
    ($var_name:ident) => {
        pub const $var_name: &str = concat!("${{ vars.", stringify!($var_name), " }}");
    };
}

secret!(APPLE_NOTARIZATION_ISSUER_ID);
secret!(APPLE_NOTARIZATION_KEY);
secret!(APPLE_NOTARIZATION_KEY_ID);
secret!(AZURE_SIGNING_CLIENT_ID);
secret!(AZURE_SIGNING_CLIENT_SECRET);
secret!(AZURE_SIGNING_TENANT_ID);
secret!(CACHIX_AUTH_TOKEN);
secret!(DIGITALOCEAN_ACCESS_TOKEN);
secret!(DIGITALOCEAN_SPACES_ACCESS_KEY);
secret!(DIGITALOCEAN_SPACES_SECRET_KEY);
secret!(GITHUB_TOKEN);
secret!(MACOS_CERTIFICATE);
secret!(MACOS_CERTIFICATE_PASSWORD);
secret!(ORION_STUDIO_CLIENT_CHECKSUM_SEED);
secret!(ORION_STUDIO_AUTOMATION_APP_ID);
secret!(ORION_STUDIO_AUTOMATION_APP_PRIVATE_KEY);
secret!(ORION_STUDIO_CLOUD_PROVIDER_ADDITIONAL_MODELS_JSON);
secret!(ORION_STUDIO_MACOS_PROVISIONING_PROFILE_BASE64);
secret!(ORION_STUDIO_REVALIDATE_TOKEN);
secret!(ORION_STUDIO_SENTRY_MINIDUMP_ENDPOINT);
secret!(SENTRY_AUTH_TOKEN);
secret!(DISCORD_WEBHOOK_RELEASE_NOTES);
secret!(WINGET_TOKEN);
secret!(SLACK_WEBHOOK_WORKFLOW_FAILURES);
secret!(R2_ACCOUNT_ID);
secret!(R2_ACCESS_KEY_ID);
secret!(R2_SECRET_ACCESS_KEY);
secret!(CLOUDFLARE_API_TOKEN);
secret!(CLOUDFLARE_ACCOUNT_ID);
secret!(DOCS_AMPLITUDE_API_KEY);
secret!(DOCS_CONSENT_IO_INSTANCE);

// todo(ci) make these secrets too...
var!(AZURE_SIGNING_ACCOUNT_NAME);
var!(AZURE_SIGNING_CERT_PROFILE_NAME);
var!(AZURE_SIGNING_ENDPOINT);
var!(ORION_STUDIO_DOCS_NIGHTLY_ORIGIN);
var!(ORION_STUDIO_DOCS_NIGHTLY_PROJECT);
var!(ORION_STUDIO_DOCS_PREVIEW_ORIGIN);
var!(ORION_STUDIO_DOCS_PREVIEW_PROJECT);
var!(ORION_STUDIO_DOCS_ROUTE_PATTERN);
var!(ORION_STUDIO_DOCS_STABLE_ORIGIN);
var!(ORION_STUDIO_DOCS_STABLE_PROJECT);
var!(ORION_STUDIO_AUTOMATION_BOT_LOGIN);
var!(ORION_STUDIO_AUTOMATION_GIT_EMAIL);
var!(ORION_STUDIO_AUTOMATION_GIT_NAME);
var!(ORION_STUDIO_CACHIX_CACHE_NAME);
var!(ORION_STUDIO_DANGER_GITHUB_API_BASE_URL);
var!(ORION_STUDIO_EXTENSION_CLI_BUCKET_NAME);
var!(ORION_STUDIO_EXTENSION_ORGANIZATION);
var!(ORION_STUDIO_EXTENSION_REGISTRY_ENABLED);
var!(ORION_STUDIO_EXTENSION_REGISTRY_REPOSITORY);
var!(ORION_STUDIO_MACOS_SIGNING_IDENTITY);
var!(ORION_STUDIO_MACOS_TEAM_ID);
var!(ORION_STUDIO_NIGHTLY_BUCKET);
var!(ORION_STUDIO_OPEN_SOURCE_WEBSITE_ASSETS_BUCKET_NAME);
var!(ORION_STUDIO_OPEN_SOURCE_WEBSITE_ASSETS_ROUTE_PATTERN);
var!(ORION_STUDIO_SENTRY_ORGANIZATION);
var!(ORION_STUDIO_SENTRY_PROJECT);
var!(ORION_STUDIO_SCCACHE_R2_BUCKET);
var!(ORION_STUDIO_WEBSITE_ORIGIN);
var!(ORION_STUDIO_WINDOWS_APPX_PUBLISHER);
var!(ORION_STUDIO_WINDOWS_PUBLISHER);

pub fn bundle_job_envs() -> Env {
    Env::default().add("CARGO_INCREMENTAL", 0)
}

pub fn bundle_step_envs(platform: Platform, release_build: bool) -> Env {
    let env = Env::default();
    if !release_build {
        return match platform {
            Platform::Windows => env
                .add("ORION_STUDIO_WINDOWS_PUBLISHER", "Orion Studio Development")
                .add(
                    "ORION_STUDIO_WINDOWS_APPX_PUBLISHER",
                    "CN=Orion Studio Development",
                )
                .add("ORION_STUDIO_WINDOWS_ALLOW_UNSIGNED_DEV", "1")
                .add("ORION_STUDIO_REQUIRE_SIGNED_RELEASE", "0"),
            Platform::Linux | Platform::Mac => env,
        };
    }

    let env = env
        .add(
            "ORION_STUDIO_CLIENT_CHECKSUM_SEED",
            ORION_STUDIO_CLIENT_CHECKSUM_SEED,
        )
        .add(
            "ORION_STUDIO_MINIDUMP_ENDPOINT",
            ORION_STUDIO_SENTRY_MINIDUMP_ENDPOINT,
        )
        .add(
            "ORION_STUDIO_SENTRY_ORGANIZATION",
            ORION_STUDIO_SENTRY_ORGANIZATION,
        )
        .add("ORION_STUDIO_SENTRY_PROJECT", ORION_STUDIO_SENTRY_PROJECT)
        .add("SENTRY_AUTH_TOKEN", SENTRY_AUTH_TOKEN);

    match platform {
        Platform::Linux => env,
        Platform::Mac => env
            .add("MACOS_CERTIFICATE", MACOS_CERTIFICATE)
            .add("MACOS_CERTIFICATE_PASSWORD", MACOS_CERTIFICATE_PASSWORD)
            .add("APPLE_NOTARIZATION_KEY", APPLE_NOTARIZATION_KEY)
            .add("APPLE_NOTARIZATION_KEY_ID", APPLE_NOTARIZATION_KEY_ID)
            .add("APPLE_NOTARIZATION_ISSUER_ID", APPLE_NOTARIZATION_ISSUER_ID)
            .add(
                "ORION_STUDIO_MACOS_SIGNING_IDENTITY",
                ORION_STUDIO_MACOS_SIGNING_IDENTITY,
            )
            .add("ORION_STUDIO_MACOS_TEAM_ID", ORION_STUDIO_MACOS_TEAM_ID)
            .add(
                "ORION_STUDIO_MACOS_PROVISIONING_PROFILE_BASE64",
                ORION_STUDIO_MACOS_PROVISIONING_PROFILE_BASE64,
            ),
        Platform::Windows => env
            .add("AZURE_TENANT_ID", AZURE_SIGNING_TENANT_ID)
            .add("AZURE_CLIENT_ID", AZURE_SIGNING_CLIENT_ID)
            .add("AZURE_CLIENT_SECRET", AZURE_SIGNING_CLIENT_SECRET)
            .add("ACCOUNT_NAME", AZURE_SIGNING_ACCOUNT_NAME)
            .add("CERT_PROFILE_NAME", AZURE_SIGNING_CERT_PROFILE_NAME)
            .add("ENDPOINT", AZURE_SIGNING_ENDPOINT)
            .add("FILE_DIGEST", "SHA256")
            .add("TIMESTAMP_DIGEST", "SHA256")
            .add("TIMESTAMP_SERVER", "http://timestamp.acs.microsoft.com")
            .add(
                "ORION_STUDIO_WINDOWS_PUBLISHER",
                ORION_STUDIO_WINDOWS_PUBLISHER,
            )
            .add(
                "ORION_STUDIO_WINDOWS_APPX_PUBLISHER",
                ORION_STUDIO_WINDOWS_APPX_PUBLISHER,
            ),
    }
}

pub fn one_workflow_per_non_main_branch() -> Concurrency {
    one_workflow_per_non_main_branch_and_token("")
}

pub fn one_workflow_per_non_main_branch_and_token<T: AsRef<str>>(token: T) -> Concurrency {
    Concurrency::default()
        .group(format!(
            concat!(
                "${{{{ github.workflow }}}}-${{{{ github.ref_name }}}}-",
                "${{{{ github.ref_name == 'main' && github.sha || 'anysha' }}}}{}"
            ),
            token.as_ref()
        ))
        .cancel_in_progress(true)
}

// Represents a pattern to check for changed files and corresponding output variable
pub struct PathCondition {
    pub name: &'static str,
    pub pattern: &'static str,
    pub invert: bool,
    pub set_by_step: RefCell<Option<String>>,
}
impl PathCondition {
    pub fn new(name: &'static str, pattern: &'static str) -> Self {
        Self {
            name,
            pattern,
            invert: false,
            set_by_step: Default::default(),
        }
    }
    pub fn inverted(name: &'static str, pattern: &'static str) -> Self {
        Self {
            name,
            pattern,
            invert: true,
            set_by_step: Default::default(),
        }
    }

    pub fn and_always<'a>(&'a self) -> PathContextCondition<'a> {
        PathContextCondition {
            condition: self,
            run_in_merge_queue: true,
        }
    }

    pub fn and_not_in_merge_queue<'a>(&'a self) -> PathContextCondition<'a> {
        PathContextCondition {
            condition: self,
            run_in_merge_queue: false,
        }
    }
}

pub struct PathContextCondition<'a> {
    condition: &'a PathCondition,
    run_in_merge_queue: bool,
}

impl<'a> PathContextCondition<'a> {
    pub fn then(&'a self, job: NamedJob) -> NamedJob {
        let set_by_step = self
            .condition
            .set_by_step
            .borrow()
            .clone()
            .unwrap_or_else(|| panic!("condition {},is never set", self.condition.name));
        NamedJob {
            name: job.name,
            job: job.job.add_need(set_by_step.clone()).cond(Expression::new(
                format!(
                    "needs.{}.outputs.{} == 'true' {merge_queue_condition}",
                    set_by_step,
                    self.condition.name,
                    merge_queue_condition = self
                        .run_in_merge_queue
                        .not()
                        .then_some("&& github.event_name != 'merge_group'")
                        .unwrap_or_default()
                )
                .trim(),
            )),
        }
    }
}

pub(crate) struct StepOutput {
    pub name: &'static str,
    step_id: String,
}

impl StepOutput {
    pub fn new<T>(step: &Step<T>, name: &'static str) -> Self {
        let step_id = step
            .value
            .id
            .clone()
            .expect("Steps that produce outputs must have an ID");

        assert!(
            step.value
                .run
                .as_ref()
                .is_none_or(|run_command| run_command.contains(name)),
            "Step output with name '{name}' must occur at least once in run command with ID {step_id}!"
        );

        Self { name, step_id }
    }

    pub fn new_unchecked<T>(step: &Step<T>, name: &'static str) -> Self {
        let step_id = step
            .value
            .id
            .clone()
            .expect("Steps that produce outputs must have an ID");

        Self { name, step_id }
    }

    pub fn expr(&self) -> String {
        format!("steps.{}.outputs.{}", self.step_id, self.name)
    }

    pub fn as_job_output(self, job: &NamedJob) -> JobOutput {
        JobOutput {
            job_name: job.name.clone(),
            name: self.name,
        }
    }
}

impl serde::Serialize for StepOutput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl std::fmt::Display for StepOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "${{{{ {} }}}}", self.expr())
    }
}

pub(crate) struct JobOutput {
    job_name: String,
    name: &'static str,
}

impl JobOutput {
    pub fn expr(&self) -> String {
        format!("needs.{}.outputs.{}", self.job_name, self.name)
    }
}

impl serde::Serialize for JobOutput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl std::fmt::Display for JobOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "${{{{ {} }}}}", self.expr())
    }
}

pub struct WorkflowInput {
    pub input_type: &'static str,
    pub name: &'static str,
    pub default: Option<String>,
    pub description: Option<String>,
}

impl WorkflowInput {
    pub fn string(name: &'static str, default: Option<String>) -> Self {
        Self {
            input_type: "string",
            name,
            default,
            description: None,
        }
    }

    pub fn bool(name: &'static str, default: Option<bool>) -> Self {
        Self {
            input_type: "boolean",
            name,
            default: default.as_ref().map(ToString::to_string),
            description: None,
        }
    }

    pub fn description(mut self, description: impl ToString) -> Self {
        self.description = Some(description.to_string());
        self
    }

    pub fn input(&self) -> WorkflowDispatchInput {
        WorkflowDispatchInput {
            description: self
                .description
                .clone()
                .unwrap_or_else(|| self.name.to_owned()),
            required: self.default.is_none(),
            input_type: self.input_type.to_owned(),
            default: self.default.clone(),
        }
    }

    pub fn call_input(&self) -> WorkflowCallInput {
        WorkflowCallInput {
            description: self.name.to_owned(),
            required: self.default.is_none(),
            input_type: self.input_type.to_owned(),
            default: self.default.clone(),
        }
    }

    pub(crate) fn expr(&self) -> String {
        format!("inputs.{}", self.name)
    }
}

impl std::fmt::Display for WorkflowInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "${{{{ {} }}}}", self.expr())
    }
}

impl serde::Serialize for WorkflowInput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub(crate) struct WorkflowSecret {
    pub name: &'static str,
    description: String,
    required: bool,
}

impl WorkflowSecret {
    pub fn new(name: &'static str, description: impl ToString) -> Self {
        Self {
            name,
            description: description.to_string(),
            required: true,
        }
    }

    pub fn secret_configuration(&self) -> WorkflowCallSecret {
        WorkflowCallSecret {
            description: self.description.clone(),
            required: self.required,
        }
    }
}

impl std::fmt::Display for WorkflowSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "${{{{ secrets.{} }}}}", self.name)
    }
}

impl serde::Serialize for WorkflowSecret {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub mod assets {
    // NOTE: these asset names also exist in the zed.dev codebase.
    pub const MAC_AARCH64: &str = "Orion-Studio-aarch64.dmg";
    pub const MAC_X86_64: &str = "Orion-Studio-x86_64.dmg";
    pub const LINUX_AARCH64: &str = "orion-studio-linux-aarch64.tar.gz";
    pub const LINUX_X86_64: &str = "orion-studio-linux-x86_64.tar.gz";
    pub const BWRAP_LINUX_AARCH64: &str = "bwrap-linux-aarch64.gz";
    pub const BWRAP_LINUX_X86_64: &str = "bwrap-linux-x86_64.gz";
    pub const WINDOWS_X86_64: &str = "Orion-Studio-x86_64.exe";
    pub const WINDOWS_AARCH64: &str = "Orion-Studio-aarch64.exe";

    pub const REMOTE_SERVER_MAC_AARCH64: &str = "orion-studio-remote-server-macos-aarch64.gz";
    pub const REMOTE_SERVER_MAC_X86_64: &str = "orion-studio-remote-server-macos-x86_64.gz";
    pub const REMOTE_SERVER_LINUX_AARCH64: &str = "orion-studio-remote-server-linux-aarch64.gz";
    pub const REMOTE_SERVER_LINUX_X86_64: &str = "orion-studio-remote-server-linux-x86_64.gz";
    pub const REMOTE_SERVER_WINDOWS_AARCH64: &str =
        "orion-studio-remote-server-windows-aarch64.zip";
    pub const REMOTE_SERVER_WINDOWS_X86_64: &str = "orion-studio-remote-server-windows-x86_64.zip";

    pub fn all() -> Vec<&'static str> {
        vec![
            MAC_AARCH64,
            MAC_X86_64,
            LINUX_AARCH64,
            LINUX_X86_64,
            BWRAP_LINUX_AARCH64,
            BWRAP_LINUX_X86_64,
            WINDOWS_X86_64,
            WINDOWS_AARCH64,
            REMOTE_SERVER_MAC_AARCH64,
            REMOTE_SERVER_MAC_X86_64,
            REMOTE_SERVER_LINUX_AARCH64,
            REMOTE_SERVER_LINUX_X86_64,
            REMOTE_SERVER_WINDOWS_AARCH64,
            REMOTE_SERVER_WINDOWS_X86_64,
        ]
    }
}
