use gh_workflow::*;
use indoc::{formatdoc, indoc};

use crate::tasks::workflows::{
    runners,
    steps::{
        self, CommonPermissionSets, DEFAULT_REPOSITORY_GUARD, GitRef, NamedJob, RefSha,
        RepositoryTarget, TokenPermissions, named,
    },
    vars::{self, StepOutput, WorkflowInput},
};

const EXTENSION_CLI_TAG: &str = "extension-cli";

pub fn publish_extension_cli() -> Workflow {
    let message = WorkflowInput::string("message", None).description(
        "Describe why the extension CLI is being bumped and/or what changes are included.",
    );

    let publish = publish_job();
    let update_sha_in_orion_studio = update_sha_in_orion_studio(&publish, &message);
    let update_sha_in_registry = update_sha_in_registry(&publish, &message);

    named::workflow()
        .with_minimal_permissions()
        .on(Event::default().workflow_dispatch(
            WorkflowDispatch::default().add_input(message.name, message.input()),
        ))
        .add_env(("CARGO_TERM_COLOR", "always"))
        .add_env(("CARGO_INCREMENTAL", 0))
        .add_job(publish.name, publish.job)
        .add_job(
            update_sha_in_orion_studio.name,
            update_sha_in_orion_studio.job,
        )
        .add_job(update_sha_in_registry.name, update_sha_in_registry.job)
}

// `workflow_dispatch` can be triggered from any branch where this workflow file
// exists, so we additionally guard the jobs to only run when dispatched from
// `main`. Jobs that depend on `publish_job` inherit this guard transitively
// because they are skipped when `publish_job` is skipped.
fn dispatched_from_main_guard() -> Expression {
    Expression::new(format!(
        "{DEFAULT_REPOSITORY_GUARD} && github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main'"
    ))
}

fn registry_update_guard() -> Expression {
    Expression::new(format!(
        "{DEFAULT_REPOSITORY_GUARD} && github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main' && vars.ORION_STUDIO_EXTENSION_REGISTRY_ENABLED == 'true' && vars.ORION_STUDIO_EXTENSION_ORGANIZATION != '' && startsWith(vars.ORION_STUDIO_EXTENSION_ORGANIZATION, 'orion') && vars.ORION_STUDIO_EXTENSION_REGISTRY_REPOSITORY != ''"
    ))
}

