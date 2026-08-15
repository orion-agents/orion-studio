use gh_workflow::{
    Event, Expression, Job, Level, Run, Step, Strategy, Use, Workflow, WorkflowDispatch,
};
use indoc::formatdoc;
use indoc::indoc;
use serde_json::json;

use crate::tasks::workflows::steps::GitRef;
use crate::tasks::workflows::steps::RefSha;
use crate::tasks::workflows::steps::{
    CheckoutStep, CommonPermissionSets, DownloadArtifactStep, IfNoFilesFound, ResultEncoding,
    TokenPermissions, UploadArtifactStep, cache_rust_dependencies_namespace,
};
use crate::tasks::workflows::vars::JobOutput;
use crate::tasks::workflows::{
    runners,
    steps::{self, DEFAULT_REPOSITORY_GUARD, NamedJob, RepositoryTarget, named},
    vars::{self, StepOutput, WorkflowInput},
};

const ROLLOUT_TAG_NAME: &str = "extension-workflows";
const WORKFLOW_ARTIFACT_NAME: &str = "extension-workflow-files";
const EXTENSION_AUTOMATION_GUARD: &str = "vars.ORION_STUDIO_EXTENSION_REGISTRY_ENABLED == 'true' && vars.ORION_STUDIO_EXTENSION_ORGANIZATION != '' && startsWith(vars.ORION_STUDIO_EXTENSION_ORGANIZATION, 'orion')";

pub(crate) fn extension_workflow_rollout() -> Workflow {
    let filter_repos_input = WorkflowInput::string("filter-repos", Some(String::new()))
        .description(
            "Comma-separated list of repository names to rollout to. Leave empty for all repos.",
        );
    let extra_context_input = WorkflowInput::string("change-description", Some(String::new()))
        .description("Description for the changes to be expected with this rollout");

    let (fetch_repos, removed_ci, removed_shared) = fetch_extension_repos(&filter_repos_input);
    let rollout_workflows = rollout_workflows_to_extension(
        &fetch_repos,
        removed_ci,
        removed_shared,
        &extra_context_input,
        &filter_repos_input,
    );
    let create_tag = create_rollout_tag(&rollout_workflows, &filter_repos_input);

    named::workflow()
        .with_minimal_permissions()
        .on(Event::default().workflow_dispatch(
            WorkflowDispatch::default()
                .add_input(filter_repos_input.name, filter_repos_input.input())
                .add_input(extra_context_input.name, extra_context_input.input()),
        ))
        .add_env(("CARGO_TERM_COLOR", "always"))
        .add_job(fetch_repos.name, fetch_repos.job)
        .add_job(rollout_workflows.name, rollout_workflows.job)
        .add_job(create_tag.name, create_tag.job)
}

