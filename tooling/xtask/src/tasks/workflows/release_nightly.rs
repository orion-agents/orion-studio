use crate::tasks::workflows::{
    nix_build::build_nix,
    production_environment,
    release::{
        ReleaseBundleJobs, create_sentry_release, download_workflow_artifacts, notify_on_failure,
        prep_release_artifacts,
    },
    run_bundling::{build_static_bwrap, bundle_linux, bundle_mac, bundle_windows},
    run_tests::run_platform_tests_no_filter,
    runners::{Arch, Platform, ReleaseChannel},
    steps::{
        CommonPermissionSets, FluentBuilder, GitRef, NamedJob, RefSha, RepositoryTarget,
        TokenPermissions,
    },
};

use super::{runners, steps, steps::named, vars};
use gh_workflow::*;

const NIGHTLY_RELEASE_ENABLED_GUARD: &str = "github.repository == 'orion-agents/orion-studio' && github.ref == 'refs/heads/main' && vars.ORION_STUDIO_NIGHTLY_RELEASE_ENABLED == 'true'";
const PRODUCTION_SIDE_EFFECT_JOBS: &[&str] = &[
    "bundle_linux_aarch64",
    "bundle_linux_x86_64",
    "build_static_bwrap_linux_aarch64",
    "build_static_bwrap_linux_x86_64",
    "bundle_mac_aarch64",
    "bundle_mac_x86_64",
    "bundle_windows_aarch64",
    "bundle_windows_x86_64",
    "build_nix_linux_x86_64",
    "build_nix_mac_aarch64",
    "update_nightly_tag",
    "notify_on_failure",
];

pub(super) fn add_production_environments(workflow: &mut serde_yaml::Value) -> anyhow::Result<()> {
    production_environment::add_to_jobs(workflow, PRODUCTION_SIDE_EFFECT_JOBS)
}

/// Generates the release_nightly.yml workflow
pub fn release_nightly() -> Workflow {
    let (check_tag, skip) = check_nightly_tag();
    let mut tests = run_platform_tests_no_filter(Platform::Linux);
    tests.job = tests
        .job
        .needs([check_tag.name.clone()])
        .cond(Expression::new(format!(
            "{NIGHTLY_RELEASE_ENABLED_GUARD} && {} != 'true'",
            skip.expr()
        )));

    const NIGHTLY: Option<ReleaseChannel> = Some(ReleaseChannel::Nightly);

    let bundle = ReleaseBundleJobs {
        linux_aarch64: bundle_linux(Arch::AARCH64, NIGHTLY, true, &[&tests]),
        linux_x86_64: bundle_linux(Arch::X86_64, NIGHTLY, true, &[&tests]),
        bwrap_linux_aarch64: build_static_bwrap(Arch::AARCH64, true, &[&tests]),
        bwrap_linux_x86_64: build_static_bwrap(Arch::X86_64, true, &[&tests]),
        mac_aarch64: bundle_mac(Arch::AARCH64, NIGHTLY, true, &[&tests]),
        mac_x86_64: bundle_mac(Arch::X86_64, NIGHTLY, true, &[&tests]),
        windows_aarch64: bundle_windows(Arch::AARCH64, NIGHTLY, true, &[&tests]),
        windows_x86_64: bundle_windows(Arch::X86_64, NIGHTLY, true, &[&tests]),
    };

    let nix_linux_x86 = build_nix(Platform::Linux, Arch::X86_64, "default", None, &[&tests]);
    let nix_mac_arm = build_nix(Platform::Mac, Arch::AARCH64, "default", None, &[&tests]);
    let update_nightly_tag = update_nightly_tag_job(&bundle);
    let notify_on_failure = {
        let notify_on_failure = notify_on_failure(&bundle.jobs());
        NamedJob {
            name: notify_on_failure.name,
            job: notify_on_failure.job.cond(Expression::new(format!(
                "{NIGHTLY_RELEASE_ENABLED_GUARD} && failure()"
            ))),
        }
    };

    named::workflow()
        .with_minimal_permissions()
        .on(Event::default()
            // Fire 6 times a day
            .schedule([Schedule::new("0 */4 * * *")])
            .workflow_dispatch(WorkflowDispatch::default()))
        .concurrency(
            Concurrency::default()
                .group("release-nightly")
                .cancel_in_progress(true),
        )
        .add_env(("CARGO_TERM_COLOR", "always"))
        .add_env(("RUST_BACKTRACE", "1"))
        .add_job(check_tag.name, check_tag.job)
        .add_job(tests.name, tests.job)
        .map(|mut workflow| {
            for job in bundle.into_jobs() {
                workflow = workflow.add_job(job.name, job.job);
            }
            workflow
        })
        .add_job(nix_linux_x86.name, nix_linux_x86.job)
        .add_job(nix_mac_arm.name, nix_mac_arm.job)
        .add_job(update_nightly_tag.name, update_nightly_tag.job)
        .add_job(notify_on_failure.name, notify_on_failure.job)
}

