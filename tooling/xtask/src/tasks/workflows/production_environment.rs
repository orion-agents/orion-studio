use std::collections::BTreeSet;

use anyhow::{Context as _, Result, bail};
use serde_yaml::{Mapping, Value};

pub(crate) const PRODUCTION_ENVIRONMENT: &str = "production";

pub(crate) fn add_to_jobs(workflow: &mut Value, job_ids: &[&str]) -> Result<()> {
    let workflow = workflow
        .as_mapping_mut()
        .context("The generated workflow must be a YAML mapping")?;
    let jobs = mapping_value_mut(workflow, "jobs")?
        .as_mapping_mut()
        .context("The generated workflow jobs must be a YAML mapping")?;

    let mut unique_job_ids = BTreeSet::new();
    for job_id in job_ids {
        if !unique_job_ids.insert(*job_id) {
            bail!("The production environment job set contains duplicate {job_id:?}");
        }

        let job = mapping_value_mut(jobs, job_id)?
            .as_mapping_mut()
            .with_context(|| format!("The generated {job_id} job must be a YAML mapping"))?;
        if job.contains_key(&yaml_key("uses")) {
            bail!(
                "The generated {job_id} job calls a reusable workflow and cannot declare a GitHub environment"
            );
        }

        let environment_key = yaml_key("environment");
        if job.contains_key(&environment_key) {
            bail!(
                "The generated {job_id} job already contains an environment; refusing to overwrite it"
            );
        }
        job.insert(
            environment_key,
            Value::String(PRODUCTION_ENVIRONMENT.to_owned()),
        );
    }

    Ok(())
}

fn mapping_value_mut<'a>(mapping: &'a mut Mapping, key: &str) -> Result<&'a mut Value> {
    mapping
        .get_mut(&yaml_key(key))
        .with_context(|| format!("The generated workflow is missing {key:?}"))
}

pub(crate) fn yaml_key(key: &str) -> Value {
    Value::String(key.to_owned())
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use super::*;

    #[test]
    fn job_environment_is_distinct_from_action_input() -> Result<()> {
        let mut workflow: Value = serde_yaml::from_str(
            r#"
jobs:
  mutate:
    runs-on: ubuntu-latest
    steps:
      - uses: example/action@0123456789abcdef
        with:
          environment: production
"#,
        )?;

        let jobs = workflow
            .as_mapping()
            .and_then(|workflow| workflow.get(&yaml_key("jobs")))
            .and_then(Value::as_mapping)
            .context("Test workflow has no jobs")?;
        let mutate = jobs
            .get(&yaml_key("mutate"))
            .and_then(Value::as_mapping)
            .context("Test workflow has no mutate job")?;
        ensure!(
            !mutate.contains_key(&yaml_key("environment")),
            "Action input was mistaken for a job environment"
        );

        add_to_jobs(&mut workflow, &["mutate"])?;

        let environment = workflow
            .as_mapping()
            .and_then(|workflow| workflow.get(&yaml_key("jobs")))
            .and_then(Value::as_mapping)
            .and_then(|jobs| jobs.get(&yaml_key("mutate")))
            .and_then(Value::as_mapping)
            .and_then(|job| job.get(&yaml_key("environment")))
            .and_then(Value::as_str);
        ensure!(
            environment == Some(PRODUCTION_ENVIRONMENT),
            "Transform did not add the job-level production environment"
        );

        Ok(())
    }

    #[test]
    fn reusable_workflow_callers_are_rejected() -> Result<()> {
        let mut workflow: Value = serde_yaml::from_str(
            r#"
jobs:
  call:
    uses: ./.github/workflows/deploy.yml
"#,
        )?;

        let error = match add_to_jobs(&mut workflow, &["call"]) {
            Ok(()) => {
                return Err(anyhow::anyhow!(
                    "Reusable workflow caller unexpectedly accepted an environment"
                ));
            }
            Err(error) => error,
        };
        ensure!(
            format!("{error:#}").contains("calls a reusable workflow"),
            "Unexpected reusable-workflow validation error: {error:#}"
        );

        Ok(())
    }
}
