use gh_workflow::*;

use crate::tasks::workflows::{
    deploy_docs::deploy_docs_workflow_call,
    production_environment,
    release::notify_on_failure,
    runners,
    steps::{CommonPermissionSets, NamedJob, dependant_job, named},
    vars::{self, StepOutput, WorkflowInput},
};

const TAG_NAME_ENV: &str = "${{ github.event.release.tag_name || inputs.tag_name }}";
const IS_PRERELEASE_ENV: &str = "${{ github.event_name == 'release' && format('{0}', github.event.release.prerelease) || format('{0}', inputs.prerelease) }}";
const TAG_NAME: &str = "${{ env.TAG_NAME }}";
const RELEASE_BODY: &str = "${{ github.event.release.body || inputs.body }}";
const DOCS_CHANNEL: &str =
    "${{ (github.event.release.prerelease || inputs.prerelease) && 'preview' || 'stable' }}";
const AFTER_RELEASE_ENABLED_GUARD: &str = "github.repository == 'orion-agents/orion-studio' && vars.ORION_STUDIO_AFTER_RELEASE_ENABLED == 'true'";
const PRODUCTION_SIDE_EFFECT_JOBS: &[&str] = &[
    "rebuild_releases_page",
    "post_to_discord",
    "publish_winget",
    "notify_on_failure",
];

pub(super) fn add_production_environments(workflow: &mut serde_yaml::Value) -> anyhow::Result<()> {
    production_environment::add_to_jobs(workflow, PRODUCTION_SIDE_EFFECT_JOBS)
}

pub fn after_release() -> Workflow {
    let tag_name = WorkflowInput::string("tag_name", None);
    let prerelease = WorkflowInput::bool("prerelease", None);
    let body = WorkflowInput::string("body", Some(String::new()));

    let validate_published_release = validate_published_release();
    let refresh_orion_dev = rebuild_releases_page(&[&validate_published_release]);
    let deploy_docs = deploy_docs_workflow_call(
        DOCS_CHANNEL,
        format!("{AFTER_RELEASE_ENABLED_GUARD} && github.event_name == 'release'"),
    );
    let deploy_docs = NamedJob {
        name: deploy_docs.name,
        job: deploy_docs
            .job
            .needs([validate_published_release.name.clone()]),
    };
    let post_to_discord = post_to_discord(&[&validate_published_release, &refresh_orion_dev]);
    let publish_winget = publish_winget(&[&validate_published_release]);
    let notify_on_failure = {
        let notify_on_failure = notify_on_failure(&[
            &validate_published_release,
            &refresh_orion_dev,
            &post_to_discord,
            &publish_winget,
        ]);
        NamedJob {
            name: notify_on_failure.name,
            job: notify_on_failure
                .job
                .add_need(deploy_docs.name.clone())
                .cond(Expression::new(format!(
                    "{AFTER_RELEASE_ENABLED_GUARD} && failure()"
                ))),
        }
    };

    named::workflow()
        .with_minimal_permissions()
        .add_env(("TAG_NAME", TAG_NAME_ENV))
        .add_env(("IS_PRERELEASE", IS_PRERELEASE_ENV))
        .on(Event::default()
            .release(Release::default().types(vec![ReleaseType::Published]))
            .workflow_dispatch(
                WorkflowDispatch::default()
                    .add_input(tag_name.name, tag_name.input())
                    .add_input(prerelease.name, prerelease.input())
                    .add_input(body.name, body.input()),
            ))
        .add_job(
            validate_published_release.name,
            validate_published_release.job,
        )
        .add_job(refresh_orion_dev.name, refresh_orion_dev.job)
        .add_job(deploy_docs.name, deploy_docs.job)
        .add_job(post_to_discord.name, post_to_discord.job)
        .add_job(publish_winget.name, publish_winget.job)
        .add_job(notify_on_failure.name, notify_on_failure.job)
}

