use gh_workflow::{
    Concurrency, Event, Expression, Input, Job, Level, Permissions, Push, Run, Step, Use, UsesJob,
    Workflow, WorkflowCall, WorkflowCallSecret,
};

use crate::tasks::workflows::{
    production_environment, runners,
    steps::{
        self, CommonPermissionSets, FluentBuilder as _, NamedJob, UploadArtifactStep, named,
        release_job,
    },
    vars::{self, StepOutput, WorkflowInput},
};

const BUILD_OUTPUT_DIR: &str = "target/deploy";
const PRODUCTION_SIDE_EFFECT_JOBS: &[&str] = &["deploy_docs"];

pub(super) fn add_production_environments(workflow: &mut serde_yaml::Value) -> anyhow::Result<()> {
    production_environment::add_to_jobs(workflow, PRODUCTION_SIDE_EFFECT_JOBS)
}

pub(crate) enum DocsChannel {
    Nightly,
    Preview,
    Stable,
}

impl DocsChannel {
    pub(crate) fn site_url(&self) -> &'static str {
        match self {
            Self::Nightly => "/docs/nightly/",
            Self::Preview => "/docs/preview/",
            Self::Stable => "/docs/",
        }
    }

    pub(crate) fn channel_name(&self) -> &'static str {
        match self {
            Self::Nightly => "nightly",
            Self::Preview => "preview",
            Self::Stable => "stable",
        }
    }
}

pub(crate) fn lychee_link_check(dir: &str) -> Step<Use> {
    named::uses(
        "lycheeverse",
        "lychee-action",
        "82202e5e9c2f4ef1a55a3d02563e1cb6041e5332",
    ) // v2.4.1
    .add_with(("args", format!("--no-progress --exclude '^http' '{dir}'")))
    .add_with(("fail", true))
    .add_with(("jobSummary", false))
}

pub(crate) fn install_mdbook() -> Step<Use> {
    named::uses(
        "peaceiris",
        "actions-mdbook",
        "ee69d230fe19748b7abf22df32acaa93833fad08", // v2
    )
    .with(("mdbook-version", "0.4.37"))
}