fn fetch_extension_repos(filter_repos_input: &WorkflowInput) -> (NamedJob, JobOutput, JobOutput) {
    fn get_repositories(filter_repos_input: &WorkflowInput) -> (Step<Use>, StepOutput) {
        let step: Step<Use> = steps::github_script(formatdoc! {r#"
                const organization = process.env.EXTENSION_ORGANIZATION.trim();
                if (!/^orion(?:-[a-z0-9]+)*$/.test(organization)) {{
                    throw new Error('ORION_STUDIO_EXTENSION_ORGANIZATION must name an Orion-owned GitHub organization');
                }}

                const repos = await github.paginate(github.rest.repos.listForOrg, {{
                    org: organization,
                    type: 'public',
                    per_page: 100,
                }});

                let filteredRepos = repos
                    .filter(repo => !repo.archived)
                    .map(repo => repo.name);

                const filterInput = process.env.FILTER_REPOS.trim();
                if (filterInput.length > 0) {{
                    const allowedNames = filterInput.split(',').map(s => s.trim()).filter(s => s.length > 0);
                    filteredRepos = filteredRepos.filter(name => allowedNames.includes(name));
                    console.log(`Filter applied. Matched ${{filteredRepos.length}} repos from ${{allowedNames.length}} requested.`);
                }}

                console.log(`Found ${{filteredRepos.length}} extension repos`);
                return filteredRepos;
            "#})
            .result_encoding(ResultEncoding::Json)
            .custom_name("get_repositories")
            .id("list-repos")
            .env(
                "EXTENSION_ORGANIZATION",
                vars::ORION_STUDIO_EXTENSION_ORGANIZATION,
            )
            .env("FILTER_REPOS", filter_repos_input.to_string())
            .into();

        let filtered_repos = StepOutput::new(&step, "result");

        (step, filtered_repos)
    }

    fn checkout_orion_studio_repo() -> CheckoutStep {
        steps::checkout_repo()
            .with_full_history()
            .without_persisted_credentials()
            .with_custom_name("checkout_orion_studio_repo")
    }

    fn get_previous_tag_commit() -> (Step<Run>, StepOutput) {
        let step = named::bash(formatdoc! {r#"
            PREV_COMMIT=$(git rev-parse "{ROLLOUT_TAG_NAME}^{{commit}}" 2>/dev/null || echo "")
            if [ -z "$PREV_COMMIT" ]; then
                echo "::error::No previous rollout tag '{ROLLOUT_TAG_NAME}' found. Cannot determine file changes."
                exit 1
            fi
            echo "Found previous rollout at commit: $PREV_COMMIT"
            echo "prev_commit=$PREV_COMMIT" >> "$GITHUB_OUTPUT"
        "#})
        .id("prev-tag");

        let step_output = StepOutput::new(&step, "prev_commit");

        (step, step_output)
    }

    fn get_removed_files(prev_commit: &StepOutput) -> (Step<Run>, StepOutput, StepOutput) {
        let step = named::bash(indoc! {r#"
            for workflow_type in "ci" "shared"; do
                if [ "$workflow_type" = "ci" ]; then
                    WORKFLOW_DIR="extensions/workflows"
                else
                    WORKFLOW_DIR="extensions/workflows/shared"
                fi

                REMOVED=$(git diff --name-status -M "$PREV_COMMIT" HEAD -- "$WORKFLOW_DIR" | \
                    awk '/^D/ { print $2 } /^R/ { print $2 }' | \
                    xargs -I{} basename {} 2>/dev/null | \
                    tr '\n' ' ' || echo "")
                REMOVED=$(echo "$REMOVED" | xargs)

                echo "Removed files for $workflow_type: $REMOVED"
                echo "removed_${workflow_type}=$REMOVED" >> "$GITHUB_OUTPUT"
            done
        "#})
        .id("calc-changes")
        .add_env(("PREV_COMMIT", prev_commit.to_string()));

        // These are created in the for-loop above and thus do exist
        let removed_ci = StepOutput::new_unchecked(&step, "removed_ci");
        let removed_shared = StepOutput::new_unchecked(&step, "removed_shared");

        (step, removed_ci, removed_shared)
    }

    fn generate_workflow_files() -> Step<Run> {
        named::bash(indoc! {r#"
            cargo xtask workflows "$COMMIT_SHA"
        "#})
        .add_env(("COMMIT_SHA", "${{ github.sha }}"))
    }

    fn upload_workflow_files() -> UploadArtifactStep {
        steps::upload_artifact(WORKFLOW_ARTIFACT_NAME, "extensions/workflows/**/*.yml")
            .if_no_files_found(IfNoFilesFound::Error)
    }

    let (get_org_repositories, list_repos_output) = get_repositories(filter_repos_input);
    let (get_prev_tag, prev_commit) = get_previous_tag_commit();
    let (calc_changes, removed_ci, removed_shared) = get_removed_files(&prev_commit);

    let job = Job::default()
        .cond(Expression::new(format!(
            "{DEFAULT_REPOSITORY_GUARD} && {EXTENSION_AUTOMATION_GUARD} && github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main'")))
        .runs_on(runners::LINUX_SMALL)
        .timeout_minutes(10u32)
        .outputs([
            ("repos".to_owned(), list_repos_output.to_string()),
            ("prev_commit".to_owned(), prev_commit.to_string()),
            ("removed_ci".to_owned(), removed_ci.to_string()),
            ("removed_shared".to_owned(), removed_shared.to_string()),
        ])
        .add_step(checkout_orion_studio_repo())
        .add_step(get_prev_tag)
        .add_step(calc_changes)
        .add_step(get_org_repositories)
        .add_step(cache_rust_dependencies_namespace())
        .add_step(generate_workflow_files())
        .add_step(upload_workflow_files());

    let job = named::job(job);
    let (removed_ci, removed_shared) = (
        removed_ci.as_job_output(&job),
        removed_shared.as_job_output(&job),
    );

    (job, removed_ci, removed_shared)
}

fn rollout_workflows_to_extension(
    fetch_repos_job: &NamedJob,
    removed_ci: JobOutput,
    removed_shared: JobOutput,
    extra_context_input: &WorkflowInput,
    filter_repos_input: &WorkflowInput,
) -> NamedJob {
    fn checkout_extension_repo(token: &StepOutput) -> CheckoutStep {
        let repository = format!(
            "{}/${{{{ matrix.repo }}}}",
            vars::ORION_STUDIO_EXTENSION_ORGANIZATION
        );
        steps::checkout_repo()
            .with_custom_name("checkout_extension_repo")
            .with_token(token)
            .without_persisted_credentials()
            .with_repository(&repository)
            .with_path("extension")
    }

    fn download_workflow_files() -> DownloadArtifactStep {
        steps::download_artifact()
            .artifact_name(WORKFLOW_ARTIFACT_NAME)
            .path("workflow-files")
    }

    fn validate_extension_target() -> Step<Run> {
        named::bash(indoc! {r#"
            if [[ "$EXTENSION_REGISTRY_ENABLED" != "true" ]]; then
                echo "::error::ORION_STUDIO_EXTENSION_REGISTRY_ENABLED must be true"
                exit 1
            fi
            if [[ ! "$EXTENSION_ORGANIZATION" =~ ^orion(-[a-z0-9]+)*$ ]]; then
                echo "::error::ORION_STUDIO_EXTENSION_ORGANIZATION must name an Orion-owned GitHub organization"
                exit 1
            fi
            if [[ ! "$EXTENSION_REPOSITORY" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,99}$ ]]; then
                echo "::error::The extension repository name is invalid"
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
        .add_env(("EXTENSION_REPOSITORY", "${{ matrix.repo }}"))
    }

    fn sync_workflow_files(removed_ci: JobOutput, removed_shared: JobOutput) -> Step<Run> {
        named::bash(indoc! {r#"
            mkdir -p extension/.github/workflows

            if [ "$MATRIX_REPO" = "workflows" ]; then
                REMOVED_FILES="$REMOVED_CI"
            else
                REMOVED_FILES="$REMOVED_SHARED"
            fi

            cd extension/.github/workflows

            if [ -n "$REMOVED_FILES" ]; then
                for file in $REMOVED_FILES; do
                    if [ -f "$file" ]; then
                        rm -f "$file"
                    fi
                done
            fi

            cd - > /dev/null

            if [ "$MATRIX_REPO" = "workflows" ]; then
                cp workflow-files/*.yml extension/.github/workflows/
            else
                cp workflow-files/shared/*.yml extension/.github/workflows/
            fi
        "#})
        .add_env(("REMOVED_CI", removed_ci))
        .add_env(("REMOVED_SHARED", removed_shared))
        .add_env(("MATRIX_REPO", "${{ matrix.repo }}"))
    }

    fn get_short_sha() -> (Step<Run>, StepOutput) {
        let step = named::bash(indoc! {r#"
            echo "sha_short=$(echo "$GITHUB_SHA" | cut -c1-7)" >> "$GITHUB_OUTPUT"
        "#})
        .id("short-sha");

        let step_output = StepOutput::new(&step, "sha_short");

        (step, step_output)
    }

    fn create_pull_request(
        token: &StepOutput,
        short_sha: &StepOutput,
        context_input: &WorkflowInput,
        filter_repos_input: &WorkflowInput,
    ) -> Step<Use> {
        let title = format!("Update CI workflows to `{short_sha}`");

        let body = formatdoc! {r#"
            This PR updates the CI workflow files from the main Orion Studio repository
            based on the commit orion-agents/orion-studio@${{{{ github.sha }}}}

            {context_input}
        "#,
        };

        let pr_step: Step<Use> = steps::CreatePrStep::new(title, "update-workflows", token)
            .with_body(body)
            .with_path("extension")
            // Save my inbox from exploding on rollout
            .with_assignee(format!(
                "${{{{ {repos_expr} != '' && github.actor || '' }}}}",
                repos_expr = filter_repos_input.expr()
            ))
            .into();
        pr_step.id("create-pr")
    }

    fn enable_auto_merge(token: &StepOutput) -> Step<gh_workflow::Run> {
        named::bash(indoc! {r#"
            if [ -n "$PR_NUMBER" ]; then
                gh pr merge "$PR_NUMBER" --auto --squash
            fi
        "#})
        .working_directory("extension")
        .add_env(("GH_TOKEN", token.to_string()))
        .add_env((
            "PR_NUMBER",
            "${{ steps.create-pr.outputs.pull-request-number }}",
        ))
    }

    let (authenticate, token) = steps::authenticate_as_orion_automation()
        .for_repository(RepositoryTarget::new(
            vars::ORION_STUDIO_EXTENSION_ORGANIZATION,
            &["${{ matrix.repo }}"],
        ))
        .with_permissions([
            (TokenPermissions::PullRequests, Level::Write),
            (TokenPermissions::Contents, Level::Write),
            (TokenPermissions::Workflows, Level::Write),
        ])
        .into();

    let (calculate_short_sha, short_sha) = get_short_sha();

    let job = Job::default()
        .needs([fetch_repos_job.name.clone()])
        .cond(Expression::new(format!(
            "{DEFAULT_REPOSITORY_GUARD} && {EXTENSION_AUTOMATION_GUARD} && needs.{}.outputs.repos != '[]'",
            fetch_repos_job.name
        )))
        .runs_on(runners::LINUX_SMALL)
        .timeout_minutes(10u32)
        .strategy(
            Strategy::default()
                .fail_fast(false)
                .max_parallel(10u32)
                .matrix(json!({
                    "repo": format!("${{{{ fromJson(needs.{}.outputs.repos) }}}}", fetch_repos_job.name)
                })),
        )
        .add_step(validate_extension_target())
        .add_step(authenticate)
        .add_step(checkout_extension_repo(&token))
        .add_step(download_workflow_files())
        .add_step(sync_workflow_files(removed_ci, removed_shared))
        .add_step(calculate_short_sha)
        .add_step(create_pull_request(&token, &short_sha, extra_context_input, filter_repos_input))
        .add_step(enable_auto_merge(&token));

    named::job(job)
}

fn create_rollout_tag(rollout_job: &NamedJob, filter_repos_input: &WorkflowInput) -> NamedJob {
    fn checkout_orion_studio_repo(token: &StepOutput) -> CheckoutStep {
        steps::checkout_repo()
            .with_full_history()
            .with_token(token)
            .without_persisted_credentials()
    }

    let (authenticate, token) = steps::authenticate_as_orion_automation()
        .for_repository(RepositoryTarget::current())
        .with_permissions([(TokenPermissions::Contents, Level::Write)])
        .into();

    let job = Job::default()
        .needs([rollout_job.name.clone()])
        .cond(Expression::new(format!(
            "{DEFAULT_REPOSITORY_GUARD} && {EXTENSION_AUTOMATION_GUARD} && github.ref == 'refs/heads/main' && {filter_repos} == ''",
            filter_repos = filter_repos_input.expr(),
        )))
        .runs_on(runners::LINUX_SMALL)
        .timeout_minutes(1u32)
        .add_step(authenticate)
        .add_step(checkout_orion_studio_repo(&token))
        .add_step(steps::update_ref(
            GitRef::Tag(ROLLOUT_TAG_NAME.to_owned()),
            RefSha::Context,
            &token,
            true,
        ));

    named::job(job)
}