fn validate_published_release() -> NamedJob {
    let validate = named::bash(indoc::indoc! {r#"
        set -euo pipefail

        case "$GITHUB_EVENT_NAME" in
          release|workflow_dispatch) ;;
          *)
            echo "::error::after-release only accepts published-release or manual events"
            exit 1
            ;;
        esac

        if [[ ! "$TAG_NAME" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-pre)?$ ]]; then
            echo "::error::after-release tag must be vMAJOR.MINOR.PATCH or vMAJOR.MINOR.PATCH-pre"
            exit 1
        fi

        release_json=$(gh api --method GET "repos/$GITHUB_REPOSITORY/releases/tags/$TAG_NAME")
        draft=$(jq -r '.draft' <<<"$release_json")
        published_at=$(jq -r '.published_at // empty' <<<"$release_json")
        actual_prerelease=$(jq -r '.prerelease' <<<"$release_json")

        if [[ "$draft" != "false" || -z "$published_at" ]]; then
            echo "::error::$TAG_NAME does not identify a published GitHub release"
            exit 1
        fi
        expected_prerelease=false
        if [[ "$TAG_NAME" == *-pre ]]; then
            expected_prerelease=true
        fi
        if [[ "$actual_prerelease" != "$expected_prerelease" ]]; then
            echo "::error::published release type does not match the release tag"
            exit 1
        fi
        if [[ "$actual_prerelease" != "$IS_PRERELEASE" ]]; then
            echo "::error::requested release channel does not match the published GitHub release"
            exit 1
        fi
    "#})
    .add_env(("GH_TOKEN", "${{ github.token }}"));

    named::job(
        Job::default()
            .cond(Expression::new(AFTER_RELEASE_ENABLED_GUARD))
            .runs_on(runners::LINUX_SMALL)
            .permissions(Permissions::default().contents(Level::Read))
            .timeout_minutes(5u32)
            .add_step(validate),
    )
}

fn rebuild_releases_page(deps: &[&NamedJob]) -> NamedJob {
    fn refresh_cloud_releases() -> Step<Run> {
        named::bash(
            "curl -fX POST \"https://cloud.orion.dev/releases/refresh?expect_tag=$TAG_NAME\"",
        )
    }

    fn revalidate_orion_dev() -> Step<Run> {
        named::bash(
            "curl -fX GET \"https://orion.dev/api/revalidate?tag=releases\" -H \"Authorization: Bearer $ORION_STUDIO_REVALIDATE_TOKEN\"",
        )
        .add_env((
            "ORION_STUDIO_REVALIDATE_TOKEN",
            vars::ORION_STUDIO_REVALIDATE_TOKEN,
        ))
    }

    named::job(
        dependant_job(deps)
            .cond(Expression::new(AFTER_RELEASE_ENABLED_GUARD))
            .runs_on(runners::LINUX_SMALL)
            .add_step(refresh_cloud_releases())
            .add_step(revalidate_orion_dev()),
    )
}

fn post_to_discord(deps: &[&NamedJob]) -> NamedJob {
    fn get_release_url() -> Step<Run> {
        named::bash(
            r#"if [ "$IS_PRERELEASE" == "true" ]; then
    URL="https://orion.dev/releases/preview"
else
    URL="https://orion.dev/releases/stable"
fi

echo "URL=$URL" >> "$GITHUB_OUTPUT"
"#,
        )
        .id("get-release-url")
    }

    fn get_content() -> Step<Use> {
        named::uses(
            "2428392",
            "gh-truncate-string-action",
            "b3ff790d21cf42af3ca7579146eedb93c8fb0757", // v1.4.1
        )
        .id("get-content")
        .add_with((
            "stringToTruncate",
            format!(
                "📣 Orion Studio [{TAG_NAME}](<${{{{ steps.get-release-url.outputs.URL }}}}>) was just released!\n\n{RELEASE_BODY}\n"
            ),
        ))
        .add_with(("maxLength", 2000))
        .add_with(("truncationSymbol", "..."))
    }

    fn discord_webhook_action() -> Step<Use> {
        named::uses(
            "tsickert",
            "discord-webhook",
            "c840d45a03a323fbc3f7507ac7769dbd91bfb164", // v5.3.0
        )
        .add_with(("webhook-url", vars::DISCORD_WEBHOOK_RELEASE_NOTES))
        .add_with(("content", "${{ steps.get-content.outputs.string }}"))
    }
    let job = dependant_job(deps)
        .cond(Expression::new(AFTER_RELEASE_ENABLED_GUARD))
        .runs_on(runners::LINUX_SMALL)
        .add_step(get_release_url())
        .add_step(get_content())
        .add_step(discord_webhook_action());
    named::job(job)
}

