use gh_workflow::{
    Event, Expression, Job, Level, Permissions, PullRequest, Push, UsesJob, Workflow,
};

use crate::tasks::workflows::{
    GenerateWorkflowArgs, GitSha,
    steps::{NamedJob, named},
    vars::one_workflow_per_non_main_branch_and_token,
};

pub(crate) fn run_tests(args: &GenerateWorkflowArgs) -> Workflow {
    let call_extension_tests = call_extension_tests(args.sha.as_ref());
    named::workflow()
        .permissions(Permissions::default().contents(Level::Read))
        .on(Event::default()
            .pull_request(PullRequest::default().add_branch("**"))
            .push(Push::default().add_branch("main")))
        .concurrency(one_workflow_per_non_main_branch_and_token("pr"))
        .add_job(call_extension_tests.name, call_extension_tests.job)
}

pub(crate) fn call_extension_tests(target_ref: Option<&GitSha>) -> NamedJob<UsesJob> {
    let job = Job::default()
        .cond(Expression::new(
            "vars.ORION_STUDIO_EXTENSION_ORGANIZATION != '' && startsWith(vars.ORION_STUDIO_EXTENSION_ORGANIZATION, 'orion') && github.repository_owner == vars.ORION_STUDIO_EXTENSION_ORGANIZATION",
        ))
        .permissions(Permissions::default().contents(Level::Read))
        .uses(
            "orion-agents",
            "orion-studio",
            ".github/workflows/extension_tests.yml",
            target_ref.map_or("main", AsRef::as_ref),
        );

    named::job(job)
}
