use gh_workflow::*;

use crate::tasks::workflows::steps::{CommonJobConditions, CommonPermissionSets, NamedJob, named};

use super::{runners, steps, vars};

/// Generates the danger.yml workflow
pub fn danger() -> Workflow {
    let danger = danger_job();

    named::workflow()
        .with_minimal_permissions()
        .on(Event::default()
            .pull_request(PullRequest::default().add_branch("main").types([
                PullRequestType::Opened,
                PullRequestType::Synchronize,
                PullRequestType::Reopened,
                PullRequestType::Edited,
            ]))
            .merge_group(MergeGroup::default()))
        .add_job(danger.name, danger.job)
}

fn danger_job() -> NamedJob {
    pub fn install_deps() -> Step<Run> {
        named::bash("pnpm install --dir script/danger")
    }

    pub fn run() -> Step<Run> {
        named::bash(
            r#"
                case "$DANGER_GITHUB_API_BASE_URL" in
                    https://orion.dev/*|https://*.orion.dev/*) ;;
                    *)
                        echo "::error::ORION_STUDIO_DANGER_GITHUB_API_BASE_URL must be an HTTPS endpoint under orion.dev"
                        exit 1
                        ;;
                esac
                pnpm run --dir script/danger danger ci
            "#,
        )
            // This GitHub token is not used, but the value needs to be here to prevent
            // Danger from throwing an error.
            .add_env(("GITHUB_TOKEN", "not_a_real_token"))
            // All requests are instead proxied through a proxy that allows Danger to securely authenticate with GitHub
            // while still being able to run on PRs from forks.
            .add_env((
                "DANGER_GITHUB_API_BASE_URL",
                vars::ORION_STUDIO_DANGER_GITHUB_API_BASE_URL,
            ))
    }

    NamedJob {
        name: "danger".to_string(),
        job: Job::default()
            .with_repository_guard()
            .runs_on(runners::LINUX_SMALL)
            .add_step(steps::checkout_repo())
            .add_step(steps::setup_pnpm())
            .add_step(
                steps::setup_node()
                    .add_with(("cache", "pnpm"))
                    .add_with(("cache-dependency-path", "script/danger/pnpm-lock.yaml")),
            )
            .add_step(install_deps())
            .add_step(run()),
    }
}