fn publish_job() -> NamedJob {
    fn build_extension_cli() -> Step<Run> {
        named::bash("cargo build --release --package extension_cli")
    }

    fn upload_binary() -> Step<Run> {
        named::bash(r#"script/upload-extension-cli "$GITHUB_SHA""#)
            .add_env((
                "DIGITALOCEAN_SPACES_ACCESS_KEY",
                vars::DIGITALOCEAN_SPACES_ACCESS_KEY,
            ))
            .add_env((
                "DIGITALOCEAN_SPACES_SECRET_KEY",
                vars::DIGITALOCEAN_SPACES_SECRET_KEY,
            ))
            .add_env((
                "ORION_STUDIO_EXTENSION_CLI_BUCKET_NAME",
                vars::ORION_STUDIO_EXTENSION_CLI_BUCKET_NAME,
            ))
    }

    let (authenticate, token) = steps::authenticate_as_orion_automation()
        .for_repository(RepositoryTarget::current())
        .with_permissions([(TokenPermissions::Contents, Level::Write)])
        .into();

    named::job(
        Job::default()
            .cond(dispatched_from_main_guard())
            .runs_on(runners::LINUX_DEFAULT)
            .add_step(steps::checkout_repo().without_persisted_credentials())
            .add_step(steps::cache_rust_dependencies_namespace())
            .add_step(steps::setup_linux())
            .add_step(build_extension_cli())
            .add_step(upload_binary())
            .add_step(authenticate)
            .add_step(steps::update_ref(
                GitRef::tag(EXTENSION_CLI_TAG),
                RefSha::Context,
                &token,
                true,
            )),
    )
}

fn update_sha_in_orion_studio(publish_job: &NamedJob, message: &WorkflowInput) -> NamedJob {
    let (authenticate, generated_token) = steps::authenticate_as_orion_automation()
        .for_repository(RepositoryTarget::current())
        .with_permissions([
            (TokenPermissions::Contents, Level::Write),
            (TokenPermissions::Issues, Level::Write),
            (TokenPermissions::PullRequests, Level::Write),
            (TokenPermissions::Workflows, Level::Write),
        ])
        .into();

    fn replace_sha() -> Step<Run> {
        named::bash(indoc! {r#"
            if ! grep -Eq 'EXTENSION_CLI_SHA: &str = "[a-f0-9]{40}"' tooling/xtask/src/tasks/workflows/extension_tests.rs; then
                echo "::error::Could not find the extension CLI compatibility SHA"
                exit 1
            fi
            sed -i "s/EXTENSION_CLI_SHA: &str = \"[a-f0-9]*\"/EXTENSION_CLI_SHA: \&str = \"$GITHUB_SHA\"/" \
                tooling/xtask/src/tasks/workflows/extension_tests.rs
            grep -Fq "EXTENSION_CLI_SHA: &str = \"$GITHUB_SHA\"" tooling/xtask/src/tasks/workflows/extension_tests.rs
        "#})
    }

    fn regenerate_workflows() -> Step<Run> {
        named::bash("cargo xtask workflows")
    }

    let (get_short_sha_step, short_sha) = get_short_sha();

    named::job(
        Job::default()
            .cond(dispatched_from_main_guard())
            .needs(vec![publish_job.name.clone()])
            .runs_on(runners::LINUX_LARGE)
            .add_step(authenticate)
            .add_step(steps::checkout_repo().without_persisted_credentials())
            .add_step(steps::cache_rust_dependencies_namespace())
            .add_step(get_short_sha_step)
            .add_step(replace_sha())
            .add_step(regenerate_workflows())
            .add_step(create_pull_request_orion_studio(
                &generated_token,
                &short_sha,
                message,
            )),
    )
}

fn create_pull_request_orion_studio(
    generated_token: &StepOutput,
    short_sha: &StepOutput,
    message: &WorkflowInput,
) -> Step<Use> {
    let title = format!(
        "Extension CI: Bump extension CLI version to `{}`",
        short_sha
    );

    let body = formatdoc! {r#"
        This PR bumps the extension CLI version used in the extension workflows to `${{{{ github.sha }}}}`.

        {message}

        Release Notes:

        - N/A
    "#};

    steps::CreatePrStep::new(title, "update-extension-cli-sha", generated_token)
        .with_body(body)
        .into()
}

fn update_sha_in_registry(publish_job: &NamedJob, message: &WorkflowInput) -> NamedJob {
    let registry_repository = RepositoryTarget::new(
        vars::ORION_STUDIO_EXTENSION_ORGANIZATION,
        &[vars::ORION_STUDIO_EXTENSION_REGISTRY_REPOSITORY],
    );
    let (authenticate, generated_token) = steps::authenticate_as_orion_automation()
        .for_repository(registry_repository)
        .with_permissions([
            (TokenPermissions::Contents, Level::Write),
            (TokenPermissions::Issues, Level::Write),
            (TokenPermissions::PullRequests, Level::Write),
            (TokenPermissions::Workflows, Level::Write),
        ])
        .into();

    fn validate_registry_config() -> Step<Run> {
        named::bash(indoc! {r#"
            if [[ "$EXTENSION_REGISTRY_ENABLED" != "true" ]]; then
                echo "::error::ORION_STUDIO_EXTENSION_REGISTRY_ENABLED must be true"
                exit 1
            fi
            if [[ ! "$EXTENSION_ORGANIZATION" =~ ^orion(-[a-z0-9]+)*$ ]]; then
                echo "::error::ORION_STUDIO_EXTENSION_ORGANIZATION must name an Orion-owned GitHub organization"
                exit 1
            fi
            if [[ ! "$EXTENSION_REGISTRY_REPOSITORY" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,99}$ ]]; then
                echo "::error::ORION_STUDIO_EXTENSION_REGISTRY_REPOSITORY must be a repository name without an owner"
                exit 1
            fi
        "#})
        .add_env((
            "EXTENSION_REGISTRY_ENABLED",
            vars::ORION_STUDIO_EXTENSION_REGISTRY_ENABLED,
        ))
        .add_env((
            "EXTENSION_ORGANIZATION",
            vars::ORION_STUDIO_EXTENSION_ORGANIZATION,
        ))
        .add_env((
            "EXTENSION_REGISTRY_REPOSITORY",
            vars::ORION_STUDIO_EXTENSION_REGISTRY_REPOSITORY,
        ))
    }

    fn checkout_registry_repo(token: &StepOutput) -> Step<Use> {
        let repository = format!(
            "{}/{}",
            vars::ORION_STUDIO_EXTENSION_ORGANIZATION,
            vars::ORION_STUDIO_EXTENSION_REGISTRY_REPOSITORY
        );
        named::uses(
            "actions",
            "checkout",
            "11bd71901bbe5b1630ceea73d27597364c9af683", // v4
        )
        .add_with(("repository", repository))
        .add_with(("token", token.to_string()))
        .add_with(("persist-credentials", false))
    }

    fn replace_sha() -> Step<Run> {
        named::bash(indoc! {r#"
            if grep -Eq 'ORION_STUDIO_EXTENSION_CLI_SHA: [a-f0-9]{40}' .github/workflows/ci.yml; then
                sed -i "s/ORION_STUDIO_EXTENSION_CLI_SHA: [a-f0-9]*/ORION_STUDIO_EXTENSION_CLI_SHA: $GITHUB_SHA/" \
                    .github/workflows/ci.yml
            elif grep -Eq 'ZED_EXTENSION_CLI_SHA: [a-f0-9]{40}' .github/workflows/ci.yml; then
                sed -i "s/ZED_EXTENSION_CLI_SHA: [a-f0-9]*/ORION_STUDIO_EXTENSION_CLI_SHA: $GITHUB_SHA/" \
                    .github/workflows/ci.yml
            else
                echo "::error::Could not find the Orion Studio or legacy extension CLI SHA in the registry workflow"
                exit 1
            fi
            grep -Fq "ORION_STUDIO_EXTENSION_CLI_SHA: $GITHUB_SHA" .github/workflows/ci.yml
        "#})
    }

    let (get_short_sha_step, short_sha) = get_short_sha();

    named::job(
        Job::default()
            .cond(registry_update_guard())
            .needs(vec![publish_job.name.clone()])
            .runs_on(runners::LINUX_SMALL)
            .add_step(validate_registry_config())
            .add_step(authenticate)
            .add_step(get_short_sha_step)
            .add_step(checkout_registry_repo(&generated_token))
            .add_step(replace_sha())
            .add_step(create_pull_request_registry(
                &generated_token,
                &short_sha,
                message,
            )),
    )
}

fn create_pull_request_registry(
    generated_token: &StepOutput,
    short_sha: &StepOutput,
    message: &WorkflowInput,
) -> Step<Use> {
    let title = format!("Bump extension CLI version to `{}`", short_sha);

    let body = formatdoc! {r#"
        This PR bumps the extension CLI version to https://github.com/orion-agents/orion-studio/commit/${{{{ github.sha }}}}.

        {message}
    "#};

    steps::CreatePrStep::new(title, "update-extension-cli-sha", generated_token)
        .with_body(body)
        .with_labels("allow-no-extension")
        .into()
}

fn get_short_sha() -> (Step<Run>, StepOutput) {
    let step = named::bash(indoc::indoc! {r#"
        echo "sha_short=$(echo "$GITHUB_SHA" | cut -c1-7)" >> "$GITHUB_OUTPUT"
    "#})
    .id("short-sha");

    let step_output = vars::StepOutput::new(&step, "sha_short");

    (step, step_output)
}