fn check_nightly_tag() -> (NamedJob, vars::JobOutput) {
    let verify_source = named::bash(indoc::indoc! {r#"
        set -euo pipefail

        if [[ "${GITHUB_REF:-}" != "refs/heads/main" ]]; then
            echo "::error::nightly release must run from refs/heads/main"
            exit 1
        fi
        if [[ ! "${GITHUB_SHA:-}" =~ ^[0-9a-f]{40}$ ]]; then
            echo "::error::GITHUB_SHA is not an immutable commit SHA"
            exit 1
        fi

        git fetch --no-tags origin +refs/heads/main:refs/remotes/origin/main
        checked_out_sha=$(git rev-parse 'HEAD^{commit}')
        main_sha=$(git rev-parse 'refs/remotes/origin/main^{commit}')
        if [[ "$checked_out_sha" != "$GITHUB_SHA" || "$main_sha" != "$GITHUB_SHA" ]]; then
            echo "::error::nightly source does not match protected main"
            exit 1
        fi
    "#});

    let step = named::bash(indoc::indoc! {r#"
        NIGHTLY_SHA=$(git rev-parse "nightly" 2>/dev/null || echo "")
        if [ "$NIGHTLY_SHA" = "$GITHUB_SHA" ]; then
            echo "Nightly tag already points to current commit. Skipping."
            echo "skip=true" >> "$GITHUB_OUTPUT"
        else
            echo "skip=false" >> "$GITHUB_OUTPUT"
        fi
    "#})
    .id("check");

    let skip_output = vars::StepOutput::new(&step, "skip");

    let job = release_job(&[])
        .runs_on(runners::LINUX_SMALL)
        .timeout_minutes(5u32)
        .outputs([("skip".to_owned(), skip_output.to_string())])
        .add_step(
            steps::checkout_repo()
                .with_full_history()
                .with_fetch_tags()
                .without_persisted_credentials(),
        )
        .add_step(verify_source)
        .add_step(step);

    let job = named::job(job);
    let skip = skip_output.as_job_output(&job);
    (job, skip)
}

fn release_job(deps: &[&NamedJob]) -> Job {
    let job = Job::default()
        .cond(Expression::new(NIGHTLY_RELEASE_ENABLED_GUARD))
        .timeout_minutes(60u32);
    if deps.len() > 0 {
        job.needs(deps.iter().map(|j| j.name.clone()).collect::<Vec<_>>())
    } else {
        job
    }
}

fn update_nightly_tag_job(bundle: &ReleaseBundleJobs) -> NamedJob {
    let (authenticate, token) = steps::authenticate_as_orion_automation()
        .for_repository(RepositoryTarget::current())
        .with_permissions([(TokenPermissions::Contents, Level::Write)])
        .into();

    NamedJob {
        name: "update_nightly_tag".to_owned(),
        job: steps::release_job(&bundle.jobs())
            .cond(Expression::new(NIGHTLY_RELEASE_ENABLED_GUARD))
            .runs_on(runners::LINUX_MEDIUM)
            .add_step(authenticate)
            .add_step(steps::checkout_repo().with_fetch_tags())
            .add_step(download_workflow_artifacts())
            .add_step(steps::script("ls -lR ./artifacts"))
            .add_step(prep_release_artifacts())
            .add_step(create_sentry_release())
            .add_step(
                steps::script("./script/upload-nightly")
                    .add_env((
                        "ORION_STUDIO_NIGHTLY_BUCKET",
                        vars::ORION_STUDIO_NIGHTLY_BUCKET,
                    ))
                    .add_env((
                        "DIGITALOCEAN_SPACES_ACCESS_KEY",
                        vars::DIGITALOCEAN_SPACES_ACCESS_KEY,
                    ))
                    .add_env((
                        "DIGITALOCEAN_SPACES_SECRET_KEY",
                        vars::DIGITALOCEAN_SPACES_SECRET_KEY,
                    )),
            )
            .add_step(steps::update_ref(
                GitRef::tag("nightly"),
                RefSha::Context,
                &token,
                true,
            )),
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{Context as _, Result, ensure};
    use serde_yaml::{Mapping, Value};

    use super::*;

    fn yaml_key(key: &str) -> Value {
        Value::String(key.to_owned())
    }

    fn generated_jobs() -> Result<Mapping> {
        let content = release_nightly().to_string().map_err(|error| {
            anyhow::anyhow!("Unable to serialize nightly release workflow: {error:?}")
        })?;
        let mut workflow: Value = serde_yaml::from_str(&content)
            .context("Unable to parse generated nightly release workflow")?;
        add_production_environments(&mut workflow)?;
        workflow
            .as_mapping()
            .and_then(|workflow| workflow.get(&yaml_key("jobs")))
            .and_then(Value::as_mapping)
            .cloned()
            .context("Generated nightly release workflow has no jobs mapping")
    }

    fn job_condition<'a>(jobs: &'a Mapping, job_id: &str) -> Result<&'a str> {
        jobs.get(&yaml_key(job_id))
            .and_then(Value::as_mapping)
            .and_then(|job| job.get(&yaml_key("if")))
            .and_then(Value::as_str)
            .with_context(|| format!("Generated nightly release job {job_id:?} has no condition"))
    }

    fn job_environment<'a>(jobs: &'a Mapping, job_id: &str) -> Result<&'a str> {
        jobs.get(&yaml_key(job_id))
            .and_then(Value::as_mapping)
            .and_then(|job| job.get(&yaml_key("environment")))
            .and_then(Value::as_str)
            .with_context(|| {
                format!("Generated nightly release job {job_id:?} has no job-level environment")
            })
    }

    #[test]
    fn nightly_upload_tag_and_notifications_require_explicit_enablement() -> Result<()> {
        let jobs = generated_jobs()?;

        ensure!(
            job_condition(&jobs, "check_nightly_tag")? == NIGHTLY_RELEASE_ENABLED_GUARD,
            "Nightly source validation is not fail-closed"
        );
        ensure!(
            job_condition(&jobs, "run_tests_linux")?
                == format!(
                    "{NIGHTLY_RELEASE_ENABLED_GUARD} && needs.check_nightly_tag.outputs.skip != 'true'"
                ),
            "Nightly tests lost their enablement or duplicate-build guard"
        );
        ensure!(
            job_condition(&jobs, "update_nightly_tag")? == NIGHTLY_RELEASE_ENABLED_GUARD,
            "Nightly upload and tag movement are not fail-closed"
        );
        ensure!(
            job_condition(&jobs, "notify_on_failure")?
                == format!("{NIGHTLY_RELEASE_ENABLED_GUARD} && failure()"),
            "Nightly failure announcement is not fail-closed"
        );

        Ok(())
    }

    #[test]
    fn nightly_side_effect_jobs_use_the_protected_production_environment() -> Result<()> {
        let jobs = generated_jobs()?;

        for job_id in PRODUCTION_SIDE_EFFECT_JOBS {
            ensure!(
                job_environment(&jobs, job_id)? == production_environment::PRODUCTION_ENVIRONMENT,
                "{job_id} is not protected by the production environment"
            );
        }

        Ok(())
    }
}