fn publish_winget(deps: &[&NamedJob]) -> NamedJob {
    fn sync_winget_pkgs_fork() -> Step<Run> {
        named::pwsh(indoc::indoc! {r#"
            $headers = @{
                "Authorization" = "Bearer $env:WINGET_TOKEN"
                "Accept" = "application/vnd.github+json"
                "X-GitHub-Api-Version" = "2022-11-28"
            }
            $body = @{ branch = "master" } | ConvertTo-Json
            $uri = "https://api.github.com/repos/$env:GITHUB_REPOSITORY_OWNER/winget-pkgs/merge-upstream"
            try {
                Invoke-RestMethod -Uri $uri -Method Post -Headers $headers -Body $body -ContentType "application/json"
                Write-Host "Successfully synced winget-pkgs fork"
            } catch {
                Write-Host "Fork sync response: $_"
                Write-Host "Continuing anyway - fork may already be up to date"
            }
        "#})
        .add_env(("WINGET_TOKEN", vars::WINGET_TOKEN))
    }

    fn set_package_name() -> (Step<Run>, StepOutput) {
        let script = r#"if ($env:IS_PRERELEASE -eq "true") {
    $PACKAGE_NAME = $env:ORION_STUDIO_WINGET_PREVIEW_PACKAGE_ID
} else {
    $PACKAGE_NAME = $env:ORION_STUDIO_WINGET_PACKAGE_ID
}

echo "PACKAGE_NAME=$PACKAGE_NAME" >> $env:GITHUB_OUTPUT
"#;
        let step = named::pwsh(script)
            .id("set-package-name")
            .add_env((
                "ORION_STUDIO_WINGET_PACKAGE_ID",
                "${{ vars.ORION_STUDIO_WINGET_PACKAGE_ID }}",
            ))
            .add_env((
                "ORION_STUDIO_WINGET_PREVIEW_PACKAGE_ID",
                "${{ vars.ORION_STUDIO_WINGET_PREVIEW_PACKAGE_ID }}",
            ));

        let output = StepOutput::new(&step, "PACKAGE_NAME");
        (step, output)
    }

    fn winget_releaser(package_name: &StepOutput) -> Step<Use> {
        named::uses(
            "vedantmgoyal9",
            "winget-releaser",
            "19e706d4c9121098010096f9c495a70a7518b30f", // v2
        )
        .add_with(("identifier", package_name.to_string()))
        .add_with(("release-tag", TAG_NAME))
        .add_with(("max-versions-to-keep", 5))
        .add_with(("token", vars::WINGET_TOKEN))
    }

    let (set_package_name, package_name) = set_package_name();

    named::job(
        dependant_job(deps)
            .cond(Expression::new(
                format!("{AFTER_RELEASE_ENABLED_GUARD} && vars.ORION_STUDIO_WINGET_ENABLED == 'true' && vars.ORION_STUDIO_WINGET_PACKAGE_ID != '' && vars.ORION_STUDIO_WINGET_PREVIEW_PACKAGE_ID != ''"),
            ))
            .runs_on(runners::WINDOWS_DEFAULT)
            .add_step(sync_winget_pkgs_fork())
            .add_step(set_package_name)
            .add_step(winget_releaser(&package_name)),
    )
}

#[cfg(test)]
mod tests {
    use anyhow::{Context as _, Result, ensure};
    use serde_yaml::{Mapping, Value};

    use super::*;

    fn yaml_key(key: &str) -> Value {
        Value::String(key.to_owned())
    }

    fn generated_workflow() -> Result<Value> {
        let content = after_release().to_string().map_err(|error| {
            anyhow::anyhow!("Unable to serialize after-release workflow: {error:?}")
        })?;
        let mut workflow = serde_yaml::from_str(&content)
            .context("Unable to parse generated after-release workflow")?;
        add_production_environments(&mut workflow)?;
        Ok(workflow)
    }

    fn generated_jobs() -> Result<Mapping> {
        generated_workflow()?
            .as_mapping()
            .and_then(|workflow| workflow.get(&yaml_key("jobs")))
            .and_then(Value::as_mapping)
            .cloned()
            .context("Generated after-release workflow has no jobs mapping")
    }

    fn job<'a>(jobs: &'a Mapping, job_id: &str) -> Result<&'a Mapping> {
        jobs.get(&yaml_key(job_id))
            .and_then(Value::as_mapping)
            .with_context(|| format!("Generated after-release workflow has no {job_id:?} job"))
    }

    fn job_condition<'a>(jobs: &'a Mapping, job_id: &str) -> Result<&'a str> {
        job(jobs, job_id)?
            .get(&yaml_key("if"))
            .and_then(Value::as_str)
            .with_context(|| format!("Generated after-release job {job_id:?} has no condition"))
    }

    fn job_needs<'a>(jobs: &'a Mapping, job_id: &str) -> Result<&'a [Value]> {
        job(jobs, job_id)?
            .get(&yaml_key("needs"))
            .and_then(Value::as_sequence)
            .map(Vec::as_slice)
            .with_context(|| format!("Generated after-release job {job_id:?} has no dependencies"))
    }

    fn job_environment<'a>(jobs: &'a Mapping, job_id: &str) -> Result<&'a str> {
        job(jobs, job_id)?
            .get(&yaml_key("environment"))
            .and_then(Value::as_str)
            .with_context(|| {
                format!("Generated after-release job {job_id:?} has no job-level environment")
            })
    }

    #[test]
    fn production_side_effects_require_enablement_and_published_release_validation() -> Result<()> {
        let jobs = generated_jobs()?;

        for job_id in [
            "validate_published_release",
            "rebuild_releases_page",
            "post_to_discord",
        ] {
            ensure!(
                job_condition(&jobs, job_id)? == AFTER_RELEASE_ENABLED_GUARD,
                "{job_id} is not fail-closed on ORION_STUDIO_AFTER_RELEASE_ENABLED"
            );
        }
        ensure!(
            job_condition(&jobs, "deploy_docs")?
                == format!("{AFTER_RELEASE_ENABLED_GUARD} && github.event_name == 'release'"),
            "Docs deployment lost its enablement or published-release event guard"
        );
        ensure!(
            job_condition(&jobs, "publish_winget")?.starts_with(&format!(
                "{AFTER_RELEASE_ENABLED_GUARD} && vars.ORION_STUDIO_WINGET_ENABLED == 'true'"
            )),
            "Winget publication lost its global or resource-specific enablement guard"
        );
        ensure!(
            job_condition(&jobs, "notify_on_failure")?
                == format!("{AFTER_RELEASE_ENABLED_GUARD} && failure()"),
            "After-release failure announcement is not fail-closed"
        );

        let validation_dependency = Value::String("validate_published_release".to_owned());
        for job_id in [
            "rebuild_releases_page",
            "deploy_docs",
            "post_to_discord",
            "publish_winget",
            "notify_on_failure",
        ] {
            ensure!(
                job_needs(&jobs, job_id)?.contains(&validation_dependency),
                "{job_id} can run without published-release validation"
            );
        }

        Ok(())
    }

    #[test]
    fn after_release_side_effect_jobs_use_the_protected_production_environment() -> Result<()> {
        let jobs = generated_jobs()?;

        for job_id in PRODUCTION_SIDE_EFFECT_JOBS {
            ensure!(
                job_environment(&jobs, job_id)? == production_environment::PRODUCTION_ENVIRONMENT,
                "{job_id} is not protected by the production environment"
            );
        }

        Ok(())
    }

    #[test]
    fn manual_dispatch_validation_queries_the_published_release() -> Result<()> {
        let jobs = generated_jobs()?;
        let steps = job(&jobs, "validate_published_release")?
            .get(&yaml_key("steps"))
            .and_then(Value::as_sequence)
            .context("Published-release validation job has no steps")?;
        let script = steps
            .iter()
            .filter_map(Value::as_mapping)
            .find_map(|step| step.get(&yaml_key("run")).and_then(Value::as_str))
            .context("Published-release validation job has no script")?;

        for required_guard in [
            "release|workflow_dispatch",
            "releases/tags/$TAG_NAME",
            "'.draft'",
            "'.published_at // empty'",
            "'.prerelease'",
            "expected_prerelease",
            "actual_prerelease",
        ] {
            ensure!(
                script.contains(required_guard),
                "Published-release validation lost guard {required_guard:?}"
            );
        }

        Ok(())
    }

    #[test]
    fn published_release_event_and_manual_dispatch_remain_available() -> Result<()> {
        let workflow = generated_workflow()?;
        let events = workflow
            .as_mapping()
            .and_then(|workflow| workflow.get(&yaml_key("on")))
            .and_then(Value::as_mapping)
            .context("After-release workflow has no events mapping")?;
        let release_types = events
            .get(&yaml_key("release"))
            .and_then(Value::as_mapping)
            .and_then(|release| release.get(&yaml_key("types")))
            .and_then(Value::as_sequence)
            .context("After-release workflow has no release types")?;

        ensure!(
            release_types.contains(&Value::String("published".to_owned())),
            "After-release workflow no longer accepts published release events"
        );
        ensure!(
            events.contains_key(&yaml_key("workflow_dispatch")),
            "After-release workflow no longer accepts validated manual dispatches"
        );

        Ok(())
    }
}