pub(crate) fn build_docs_book(docs_channel: String, site_url: String) -> Step<Run> {
    named::bash(indoc::formatdoc! {r#"
        mkdir -p {BUILD_OUTPUT_DIR}
        mdbook build ./docs --dest-dir=../{BUILD_OUTPUT_DIR}/docs/
    "#})
    .add_env(("DOCS_CHANNEL", docs_channel))
    .add_env(("MDBOOK_BOOK__SITE_URL", site_url))
}

fn docs_build_steps(
    job: Job,
    checkout_ref: Option<String>,
    source_verification: Option<Step<Run>>,
    docs_channel: impl Into<String>,
    site_url: impl Into<String>,
) -> Job {
    let docs_channel = docs_channel.into();
    let site_url = site_url.into();

    let mut job = job
        .add_env(("DOCS_AMPLITUDE_API_KEY", vars::DOCS_AMPLITUDE_API_KEY))
        .add_env(("DOCS_CONSENT_IO_INSTANCE", vars::DOCS_CONSENT_IO_INSTANCE))
        .add_step(
            steps::checkout_repo()
                .with_full_history()
                .without_persisted_credentials()
                .when_some(checkout_ref, |step, checkout_ref| {
                    step.with_ref(checkout_ref)
                }),
        );
    if let Some(source_verification) = source_verification {
        job = job.add_step(source_verification);
    }

    steps::use_clang(
        job.runs_on(runners::LINUX_XL)
            .add_step(steps::setup_cargo_config(runners::Platform::Linux))
            .add_step(steps::cache_rust_dependencies_namespace())
            .map(steps::install_linux_dependencies)
            .add_step(steps::script("./script/generate-action-metadata"))
            .add_step(lychee_link_check("./docs/src/**/*"))
            .add_step(install_mdbook())
            .add_step(build_docs_book(docs_channel, site_url))
            .add_step(lychee_link_check(&format!("{BUILD_OUTPUT_DIR}/docs"))),
    )
}

fn docs_deploy_steps(
    job: Job,
    channel: &StepOutput,
    site_url: &StepOutput,
    project_name: &StepOutput,
    pages_origin: &StepOutput,
) -> Job {
    fn render_worker_configs() -> Step<Run> {
        named::bash(indoc::indoc! {r#"
            set -euo pipefail
            config_directory="target/cloudflare-deploy"
            rm -rf "$config_directory"
            mkdir -p "$config_directory"
            node .cloudflare/render-wrangler-config.mjs docs-proxy "$config_directory/docs-proxy.toml"
            node .cloudflare/render-wrangler-config.mjs open-source-website-assets "$config_directory/open-source-website-assets.toml"
        "#})
        .add_env(("ORION_STUDIO_CLOUDFLARE_ZONE_NAME", "orion.dev"))
        .add_env((
            "ORION_STUDIO_DOCS_ROUTE_PATTERN",
            vars::ORION_STUDIO_DOCS_ROUTE_PATTERN,
        ))
        .add_env((
            "ORION_STUDIO_DOCS_STABLE_ORIGIN",
            vars::ORION_STUDIO_DOCS_STABLE_ORIGIN,
        ))
        .add_env((
            "ORION_STUDIO_DOCS_PREVIEW_ORIGIN",
            vars::ORION_STUDIO_DOCS_PREVIEW_ORIGIN,
        ))
        .add_env((
            "ORION_STUDIO_DOCS_NIGHTLY_ORIGIN",
            vars::ORION_STUDIO_DOCS_NIGHTLY_ORIGIN,
        ))
        .add_env((
            "ORION_STUDIO_WEBSITE_ORIGIN",
            vars::ORION_STUDIO_WEBSITE_ORIGIN,
        ))
        .add_env((
            "ORION_STUDIO_OPEN_SOURCE_WEBSITE_ASSETS_ROUTE_PATTERN",
            vars::ORION_STUDIO_OPEN_SOURCE_WEBSITE_ASSETS_ROUTE_PATTERN,
        ))
        .add_env((
            "ORION_STUDIO_OPEN_SOURCE_WEBSITE_ASSETS_BUCKET_NAME",
            vars::ORION_STUDIO_OPEN_SOURCE_WEBSITE_ASSETS_BUCKET_NAME,
        ))
    }

    fn deploy_to_cf_pages(project_name: &StepOutput) -> Step<Use> {
        named::uses(
            "cloudflare",
            "wrangler-action",
            "da0e0dfe58b7a431659754fdf3f186c529afbe65",
        ) // v3
        .add_with(("apiToken", vars::CLOUDFLARE_API_TOKEN))
        .add_with(("accountId", vars::CLOUDFLARE_ACCOUNT_ID))
        .add_with((
            "command",
            format!(
                "pages deploy {BUILD_OUTPUT_DIR} --project-name=${{{{ {} }}}} --branch main",
                project_name.expr()
            ),
        ))
    }

    fn dry_run_docs_worker() -> Step<Use> {
        named::uses(
            "cloudflare",
            "wrangler-action",
            "da0e0dfe58b7a431659754fdf3f186c529afbe65",
        ) // v3
        .add_with(("apiToken", vars::CLOUDFLARE_API_TOKEN))
        .add_with(("accountId", vars::CLOUDFLARE_ACCOUNT_ID))
        .add_with((
            "command",
            "deploy --dry-run --outdir target/cloudflare-dry-run/docs-proxy --config target/cloudflare-deploy/docs-proxy.toml",
        ))
    }

    fn dry_run_assets_worker() -> Step<Use> {
        named::uses(
            "cloudflare",
            "wrangler-action",
            "da0e0dfe58b7a431659754fdf3f186c529afbe65",
        ) // v3
        .add_with(("apiToken", vars::CLOUDFLARE_API_TOKEN))
        .add_with(("accountId", vars::CLOUDFLARE_ACCOUNT_ID))
        .add_with((
            "command",
            "deploy --dry-run --outdir target/cloudflare-dry-run/open-source-website-assets --config target/cloudflare-deploy/open-source-website-assets.toml",
        ))
    }

    fn upload_versioned_install_script() -> Step<Use> {
        named::uses(
            "cloudflare",
            "wrangler-action",
            "da0e0dfe58b7a431659754fdf3f186c529afbe65",
        ) // v3
        .add_with(("apiToken", vars::CLOUDFLARE_API_TOKEN))
        .add_with(("accountId", vars::CLOUDFLARE_ACCOUNT_ID))
        .add_with((
            "command",
            format!(
                "r2 object put --config target/cloudflare-deploy/open-source-website-assets.toml -f script/install.sh {}/releases/${{{{ github.sha }}}}/install.sh",
                vars::ORION_STUDIO_OPEN_SOURCE_WEBSITE_ASSETS_BUCKET_NAME,
            ),
        ))
    }

    fn promote_install_script(channel: &StepOutput) -> Step<Use> {
        named::uses(
            "cloudflare",
            "wrangler-action",
            "da0e0dfe58b7a431659754fdf3f186c529afbe65",
        ) // v3
        .add_with(("apiToken", vars::CLOUDFLARE_API_TOKEN))
        .add_with(("accountId", vars::CLOUDFLARE_ACCOUNT_ID))
        .add_with((
            "command",
            format!(
                "r2 object put --config target/cloudflare-deploy/open-source-website-assets.toml -f script/install.sh {}/install.sh",
                vars::ORION_STUDIO_OPEN_SOURCE_WEBSITE_ASSETS_BUCKET_NAME,
            ),
        ))
        .if_condition(Expression::new(format!(
            "{} == 'stable'",
            channel.expr()
        )))
    }

    fn deploy_docs_worker() -> Step<Use> {
        named::uses(
            "cloudflare",
            "wrangler-action",
            "da0e0dfe58b7a431659754fdf3f186c529afbe65",
        ) // v3
        .add_with(("apiToken", vars::CLOUDFLARE_API_TOKEN))
        .add_with(("accountId", vars::CLOUDFLARE_ACCOUNT_ID))
        .add_with((
            "command",
            "deploy --config target/cloudflare-deploy/docs-proxy.toml",
        ))
    }

    fn deploy_assets_worker() -> Step<Use> {
        named::uses(
            "cloudflare",
            "wrangler-action",
            "da0e0dfe58b7a431659754fdf3f186c529afbe65",
        ) // v3
        .add_with(("apiToken", vars::CLOUDFLARE_API_TOKEN))
        .add_with(("accountId", vars::CLOUDFLARE_ACCOUNT_ID))
        .add_with((
            "command",
            "deploy --config target/cloudflare-deploy/open-source-website-assets.toml",
        ))
    }

    fn upload_wrangler_logs() -> UploadArtifactStep {
        steps::upload_artifact("wrangler_logs", "/home/runner/.config/.wrangler/logs/")
            .if_condition(Expression::new("always()"))
    }

    fn smoke_pages_origin(pages_origin: &StepOutput) -> Step<Run> {
        named::bash(indoc::indoc! {r#"
            set -euo pipefail
            curl --fail --location --retry 5 --retry-all-errors --max-time 30 "$PAGES_ORIGIN/docs/"
        "#})
        .add_env(("PAGES_ORIGIN", pages_origin.to_string()))
    }

    fn smoke_public_routes(site_url: &StepOutput) -> Step<Run> {
        named::bash(indoc::indoc! {r#"
            set -euo pipefail
            public_docs_url="${ORION_STUDIO_WEBSITE_ORIGIN%/}${SITE_URL}"
            curl --fail --location --retry 5 --retry-all-errors --max-time 30 "$public_docs_url"
        "#})
        .add_env(("SITE_URL", site_url.to_string()))
        .add_env((
            "ORION_STUDIO_WEBSITE_ORIGIN",
            vars::ORION_STUDIO_WEBSITE_ORIGIN,
        ))
    }

    fn smoke_public_install_script(channel: &StepOutput) -> Step<Run> {
        named::bash(indoc::indoc! {r#"
            set -euo pipefail
            curl --fail --location --retry 5 --retry-all-errors --max-time 30 "${ORION_STUDIO_WEBSITE_ORIGIN%/}/install.sh" >/dev/null
        "#})
        .if_condition(Expression::new(format!(
            "{} == 'stable'",
            channel.expr()
        )))
        .add_env((
            "ORION_STUDIO_WEBSITE_ORIGIN",
            vars::ORION_STUDIO_WEBSITE_ORIGIN,
        ))
    }

    job.add_step(render_worker_configs())
        .add_step(dry_run_docs_worker())
        .add_step(dry_run_assets_worker())
        .add_step(upload_versioned_install_script())
        .add_step(deploy_to_cf_pages(project_name))
        .add_step(smoke_pages_origin(pages_origin))
        .add_step(deploy_assets_worker())
        .add_step(deploy_docs_worker())
        .add_step(smoke_public_routes(site_url))
        .add_step(promote_install_script(channel))
        .add_step(smoke_public_install_script(channel))
        .add_step(upload_wrangler_logs())
}

pub(crate) fn check_docs() -> NamedJob {
    NamedJob {
        name: "check_docs".to_owned(),
        job: docs_build_steps(
            release_job(&[]).add_step(steps::harden_runner()),
            None,
            None,
            DocsChannel::Stable.channel_name(),
            DocsChannel::Stable.site_url(),
        ),
    }
}

fn resolve_channel_step(
    channel_expr: impl Into<String>,
) -> (Step<Run>, StepOutput, StepOutput, StepOutput, StepOutput) {
    let step = Step::new("deploy_docs::resolve_channel_step").run(format!(
        indoc::indoc! {r#"
            if [ -z "$CHANNEL" ]; then
                if [ "$GITHUB_REF" = "refs/heads/main" ]; then
                    CHANNEL="nightly"
                else
                    echo "::error::channel input is required when ref is not main."
                    exit 1
                fi
            fi

            case "$CHANNEL" in
                "nightly")
                    SITE_URL="{nightly_site_url}"
                    PROJECT_NAME="$ORION_STUDIO_DOCS_NIGHTLY_PROJECT"
                    PAGES_ORIGIN="$ORION_STUDIO_DOCS_NIGHTLY_ORIGIN"
                    ;;
                "preview")
                    SITE_URL="{preview_site_url}"
                    PROJECT_NAME="$ORION_STUDIO_DOCS_PREVIEW_PROJECT"
                    PAGES_ORIGIN="$ORION_STUDIO_DOCS_PREVIEW_ORIGIN"
                    ;;
                "stable")
                    SITE_URL="{stable_site_url}"
                    PROJECT_NAME="$ORION_STUDIO_DOCS_STABLE_PROJECT"
                    PAGES_ORIGIN="$ORION_STUDIO_DOCS_STABLE_ORIGIN"
                    ;;
                *)
                    echo "::error::Invalid docs channel '$CHANNEL'. Expected one of: nightly, preview, stable."
                    exit 1
                    ;;
            esac

            if [[ ! "$PROJECT_NAME" =~ ^orion-studio-[a-z0-9]+(-[a-z0-9]+)*$ ]]; then
                echo "::error::The selected Cloudflare Pages project must be an Orion Studio resource name."
                exit 1
            fi

            {{
                echo "channel=$CHANNEL"
                echo "site_url=$SITE_URL"
                echo "project_name=$PROJECT_NAME"
                echo "pages_origin=$PAGES_ORIGIN"
            }} >> "$GITHUB_OUTPUT"
        "#},
        nightly_site_url = DocsChannel::Nightly.site_url(),
        preview_site_url = DocsChannel::Preview.site_url(),
        stable_site_url = DocsChannel::Stable.site_url(),
    ))
    .id("resolve-channel")
    .add_env(("CHANNEL", channel_expr.into()))
    .add_env((
        "ORION_STUDIO_DOCS_NIGHTLY_PROJECT",
        vars::ORION_STUDIO_DOCS_NIGHTLY_PROJECT,
    ))
    .add_env((
        "ORION_STUDIO_DOCS_PREVIEW_PROJECT",
        vars::ORION_STUDIO_DOCS_PREVIEW_PROJECT,
    ))
    .add_env((
        "ORION_STUDIO_DOCS_STABLE_PROJECT",
        vars::ORION_STUDIO_DOCS_STABLE_PROJECT,
    ))
    .add_env((
        "ORION_STUDIO_DOCS_NIGHTLY_ORIGIN",
        vars::ORION_STUDIO_DOCS_NIGHTLY_ORIGIN,
    ))
    .add_env((
        "ORION_STUDIO_DOCS_PREVIEW_ORIGIN",
        vars::ORION_STUDIO_DOCS_PREVIEW_ORIGIN,
    ))
    .add_env((
        "ORION_STUDIO_DOCS_STABLE_ORIGIN",
        vars::ORION_STUDIO_DOCS_STABLE_ORIGIN,
    ));

    let channel = StepOutput::new(&step, "channel");
    let site_url = StepOutput::new(&step, "site_url");
    let project_name = StepOutput::new(&step, "project_name");
    let pages_origin = StepOutput::new(&step, "pages_origin");
    (step, channel, site_url, project_name, pages_origin)
}

fn verify_deploy_source(channel: &StepOutput) -> Step<Run> {
    named::bash(indoc::indoc! {r#"
        set -euo pipefail
        actual_sha="$(git rev-parse HEAD)"
        if [[ "$actual_sha" != "$GITHUB_SHA" ]]; then
          echo "::error::Checked out SHA does not match the triggering event SHA."
          exit 1
        fi
        if ! git merge-base --is-ancestor "$actual_sha" origin/main; then
          echo "::error::Docs deployments are limited to commits reachable from main."
          exit 1
        fi

        case "$CHANNEL" in
          nightly)
            if [[ "$GITHUB_EVENT_NAME" != "push" || "$GITHUB_REF" != "refs/heads/main" ]]; then
              echo "::error::Nightly docs may only deploy from a main branch push."
              exit 1
            fi
            ;;
          preview)
            if [[ "$GITHUB_EVENT_NAME" != "release" || "$GITHUB_REF" != refs/tags/v*-pre || "$RELEASE_PRERELEASE" != "true" ]]; then
              echo "::error::Preview docs require a published preview release tag."
              exit 1
            fi
            ;;
          stable)
            if [[ "$GITHUB_EVENT_NAME" != "release" || "$GITHUB_REF" != refs/tags/v* || "$GITHUB_REF" = *-pre || "$RELEASE_PRERELEASE" != "false" ]]; then
              echo "::error::Stable docs require a published stable release tag."
              exit 1
            fi
            ;;
          *)
            echo "::error::Unknown docs channel."
            exit 1
            ;;
        esac
    "#})
    .add_env(("CHANNEL", channel.to_string()))
    .add_env(("RELEASE_PRERELEASE", "${{ github.event.release.prerelease }}"))
}

fn docs_job(channel_expr: impl Into<String>) -> NamedJob {
    let (resolve_step, channel, site_url, project_name, pages_origin) =
        resolve_channel_step(channel_expr);
    let source_verification = verify_deploy_source(&channel);

    NamedJob {
        name: "deploy_docs".to_owned(),
        job: docs_deploy_steps(
            docs_build_steps(
                release_job(&[])
                    .cond(Expression::new(
                        "github.repository == 'orion-agents/orion-studio'",
                    ))
                    .concurrency(
                        Concurrency::new(Expression::new(
                            "orion-studio-docs-${{ inputs.channel }}",
                        ))
                        .cancel_in_progress(false),
                    )
                    .name("Build and Deploy Docs")
                    .add_step(steps::harden_runner())
                    .add_step(resolve_step),
                Some("${{ github.sha }}".to_owned()),
                Some(source_verification),
                channel.to_string(),
                site_url.to_string(),
            ),
            &channel,
            &site_url,
            &project_name,
            &pages_origin,
        ),
    }
}

pub(crate) fn deploy_docs_workflow_call(
    channel: impl Into<String>,
    caller_condition: impl Into<String>,
) -> NamedJob<UsesJob> {
    let job = Job::default()
        .cond(Expression::new(caller_condition.into()))
        .permissions(Permissions::default().contents(Level::Read))
        .uses_local(".github/workflows/deploy_docs.yml")
        .with(Input::default().add("channel", channel.into()))
        .secrets(indexmap::IndexMap::from([
            (
                "DOCS_AMPLITUDE_API_KEY".to_owned(),
                vars::DOCS_AMPLITUDE_API_KEY.to_owned(),
            ),
            (
                "DOCS_CONSENT_IO_INSTANCE".to_owned(),
                vars::DOCS_CONSENT_IO_INSTANCE.to_owned(),
            ),
            (
                "CLOUDFLARE_API_TOKEN".to_owned(),
                vars::CLOUDFLARE_API_TOKEN.to_owned(),
            ),
            (
                "CLOUDFLARE_ACCOUNT_ID".to_owned(),
                vars::CLOUDFLARE_ACCOUNT_ID.to_owned(),
            ),
        ]));

    NamedJob {
        name: "deploy_docs".to_owned(),
        job,
    }
}

pub(crate) fn deploy_docs_job(channel_input: &WorkflowInput) -> NamedJob {
    docs_job(channel_input.to_string())
}

pub(crate) fn deploy_docs() -> Workflow {
    let channel = WorkflowInput::string("channel", None)
        .description("Docs channel to deploy: nightly, preview, or stable");
    let deploy_docs = deploy_docs_job(&channel);

    named::workflow()
        .with_minimal_permissions()
        .add_event(
            Event::default().workflow_call(
                WorkflowCall::default()
                    .add_input(channel.name, channel.call_input())
                    .secrets([
                        (
                            "DOCS_AMPLITUDE_API_KEY".to_owned(),
                            WorkflowCallSecret {
                                description: "DOCS_AMPLITUDE_API_KEY".to_owned(),
                                required: true,
                            },
                        ),
                        (
                            "DOCS_CONSENT_IO_INSTANCE".to_owned(),
                            WorkflowCallSecret {
                                description: "DOCS_CONSENT_IO_INSTANCE".to_owned(),
                                required: true,
                            },
                        ),
                        (
                            "CLOUDFLARE_API_TOKEN".to_owned(),
                            WorkflowCallSecret {
                                description: "CLOUDFLARE_API_TOKEN".to_owned(),
                                required: true,
                            },
                        ),
                        (
                            "CLOUDFLARE_ACCOUNT_ID".to_owned(),
                            WorkflowCallSecret {
                                description: "CLOUDFLARE_ACCOUNT_ID".to_owned(),
                                required: true,
                            },
                        ),
                    ]),
            ),
        )
        .add_job(deploy_docs.name, deploy_docs.job)
}

pub(crate) fn deploy_nightly_docs() -> Workflow {
    let deploy_docs = deploy_docs_workflow_call(
        "nightly",
        "github.repository == 'orion-agents/orion-studio' && github.event_name == 'push' && github.ref == 'refs/heads/main'",
    );

    named::workflow()
        .name("deploy_nightly_docs")
        .permissions(Permissions::default())
        .add_event(Event::default().push(Push::default().add_branch("main")))
        .add_job(deploy_docs.name, deploy_docs.job)
}

#[cfg(test)]
mod tests {
    use anyhow::{Context as _, Result, ensure};
    use serde_yaml::Value;

    use super::*;

    #[test]
    fn docs_deployment_uses_the_protected_production_environment() -> Result<()> {
        let content = deploy_docs()
            .to_string()
            .map_err(|error| anyhow::anyhow!("Unable to serialize docs workflow: {error:?}"))?;
        let mut workflow: Value =
            serde_yaml::from_str(&content).context("Unable to parse generated docs workflow")?;
        add_production_environments(&mut workflow)?;

        let environment = workflow
            .as_mapping()
            .and_then(|workflow| workflow.get(&production_environment::yaml_key("jobs")))
            .and_then(Value::as_mapping)
            .and_then(|jobs| jobs.get(&production_environment::yaml_key("deploy_docs")))
            .and_then(Value::as_mapping)
            .and_then(|job| job.get(&production_environment::yaml_key("environment")))
            .and_then(Value::as_str);
        ensure!(
            environment == Some(production_environment::PRODUCTION_ENVIRONMENT),
            "Docs deployment is not protected by the production environment"
        );

        Ok(())
    }
}
