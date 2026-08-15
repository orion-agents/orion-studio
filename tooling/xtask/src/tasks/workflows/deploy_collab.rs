use anyhow::{Context as _, Result, bail};
use gh_workflow::{
    Concurrency, Container, Event, Expression, Port, Push, Run, Step, Use, Workflow,
};
use indoc::indoc;
use serde_yaml::{Mapping, Value};

use crate::tasks::workflows::runners::{self, Platform};
use crate::tasks::workflows::steps::{
    self, CommonPermissionSets, FluentBuilder as _, NamedJob, dependant_job, named, use_clang,
};
use crate::tasks::workflows::vars;

const DOCTL_VERSION: &str = "1.109.0";
const REPOSITORY_GUARD: &str = "github.repository == 'orion-agents/orion-studio'";

#[derive(Clone, Copy)]
struct DeploymentTarget {
    job_id: &'static str,
    environment: &'static str,
    git_ref: &'static str,
}

impl DeploymentTarget {
    fn condition(self) -> String {
        format!("{REPOSITORY_GUARD} && github.ref == '{}'", self.git_ref)
    }
}

const PRODUCTION_DEPLOYMENT: DeploymentTarget = DeploymentTarget {
    job_id: "deploy_production",
    environment: "production",
    git_ref: "refs/tags/collab-production",
};
const STAGING_DEPLOYMENT: DeploymentTarget = DeploymentTarget {
    job_id: "deploy_staging",
    environment: "staging",
    git_ref: "refs/tags/collab-staging",
};
const DEPLOYMENT_TARGETS: [DeploymentTarget; 2] = [PRODUCTION_DEPLOYMENT, STAGING_DEPLOYMENT];

pub(crate) fn deploy_collab() -> Workflow {
    let style = style();
    let tests = tests(&[&style]);
    let (publish, image_id) = publish(&[&style, &tests]);
    let production_deploy = deploy(&[&publish], &image_id, PRODUCTION_DEPLOYMENT);
    let staging_deploy = deploy(&[&publish], &image_id, STAGING_DEPLOYMENT);

    named::workflow()
        .with_minimal_permissions()
        .on(Event::default().push(
            Push::default()
                .add_tag("collab-production")
                .add_tag("collab-staging"),
        ))
        .concurrency(
            Concurrency::new(Expression::new(
                "orion-studio-collab-${{ github.ref_name }}",
            ))
            .cancel_in_progress(false),
        )
        .add_env(("DOCKER_BUILDKIT", "1"))
        .add_job(style.name, style.job)
        .add_job(tests.name, tests.job)
        .add_job(publish.name, publish.job)
        .add_job(PRODUCTION_DEPLOYMENT.job_id, production_deploy.job)
        .add_job(STAGING_DEPLOYMENT.job_id, staging_deploy.job)
}

pub(super) fn add_deployment_environments(workflow: &mut Value) -> Result<()> {
    let workflow = workflow
        .as_mapping_mut()
        .context("The generated collab workflow must be a YAML mapping")?;
    let jobs = mapping_value_mut(workflow, "jobs")?
        .as_mapping_mut()
        .context("The generated collab workflow jobs must be a YAML mapping")?;

    let mut actual_deployment_jobs = jobs
        .keys()
        .filter_map(Value::as_str)
        .filter(|job_id| *job_id == "deploy" || job_id.starts_with("deploy_"))
        .collect::<Vec<_>>();
    actual_deployment_jobs.sort_unstable();
    let mut expected_deployment_jobs = DEPLOYMENT_TARGETS
        .iter()
        .map(|target| target.job_id)
        .collect::<Vec<_>>();
    expected_deployment_jobs.sort_unstable();
    if actual_deployment_jobs != expected_deployment_jobs {
        bail!(
            "The generated collab deployment jobs differ from the reviewed set: expected {expected_deployment_jobs:?}, found {actual_deployment_jobs:?}"
        );
    }

    let publish = mapping_value_mut(jobs, "publish")?
        .as_mapping_mut()
        .context("The generated collab publish job must be a YAML mapping")?;
    if publish.contains_key(&yaml_key("environment")) {
        bail!("The collab publish job must remain outside deployment environment approval");
    }

    for target in DEPLOYMENT_TARGETS {
        add_deployment_environment(jobs, target)?;
    }

    Ok(())
}

fn mapping_value_mut<'a>(mapping: &'a mut Mapping, key: &str) -> Result<&'a mut Value> {
    mapping
        .get_mut(&yaml_key(key))
        .with_context(|| format!("The generated collab workflow is missing {key:?}"))
}

fn yaml_key(key: &str) -> Value {
    Value::String(key.to_owned())
}

fn add_deployment_environment(jobs: &mut Mapping, target: DeploymentTarget) -> Result<()> {
    let job = mapping_value_mut(jobs, target.job_id)?
        .as_mapping_mut()
        .with_context(|| format!("The generated {} job must be a YAML mapping", target.job_id))?;

    let expected_condition = target.condition();
    let condition = job
        .get(&yaml_key("if"))
        .and_then(Value::as_str)
        .with_context(|| {
            format!(
                "The generated {} job must have a string condition",
                target.job_id
            )
        })?;
    if condition != expected_condition {
        bail!(
            "The generated {} condition must be {expected_condition:?}, found {condition:?}",
            target.job_id
        );
    }

    let needs = job
        .get(&yaml_key("needs"))
        .and_then(Value::as_sequence)
        .with_context(|| {
            format!(
                "The generated {} job must have a needs sequence",
                target.job_id
            )
        })?;
    if needs.as_slice() != [Value::String("publish".to_owned())] {
        bail!(
            "The generated {} job must depend only on publish, found {needs:?}",
            target.job_id
        );
    }

    let runtime_environment = job
        .get(&yaml_key("env"))
        .and_then(Value::as_mapping)
        .and_then(|environment| {
            environment.get(&yaml_key("ORION_STUDIO_EXPECTED_DEPLOYMENT_ENVIRONMENT"))
        })
        .and_then(Value::as_str)
        .with_context(|| {
            format!(
                "The generated {} job must declare its expected deployment environment",
                target.job_id
            )
        })?;
    if runtime_environment != target.environment {
        bail!(
            "The generated {} runtime environment must be {:?}, found {runtime_environment:?}",
            target.job_id,
            target.environment
        );
    }

    let environment_key = yaml_key("environment");
    if job.contains_key(&environment_key) {
        bail!(
            "The generated {} job already contains an environment; refusing to overwrite it",
            target.job_id
        );
    }
    job.insert(
        environment_key,
        Value::String(target.environment.to_owned()),
    );

    Ok(())
}

fn style() -> NamedJob {
    named::job(use_clang(
        dependant_job(&[])
            .name("Check formatting and Clippy lints")
            .cond(Expression::new(REPOSITORY_GUARD))
            .runs_on(runners::LINUX_XL)
            .add_step(
                steps::checkout_repo()
                    .with_full_history()
                    .without_persisted_credentials(),
            )
            .add_step(steps::setup_cargo_config(Platform::Linux))
            .add_step(steps::cache_rust_dependencies_namespace())
            .map(steps::install_linux_dependencies)
            .add_step(steps::cargo_fmt())
            .add_step(steps::clippy(Platform::Linux, None)),
    ))
}

fn tests(deps: &[&NamedJob]) -> NamedJob {
    fn run_collab_tests() -> Step<Run> {
        named::bash("cargo nextest run --package collab --no-fail-fast")
    }

    named::job(use_clang(
        dependant_job(deps)
            .name("Run tests")
            .cond(Expression::new(REPOSITORY_GUARD))
            .runs_on(runners::LINUX_XL)
            .add_service(
                "postgres",
                Container::new("postgres:15@sha256:1b92e7a80c021647bf70f5d3eb66066a998e4f5cf43c07bb9dc9f729782cf88e")
                    .add_env(("POSTGRES_HOST_AUTH_METHOD", "trust"))
                    .ports(vec![Port::Name("5432:5432".into())])
                    .options(
                        "--health-cmd pg_isready \
                         --health-interval 500ms \
                         --health-timeout 5s \
                         --health-retries 10",
                    ),
            )
            .add_step(
                steps::checkout_repo()
                    .with_full_history()
                    .without_persisted_credentials(),
            )
            .add_step(steps::setup_cargo_config(Platform::Linux))
            .add_step(steps::cache_rust_dependencies_namespace())
            .map(steps::install_linux_dependencies)
            .add_step(steps::cargo_install_nextest())
            .add_step(steps::clear_target_dir_if_large(Platform::Linux))
            .add_step(run_collab_tests()),
    ))
}

fn verify_source() -> Step<Run> {
    named::bash(indoc! {r#"
        set -euo pipefail
        if [[ "$GITHUB_REPOSITORY" != "orion-agents/orion-studio" ]]; then
          echo "Collab deployment is restricted to orion-agents/orion-studio" >&2
          exit 1
        fi
        case "$GITHUB_REF" in
          refs/tags/collab-production|refs/tags/collab-staging) ;;
          *)
            echo "Collab deployment requires an approved deployment tag" >&2
            exit 1
            ;;
        esac
        if [[ ! "$GITHUB_SHA" =~ ^[0-9a-f]{40}$ ]]; then
          echo "The deployment event must expose a full lowercase commit SHA" >&2
          exit 1
        fi
        actual_sha="$(git rev-parse HEAD)"
        tag_sha="$(git rev-parse "${GITHUB_REF}^{commit}")"
        main_sha="$(git rev-parse "origin/main^{commit}")"
        if [[ "$actual_sha" != "$GITHUB_SHA" || "$tag_sha" != "$GITHUB_SHA" ]]; then
          echo "Checked out SHA does not match the collab deployment tag" >&2
          exit 1
        fi
        if [[ "$actual_sha" != "$main_sha" ]]; then
          echo "Collab deployment tags must point at the current protected main commit" >&2
          exit 1
        fi
    "#})
}

fn repository_configuration_envs(step: Step<Run>) -> Step<Run> {
    step.add_env((
        "ORION_STUDIO_PRODUCTION_COLLAB_IMAGE_REPOSITORY",
        "${{ vars.ORION_STUDIO_PRODUCTION_COLLAB_IMAGE_REPOSITORY }}",
    ))
    .add_env((
        "ORION_STUDIO_STAGING_COLLAB_IMAGE_REPOSITORY",
        "${{ vars.ORION_STUDIO_STAGING_COLLAB_IMAGE_REPOSITORY }}",
    ))
}

fn resolve_image_repository() -> Step<Run> {
    repository_configuration_envs(named::bash(indoc! {r#"
        set -euo pipefail
        case "$GITHUB_REF_NAME" in
          collab-production)
            repository="${ORION_STUDIO_PRODUCTION_COLLAB_IMAGE_REPOSITORY:?Configure the production Orion collab repository}"
            ;;
          collab-staging)
            repository="${ORION_STUDIO_STAGING_COLLAB_IMAGE_REPOSITORY:?Configure the staging Orion collab repository}"
            ;;
          *)
            echo "Refusing to publish from an unknown deployment tag" >&2
            exit 1
            ;;
        esac
        if [[ ! "$repository" =~ ^registry\.digitalocean\.com/orion-[a-z0-9]([a-z0-9-]{0,54}[a-z0-9])?/collab$ ]]; then
          echo "The collab repository must be a single lowercase Orion-owned DigitalOcean namespace" >&2
          exit 1
        fi
        if [[ "$repository" == *zed* ]]; then
          echo "Refusing to publish the Orion collab image to a Zed-labelled repository" >&2
          exit 1
        fi
        echo "ORION_STUDIO_COLLAB_IMAGE_REPOSITORY=$repository" >> "$GITHUB_ENV"
    "#}))
}

fn prepare_credentials() -> Step<Run> {
    named::bash(indoc! {r#"
        set -euo pipefail
        credential_root="$(mktemp -d "$RUNNER_TEMP/orion-collab-credentials.XXXXXX")"
        case "$credential_root" in
          "$RUNNER_TEMP"/orion-collab-credentials.*) ;;
          *)
            echo "Credential directory escaped RUNNER_TEMP" >&2
            exit 1
            ;;
        esac
        chmod 700 "$credential_root"
        mkdir -m 700 -p \
          "$credential_root/home/.config/doctl" \
          "$credential_root/docker" \
          "$credential_root/kube"
        {
          echo "ORION_STUDIO_CREDENTIAL_ROOT=$credential_root"
          echo "HOME=$credential_root/home"
          echo "DOCTL_CONFIG=$credential_root/home/.config/doctl/config.yaml"
          echo "DOCKER_CONFIG=$credential_root/docker"
          echo "KUBECONFIG=$credential_root/kube/config"
        } >> "$GITHUB_ENV"
    "#})
}

fn install_doctl() -> Step<Use> {
    named::uses(
        "digitalocean",
        "action-doctl",
        "3cb3953159719656269e044e0e24ca16dd2a690f", // v2.5.2
    )
    .add_with(("version", DOCTL_VERSION))
    .add_with(("token", vars::DIGITALOCEAN_ACCESS_TOKEN))
}

fn verify_doctl_config() -> Step<Run> {
    named::bash(indoc! {r#"
        set -euo pipefail
        if [[ ! -s "$DOCTL_CONFIG" ]]; then
          echo "Pinned doctl did not create its configuration in the private directory" >&2
          exit 1
        fi
        chmod 600 "$DOCTL_CONFIG"
    "#})
}

fn cleanup_publish_credentials() -> Step<Run> {
    named::bash(indoc! {r#"
        set +e
        cleanup_failed=0
        if [[ -n "${DOCKER_CONFIG:-}" ]] && ! docker logout registry.digitalocean.com; then
          echo "Unable to remove the temporary DigitalOcean registry login" >&2
          cleanup_failed=1
        fi
        credential_root="${ORION_STUDIO_CREDENTIAL_ROOT:-}"
        if [[ -n "$credential_root" ]]; then
          case "$credential_root" in
            "$RUNNER_TEMP"/orion-collab-credentials.*)
              if ! rm -rf -- "$credential_root"; then
                echo "Unable to remove the private collab credential directory" >&2
                cleanup_failed=1
              fi
              ;;
            *)
              echo "Refusing to remove an unexpected credential directory" >&2
              cleanup_failed=1
              ;;
          esac
        fi
        exit "$cleanup_failed"
    "#})
    .if_condition(Expression::new("always()"))
}

fn cleanup_deploy_credentials() -> Step<Run> {
    named::bash(indoc! {r#"
        set +e
        cleanup_failed=0
        credential_root="${ORION_STUDIO_CREDENTIAL_ROOT:-}"
        if [[ -n "$credential_root" ]]; then
          case "$credential_root" in
            "$RUNNER_TEMP"/orion-collab-credentials.*)
              if ! rm -rf -- "$credential_root"; then
                echo "Unable to remove the private collab deployment credentials" >&2
                cleanup_failed=1
              fi
              ;;
            *)
              echo "Refusing to remove an unexpected credential directory" >&2
              cleanup_failed=1
              ;;
          esac
        fi
        exit "$cleanup_failed"
    "#})
    .if_condition(Expression::new("always()"))
}

fn publish(deps: &[&NamedJob]) -> (NamedJob, vars::JobOutput) {
    fn build_docker_image() -> Step<Run> {
        named::bash(indoc! {r#"
            set -euo pipefail
            temporary_dockerfile="$(mktemp "$RUNNER_TEMP/Dockerfile-collab.XXXXXX")"
            trap 'rm -f "$temporary_dockerfile"' EXIT
            cp Dockerfile-collab "$temporary_dockerfile"
            docker build \
              --file "$temporary_dockerfile" \
              --build-arg "GITHUB_SHA=$GITHUB_SHA" \
              --label "orion.studio/source-revision=$GITHUB_SHA" \
              --tag "${ORION_STUDIO_COLLAB_IMAGE_REPOSITORY}:$GITHUB_SHA" \
              .
        "#})
    }

    fn sign_into_registry() -> Step<Run> {
        named::bash(indoc! {r#"
            set -euo pipefail
            repository_namespace="${ORION_STUDIO_COLLAB_IMAGE_REPOSITORY#registry.digitalocean.com/}"
            repository_namespace="${repository_namespace%/collab}"
            owned_registry="$(doctl --config "$DOCTL_CONFIG" registry get --format Name --no-header)"
            if [[ "$owned_registry" != "$repository_namespace" ]]; then
              echo "The configured collab repository is not owned by the authenticated DigitalOcean account" >&2
              exit 1
            fi
            doctl --config "$DOCTL_CONFIG" registry login --expiry-seconds 1200
        "#})
    }

    fn publish_docker_image() -> Step<Run> {
        named::bash(indoc! {r#"
            set -euo pipefail
            tagged_image="${ORION_STUDIO_COLLAB_IMAGE_REPOSITORY}:${GITHUB_SHA}"
            docker push "$tagged_image"
            expected_prefix="${ORION_STUDIO_COLLAB_IMAGE_REPOSITORY}@sha256:"
            image_id=""
            while IFS= read -r candidate; do
              if [[ "$candidate" == "$expected_prefix"* ]] && [[ "${candidate#"$expected_prefix"}" =~ ^[0-9a-f]{64}$ ]]; then
                image_id="$candidate"
                break
              fi
            done < <(docker image inspect "$tagged_image" --format '{{range .RepoDigests}}{{println .}}{{end}}')
            if [[ -z "$image_id" ]]; then
              echo "Unable to resolve the published collab image to an immutable digest" >&2
              exit 1
            fi
            echo "image_id=$image_id" >> "$GITHUB_OUTPUT"
        "#})
        .id("publish-image")
    }

    let publish_image = publish_docker_image();
    let image_id = vars::StepOutput::new(&publish_image, "image_id");
    let publish = named::job(
        dependant_job(deps)
            .name("Publish collab server image")
            .cond(Expression::new(REPOSITORY_GUARD))
            .runs_on(runners::LINUX_XL)
            .add_step(
                steps::checkout_repo()
                    .with_full_history()
                    .without_persisted_credentials(),
            )
            .add_step(verify_source())
            .add_step(resolve_image_repository())
            .add_step(prepare_credentials())
            .add_step(build_docker_image())
            .add_step(install_doctl())
            .add_step(verify_doctl_config())
            .add_step(sign_into_registry())
            .add_step(publish_image)
            .add_step(cleanup_publish_credentials())
            .outputs([("image_id".to_owned(), image_id.to_string())]),
    );
    let image_id = image_id.as_job_output(&publish);
    (publish, image_id)
}

fn deployment_configuration_envs(step: Step<Run>) -> Step<Run> {
    repository_configuration_envs(step)
        .add_env((
            "ORION_STUDIO_PRODUCTION_CLUSTER_ID",
            "${{ vars.ORION_STUDIO_PRODUCTION_CLUSTER_ID }}",
        ))
        .add_env((
            "ORION_STUDIO_STAGING_CLUSTER_ID",
            "${{ vars.ORION_STUDIO_STAGING_CLUSTER_ID }}",
        ))
        .add_env((
            "ORION_STUDIO_PRODUCTION_CERTIFICATE_ID",
            "${{ vars.ORION_STUDIO_PRODUCTION_CERTIFICATE_ID }}",
        ))
        .add_env((
            "ORION_STUDIO_STAGING_CERTIFICATE_ID",
            "${{ vars.ORION_STUDIO_STAGING_CERTIFICATE_ID }}",
        ))
        .add_env((
            "ORION_STUDIO_PRODUCTION_TLS_HOSTNAME",
            "${{ vars.ORION_STUDIO_PRODUCTION_TLS_HOSTNAME }}",
        ))
        .add_env((
            "ORION_STUDIO_STAGING_TLS_HOSTNAME",
            "${{ vars.ORION_STUDIO_STAGING_TLS_HOSTNAME }}",
        ))
        .add_env((
            "ORION_STUDIO_PRODUCTION_INGRESS_CIDR",
            "${{ vars.ORION_STUDIO_PRODUCTION_INGRESS_CIDR }}",
        ))
        .add_env((
            "ORION_STUDIO_STAGING_INGRESS_CIDR",
            "${{ vars.ORION_STUDIO_STAGING_INGRESS_CIDR }}",
        ))
        .add_env((
            "ORION_STUDIO_PRODUCTION_DATABASE_EGRESS_CIDR",
            "${{ vars.ORION_STUDIO_PRODUCTION_DATABASE_EGRESS_CIDR }}",
        ))
        .add_env((
            "ORION_STUDIO_STAGING_DATABASE_EGRESS_CIDR",
            "${{ vars.ORION_STUDIO_STAGING_DATABASE_EGRESS_CIDR }}",
        ))
        .add_env((
            "ORION_STUDIO_PRODUCTION_HTTPS_EGRESS_CIDR",
            "${{ vars.ORION_STUDIO_PRODUCTION_HTTPS_EGRESS_CIDR }}",
        ))
        .add_env((
            "ORION_STUDIO_STAGING_HTTPS_EGRESS_CIDR",
            "${{ vars.ORION_STUDIO_STAGING_HTTPS_EGRESS_CIDR }}",
        ))
}

fn deploy(deps: &[&NamedJob], image_id: &vars::JobOutput, target: DeploymentTarget) -> NamedJob {
    fn resolve_deployment_configuration(image_id: &vars::JobOutput) -> Step<Run> {
        deployment_configuration_envs(
            named::bash(indoc! {r#"
                set -euo pipefail
                case "$GITHUB_REF_NAME" in
                  collab-production)
                    namespace=production
                    repository="${ORION_STUDIO_PRODUCTION_COLLAB_IMAGE_REPOSITORY:?Configure the production Orion collab repository}"
                    cluster_id="${ORION_STUDIO_PRODUCTION_CLUSTER_ID:?Configure the production cluster ID}"
                    certificate_id="${ORION_STUDIO_PRODUCTION_CERTIFICATE_ID:?Configure the production certificate ID}"
                    tls_hostname="${ORION_STUDIO_PRODUCTION_TLS_HOSTNAME:?Configure the production TLS hostname}"
                    ingress_cidr="${ORION_STUDIO_PRODUCTION_INGRESS_CIDR:?Configure the production ingress CIDR}"
                    database_cidr="${ORION_STUDIO_PRODUCTION_DATABASE_EGRESS_CIDR:?Configure the production database CIDR}"
                    https_cidr="${ORION_STUDIO_PRODUCTION_HTTPS_EGRESS_CIDR:?Configure the production HTTPS CIDR}"
                    collab_load_balancer_size=10
                    api_load_balancer_size=2
                    ;;
                  collab-staging)
                    namespace=staging
                    repository="${ORION_STUDIO_STAGING_COLLAB_IMAGE_REPOSITORY:?Configure the staging Orion collab repository}"
                    cluster_id="${ORION_STUDIO_STAGING_CLUSTER_ID:?Configure the staging cluster ID}"
                    certificate_id="${ORION_STUDIO_STAGING_CERTIFICATE_ID:?Configure the staging certificate ID}"
                    tls_hostname="${ORION_STUDIO_STAGING_TLS_HOSTNAME:?Configure the staging TLS hostname}"
                    ingress_cidr="${ORION_STUDIO_STAGING_INGRESS_CIDR:?Configure the staging ingress CIDR}"
                    database_cidr="${ORION_STUDIO_STAGING_DATABASE_EGRESS_CIDR:?Configure the staging database CIDR}"
                    https_cidr="${ORION_STUDIO_STAGING_HTTPS_EGRESS_CIDR:?Configure the staging HTTPS CIDR}"
                    collab_load_balancer_size=1
                    api_load_balancer_size=1
                    ;;
                  *)
                    echo "Refusing to deploy from an unknown tag" >&2
                    exit 1
                    ;;
                esac

                if [[ "$namespace" != "$ORION_STUDIO_EXPECTED_DEPLOYMENT_ENVIRONMENT" ]]; then
                  echo "The deployment tag does not match the reviewed GitHub Environment" >&2
                  exit 1
                fi

                if [[ ! "$repository" =~ ^registry\.digitalocean\.com/orion-[a-z0-9]([a-z0-9-]{0,54}[a-z0-9])?/collab$ ]] || [[ "$repository" == *zed* ]]; then
                  echo "The collab repository must be a single lowercase Orion-owned DigitalOcean namespace" >&2
                  exit 1
                fi
                uuid_pattern='^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                if [[ ! "$cluster_id" =~ $uuid_pattern ]]; then
                  echo "The per-environment DigitalOcean cluster ID must be a lowercase UUID" >&2
                  exit 1
                fi
                if [[ ! "$certificate_id" =~ $uuid_pattern ]]; then
                  echo "The per-environment DigitalOcean certificate ID must be a lowercase UUID" >&2
                  exit 1
                fi
                if [[ ! "$tls_hostname" =~ ^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)+$ ]]; then
                  echo "The expected TLS hostname must be a lowercase DNS name" >&2
                  exit 1
                fi
                image_id="${ORION_STUDIO_IMAGE_ID:?The publish job must provide an immutable collab image digest}"
                expected_prefix="${repository}@sha256:"
                if [[ "$image_id" != "$expected_prefix"* ]] || [[ ! "${image_id#"$expected_prefix"}" =~ ^[0-9a-f]{64}$ ]]; then
                  echo "The collab image must be an immutable digest from the selected environment repository" >&2
                  exit 1
                fi

                python3 - "$ingress_cidr" "$database_cidr" "$https_cidr" <<'PY'
                import ipaddress
                import sys

                labels = ("ingress", "database egress", "HTTPS egress")
                for label, value in zip(labels, sys.argv[1:]):
                    try:
                        network = ipaddress.ip_network(value, strict=True)
                    except ValueError as error:
                        raise SystemExit(f"Invalid {label} CIDR: {error}")
                    if network.version != 4:
                        raise SystemExit(f"{label} CIDR must be IPv4")
                    if network.is_loopback or network.is_link_local or network.is_multicast:
                        raise SystemExit(f"{label} CIDR is not deployable")
                    if str(network) != value:
                        raise SystemExit(f"{label} CIDR must use canonical notation")
                PY

                {
                  echo "ORION_STUDIO_KUBE_NAMESPACE=$namespace"
                  echo "ORION_STUDIO_COLLAB_IMAGE_REPOSITORY=$repository"
                  echo "ORION_STUDIO_CLUSTER_ID=$cluster_id"
                  echo "ORION_STUDIO_DO_CERTIFICATE_ID=$certificate_id"
                  echo "ORION_STUDIO_TLS_HOSTNAME=$tls_hostname"
                  echo "ORION_STUDIO_INGRESS_CIDR=$ingress_cidr"
                  echo "ORION_STUDIO_DATABASE_EGRESS_CIDR=$database_cidr"
                  echo "ORION_STUDIO_HTTPS_EGRESS_CIDR=$https_cidr"
                  echo "ORION_STUDIO_COLLAB_LOAD_BALANCER_SIZE_UNIT=$collab_load_balancer_size"
                  echo "ORION_STUDIO_API_LOAD_BALANCER_SIZE_UNIT=$api_load_balancer_size"
                } >> "$GITHUB_ENV"
            "#})
            .add_env(("ORION_STUDIO_IMAGE_ID", image_id.to_string())),
        )
    }

    fn sign_into_kubernetes() -> Step<Run> {
        named::bash(indoc! {r#"
            set -euo pipefail
            owned_cluster_id="$(doctl --config "$DOCTL_CONFIG" kubernetes cluster get "$ORION_STUDIO_CLUSTER_ID" --format ID --no-header)"
            if [[ "$owned_cluster_id" != "$ORION_STUDIO_CLUSTER_ID" ]]; then
              echo "The selected cluster is not owned by the authenticated DigitalOcean account" >&2
              exit 1
            fi

            certificate_json="$(doctl --config "$DOCTL_CONFIG" compute certificate get "$ORION_STUDIO_DO_CERTIFICATE_ID" --output json)"
            certificate_json_path="$ORION_STUDIO_CREDENTIAL_ROOT/certificate.json"
            printf '%s' "$certificate_json" > "$certificate_json_path"
            chmod 600 "$certificate_json_path"
            python3 - "$ORION_STUDIO_DO_CERTIFICATE_ID" "$ORION_STUDIO_TLS_HOSTNAME" "$certificate_json_path" <<'PY'
            import datetime
            import json
            import sys

            expected_id, expected_hostname, certificate_json_path = sys.argv[1:]
            with open(certificate_json_path, encoding="utf-8") as certificate_file:
                payload = json.load(certificate_file)
            if isinstance(payload, list):
                if len(payload) != 1:
                    raise SystemExit("DigitalOcean returned an ambiguous certificate response")
                payload = payload[0]
            if not isinstance(payload, dict) or payload.get("id") != expected_id:
                raise SystemExit("The certificate is not owned by the authenticated DigitalOcean account")
            if payload.get("state") not in {"verified", "active"}:
                raise SystemExit("The DigitalOcean certificate is not active")
            dns_names = payload.get("dns_names") or []
            def covers(name):
                return name == expected_hostname or (
                    name.startswith("*.")
                    and expected_hostname.endswith(name[1:])
                    and expected_hostname.count(".") == name.count(".")
                )
            if not any(covers(name) for name in dns_names):
                raise SystemExit("The DigitalOcean certificate does not cover the expected hostname")
            not_after = payload.get("not_after")
            if not not_after:
                raise SystemExit("The DigitalOcean certificate has no expiration timestamp")
            expires_at = datetime.datetime.fromisoformat(not_after.replace("Z", "+00:00"))
            if expires_at <= datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(days=14):
                raise SystemExit("The DigitalOcean certificate expires within 14 days")
            PY

            doctl --config "$DOCTL_CONFIG" kubernetes cluster kubeconfig save \
              --expiry-seconds 600 \
              "$ORION_STUDIO_CLUSTER_ID"
            if [[ ! -s "$KUBECONFIG" ]]; then
              echo "doctl did not create the private kubeconfig" >&2
              exit 1
            fi
            chmod 600 "$KUBECONFIG"
            rm -f -- "$DOCTL_CONFIG"
        "#})
    }

    fn start_rollout(image_id: &vars::JobOutput) -> Step<Run> {
        named::bash(indoc! {r#"
            set -euo pipefail
            echo "Deploying $GITHUB_SHA to $ORION_STUDIO_KUBE_NAMESPACE"

            environment_file="crates/collab/k8s/environments/${ORION_STUDIO_KUBE_NAMESPACE}.sh"
            if [[ ! -f "$environment_file" ]]; then
              echo "Missing collab environment configuration" >&2
              exit 1
            fi
            while IFS= read -r line || [[ -n "$line" ]]; do
              case "$line" in
                ''|'#'*) continue ;;
              esac
              if [[ ! "$line" =~ ^(ORION_STUDIO_ENVIRONMENT|RUST_LOG)=[A-Za-z0-9._:/-]+$ ]]; then
                echo "Invalid or unexpected assignment in $environment_file" >&2
                exit 1
              fi
              export "$line"
            done < "$environment_file"
            if [[ "$ORION_STUDIO_ENVIRONMENT" != "$ORION_STUDIO_KUBE_NAMESPACE" ]]; then
              echo "The runtime environment must match its Kubernetes namespace" >&2
              exit 1
            fi

            template=crates/collab/k8s/collab.template.yml
            expected_variables=(
              DATABASE_MAX_CONNECTIONS
              GITHUB_SHA
              ORION_STUDIO_DATABASE_EGRESS_CIDR
              ORION_STUDIO_DO_CERTIFICATE_ID
              ORION_STUDIO_ENVIRONMENT
              ORION_STUDIO_HTTPS_EGRESS_CIDR
              ORION_STUDIO_IMAGE_ID
              ORION_STUDIO_INGRESS_CIDR
              ORION_STUDIO_KUBE_NAMESPACE
              ORION_STUDIO_LOAD_BALANCER_SIZE_UNIT
              ORION_STUDIO_SERVICE_NAME
              RUST_LOG
            )
            actual_variables=()
            while IFS= read -r variable_name; do
              actual_variables[${#actual_variables[@]}]="$variable_name"
            done < <(
              grep -oE '\$\{[A-Z][A-Z0-9_]*\}' "$template" \
                | tr -d '${}' \
                | sort -u
            )
            if [[ "${actual_variables[*]}" != "${expected_variables[*]}" ]]; then
              echo "The collab template variable set differs from the reviewed allowlist" >&2
              printf 'expected: %s\nactual: %s\n' "${expected_variables[*]}" "${actual_variables[*]}" >&2
              exit 1
            fi

            manifest_root="$(mktemp -d "$RUNNER_TEMP/orion-collab-manifests.XXXXXX")"
            trap 'rm -rf -- "$manifest_root"' EXIT
            substitution_variables='${DATABASE_MAX_CONNECTIONS} ${GITHUB_SHA} ${ORION_STUDIO_DATABASE_EGRESS_CIDR} ${ORION_STUDIO_DO_CERTIFICATE_ID} ${ORION_STUDIO_ENVIRONMENT} ${ORION_STUDIO_HTTPS_EGRESS_CIDR} ${ORION_STUDIO_IMAGE_ID} ${ORION_STUDIO_INGRESS_CIDR} ${ORION_STUDIO_KUBE_NAMESPACE} ${ORION_STUDIO_LOAD_BALANCER_SIZE_UNIT} ${ORION_STUDIO_SERVICE_NAME} ${RUST_LOG}'

            render_manifest() {
              local service_name=$1
              local max_connections=$2
              local load_balancer_size=$3
              local output_path=$4
              export ORION_STUDIO_SERVICE_NAME="$service_name"
              export DATABASE_MAX_CONNECTIONS="$max_connections"
              export ORION_STUDIO_LOAD_BALANCER_SIZE_UNIT="$load_balancer_size"
              for variable_name in "${expected_variables[@]}"; do
                if [[ -z "${!variable_name:-}" ]]; then
                  echo "Required template variable $variable_name is empty for $service_name" >&2
                  return 1
                fi
              done
              envsubst "$substitution_variables" < "$template" > "$output_path"
              if grep -F '${' "$output_path"; then
                echo "Rendered $service_name manifest contains an unresolved variable" >&2
                return 1
              fi
            }

            collab_manifest="$manifest_root/collab.yml"
            api_manifest="$manifest_root/api.yml"
            render_manifest collab 850 "$ORION_STUDIO_COLLAB_LOAD_BALANCER_SIZE_UNIT" "$collab_manifest"
            render_manifest api 60 "$ORION_STUDIO_API_LOAD_BALANCER_SIZE_UNIT" "$api_manifest"

            kubectl apply --server-side --dry-run=server --field-manager=orion-studio-deploy \
              -f "$collab_manifest" \
              -f "$api_manifest" \
              >/dev/null

            deployment_state() {
              local service_name=$1
              local revision
              local source_revision
              local image
              local expected_image_prefix
              if ! revision="$(kubectl -n "$ORION_STUDIO_KUBE_NAMESPACE" get "deployment/$service_name" -o 'jsonpath={.metadata.annotations.deployment\.kubernetes\.io/revision}')"; then
                echo "Unable to read the current $service_name deployment; bootstrap must be performed separately" >&2
                return 1
              fi
              if [[ ! "$revision" =~ ^[1-9][0-9]*$ ]]; then
                echo "The current $service_name deployment has no rollback revision" >&2
                return 1
              fi
              if ! source_revision="$(kubectl -n "$ORION_STUDIO_KUBE_NAMESPACE" get "deployment/$service_name" -o 'jsonpath={.metadata.annotations.orion\.studio/source-revision}')"; then
                echo "Unable to read the current $service_name source revision" >&2
                return 1
              fi
              if [[ ! "$source_revision" =~ ^[0-9a-f]{40}$ ]]; then
                echo "The current $service_name deployment has no valid source revision" >&2
                return 1
              fi
              if ! image="$(kubectl -n "$ORION_STUDIO_KUBE_NAMESPACE" get "deployment/$service_name" -o 'jsonpath={.spec.template.spec.containers[0].image}')"; then
                echo "Unable to read the current $service_name image" >&2
                return 1
              fi
              expected_image_prefix="${ORION_STUDIO_COLLAB_IMAGE_REPOSITORY}@sha256:"
              if [[ "$image" != "$expected_image_prefix"* ]] || [[ ! "${image#"$expected_image_prefix"}" =~ ^[0-9a-f]{64}$ ]]; then
                echo "The current $service_name deployment is not pinned to the reviewed repository" >&2
                return 1
              fi
              printf '%s|%s|%s' "$revision" "$source_revision" "$image"
            }

            IFS='|' read -r collab_revision collab_source_revision collab_image <<< "$(deployment_state collab)"
            IFS='|' read -r api_revision api_source_revision api_image <<< "$(deployment_state api)"
            if [[ "$collab_source_revision" != "$api_source_revision" || "$collab_image" != "$api_image" ]]; then
              echo "collab and api must start from the same source revision and image digest" >&2
              exit 1
            fi

            rollback_all() {
              local rollback_failed=0
              local service_name
              local revision
              local source_revision
              local image
              local current_revision
              local restored_image
              local restored_source_revision
              local rollout_started
              while IFS='|' read -r service_name revision source_revision image; do
                rollout_started=0
                if ! current_revision="$(kubectl -n "$ORION_STUDIO_KUBE_NAMESPACE" get "deployment/$service_name" -o 'jsonpath={.metadata.annotations.deployment\.kubernetes\.io/revision}')"; then
                  echo "Unable to read the current revision while rolling back $service_name" >&2
                  rollback_failed=1
                  current_revision=unknown
                fi
                if [[ "$current_revision" != "$revision" ]]; then
                  if ! kubectl -n "$ORION_STUDIO_KUBE_NAMESPACE" rollout undo "deployment/$service_name" --to-revision="$revision"; then
                    echo "Unable to start rollback for $service_name to revision $revision" >&2
                    rollback_failed=1
                  else
                    rollout_started=1
                  fi
                fi
                if ! kubectl -n "$ORION_STUDIO_KUBE_NAMESPACE" annotate "deployment/$service_name" "orion.studio/source-revision=$source_revision" --overwrite; then
                  echo "Unable to restore the source annotation for $service_name" >&2
                  rollback_failed=1
                fi
                if (( rollout_started != 0 )) && ! kubectl -n "$ORION_STUDIO_KUBE_NAMESPACE" rollout status "deployment/$service_name" --watch --timeout=10m; then
                  echo "Rollback for $service_name did not become healthy" >&2
                  rollback_failed=1
                fi
                if ! restored_image="$(kubectl -n "$ORION_STUDIO_KUBE_NAMESPACE" get "deployment/$service_name" -o 'jsonpath={.spec.template.spec.containers[0].image}')"; then
                  echo "Unable to verify the restored image for $service_name" >&2
                  rollback_failed=1
                  restored_image=unknown
                fi
                if ! restored_source_revision="$(kubectl -n "$ORION_STUDIO_KUBE_NAMESPACE" get "deployment/$service_name" -o 'jsonpath={.metadata.annotations.orion\.studio/source-revision}')"; then
                  echo "Unable to verify the restored source annotation for $service_name" >&2
                  rollback_failed=1
                  restored_source_revision=unknown
                fi
                if [[ "$restored_image" != "$image" || "$restored_source_revision" != "$source_revision" ]]; then
                  echo "Rollback identity verification failed for $service_name" >&2
                  rollback_failed=1
                fi
              done <<ROLLBACK_REVISIONS
            collab|$collab_revision|$collab_source_revision|$collab_image
            api|$api_revision|$api_source_revision|$api_image
            ROLLBACK_REVISIONS
              return "$rollback_failed"
            }

            if ! kubectl apply --server-side --field-manager=orion-studio-deploy \
              -f "$collab_manifest" \
              -f "$api_manifest"; then
              echo "Applying the collab release failed; rolling back both deployments" >&2
              if ! rollback_all; then
                echo "One or more deployment rollback operations failed" >&2
              fi
              exit 1
            fi

            rollout_failed=0
            for service_name in collab api; do
              if ! kubectl -n "$ORION_STUDIO_KUBE_NAMESPACE" rollout status "deployment/$service_name" --watch --timeout=10m; then
                echo "Rollout failed for $service_name" >&2
                rollout_failed=1
              fi
            done
            if (( rollout_failed != 0 )); then
              if ! rollback_all; then
                echo "One or more deployment rollback operations failed" >&2
              fi
              exit 1
            fi

            identity_failed=0
            for service_name in collab api; do
              if ! deployed_image="$(kubectl -n "$ORION_STUDIO_KUBE_NAMESPACE" get "deployment/$service_name" -o 'jsonpath={.spec.template.spec.containers[0].image}')"; then
                echo "Unable to read the deployed image for $service_name" >&2
                identity_failed=1
                deployed_image=unknown
              fi
              if ! deployed_revision="$(kubectl -n "$ORION_STUDIO_KUBE_NAMESPACE" get "deployment/$service_name" -o 'jsonpath={.metadata.annotations.orion\.studio/source-revision}')"; then
                echo "Unable to read the deployed source revision for $service_name" >&2
                identity_failed=1
                deployed_revision=unknown
              fi
              if [[ "$deployed_image" != "$ORION_STUDIO_IMAGE_ID" || "$deployed_revision" != "$GITHUB_SHA" ]]; then
                echo "Post-rollout identity verification failed for $service_name" >&2
                identity_failed=1
              else
                echo "Deployed $service_name to $ORION_STUDIO_KUBE_NAMESPACE at $GITHUB_SHA"
              fi
            done
            if (( identity_failed != 0 )); then
              if ! rollback_all; then
                echo "One or more deployment rollback operations failed" >&2
              fi
              exit 1
            fi
        "#})
        .add_env(("ORION_STUDIO_IMAGE_ID", image_id.to_string()))
    }

    named::job(
        dependant_job(deps)
            .name(format!("Deploy new server image to {}", target.environment))
            .cond(Expression::new(target.condition()))
            .runs_on(runners::LINUX_XL)
            .add_env((
                "ORION_STUDIO_EXPECTED_DEPLOYMENT_ENVIRONMENT",
                target.environment,
            ))
            .add_step(
                steps::checkout_repo()
                    .with_full_history()
                    .without_persisted_credentials(),
            )
            .add_step(verify_source())
            .add_step(resolve_deployment_configuration(image_id))
            .add_step(prepare_credentials())
            .add_step(install_doctl())
            .add_step(verify_doctl_config())
            .add_step(sign_into_kubernetes())
            .add_step(start_rollout(image_id))
            .add_step(cleanup_deploy_credentials()),
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::{Result, anyhow};

    use super::*;

    fn raw_workflow() -> Result<Value> {
        let content = deploy_collab()
            .to_string()
            .map_err(|error| anyhow!("Unable to serialize the collab workflow: {error:?}"))?;
        serde_yaml::from_str(&content).context("Unable to parse the generated collab workflow")
    }

    fn transformed_workflow() -> Result<Value> {
        let workflow_file = super::super::WorkflowFile::transformed_orion_studio(
            deploy_collab,
            add_deployment_environments,
        );
        let workflow = deploy_collab();
        let content =
            workflow_file.serialize(&workflow, Path::new(".github/workflows/deploy_collab.yml"))?;
        serde_yaml::from_str(&content).context("Unable to parse the transformed collab workflow")
    }

    fn jobs(workflow: &Value) -> Result<&Mapping> {
        workflow
            .as_mapping()
            .and_then(|workflow| workflow.get(&yaml_key("jobs")))
            .and_then(Value::as_mapping)
            .context("The test collab workflow has no jobs mapping")
    }

    fn jobs_mut(workflow: &mut Value) -> Result<&mut Mapping> {
        workflow
            .as_mapping_mut()
            .and_then(|workflow| workflow.get_mut(&yaml_key("jobs")))
            .and_then(Value::as_mapping_mut)
            .context("The test collab workflow has no mutable jobs mapping")
    }

    fn job<'a>(jobs: &'a Mapping, job_id: &str) -> Result<&'a Mapping> {
        jobs.get(&yaml_key(job_id))
            .and_then(Value::as_mapping)
            .with_context(|| format!("The test collab workflow has no {job_id} job"))
    }

    fn job_mut<'a>(jobs: &'a mut Mapping, job_id: &str) -> Result<&'a mut Mapping> {
        jobs.get_mut(&yaml_key(job_id))
            .and_then(Value::as_mapping_mut)
            .with_context(|| format!("The test collab workflow has no mutable {job_id} job"))
    }

    fn field<'a>(mapping: &'a Mapping, key: &str) -> Result<&'a Value> {
        mapping
            .get(&yaml_key(key))
            .with_context(|| format!("The test YAML mapping has no {key:?} field"))
    }

    fn assert_transform_error(mut workflow: Value, expected_message: &str) -> Result<()> {
        let error = match add_deployment_environments(&mut workflow) {
            Ok(()) => return Err(anyhow!("The malformed workflow unexpectedly passed")),
            Err(error) => error,
        };
        let message = format!("{error:#}");
        if !message.contains(expected_message) {
            return Err(anyhow!(
                "Expected transform error containing {expected_message:?}, found {message:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn generated_deployment_jobs_use_fixed_environments() -> Result<()> {
        let workflow = transformed_workflow()?;
        let jobs = jobs(&workflow)?;

        if jobs.contains_key(&yaml_key("deploy")) {
            return Err(anyhow!("The legacy shared deploy job is still generated"));
        }
        if job(jobs, "publish")?.contains_key(&yaml_key("environment")) {
            return Err(anyhow!(
                "The publish job must run before deployment environment approval"
            ));
        }

        for target in DEPLOYMENT_TARGETS {
            let job = job(jobs, target.job_id)?;
            if field(job, "environment")?.as_str() != Some(target.environment) {
                return Err(anyhow!(
                    "{} does not use the fixed {:?} environment",
                    target.job_id,
                    target.environment
                ));
            }
            if field(job, "if")?.as_str() != Some(target.condition().as_str()) {
                return Err(anyhow!(
                    "{} does not use its exact repository and tag condition",
                    target.job_id
                ));
            }
            let expected_needs = [Value::String("publish".to_owned())];
            if field(job, "needs")?.as_sequence().map(Vec::as_slice)
                != Some(expected_needs.as_slice())
            {
                return Err(anyhow!("{} does not depend only on publish", target.job_id));
            }
            let expected_runtime_environment = field(job, "env")?
                .as_mapping()
                .and_then(|environment| {
                    environment.get(&yaml_key("ORION_STUDIO_EXPECTED_DEPLOYMENT_ENVIRONMENT"))
                })
                .and_then(Value::as_str);
            if expected_runtime_environment != Some(target.environment) {
                return Err(anyhow!(
                    "{} does not pin its runtime deployment target",
                    target.job_id
                ));
            }
        }

        Ok(())
    }

    #[test]
    fn deployment_jobs_retain_source_guards() -> Result<()> {
        let workflow = transformed_workflow()?;
        let jobs = jobs(&workflow)?;

        for target in DEPLOYMENT_TARGETS {
            let steps = field(job(jobs, target.job_id)?, "steps")?
                .as_sequence()
                .with_context(|| format!("{} has no steps sequence", target.job_id))?;
            let verify_source = steps
                .iter()
                .filter_map(Value::as_mapping)
                .find(|step| {
                    step.get(&yaml_key("name")).and_then(Value::as_str)
                        == Some("deploy_collab::verify_source")
                })
                .with_context(|| format!("{} has no source verification step", target.job_id))?;
            let script = field(verify_source, "run")?.as_str().with_context(|| {
                format!("{} source verification is not a script", target.job_id)
            })?;
            for guard in [
                "orion-agents/orion-studio",
                "refs/tags/collab-production|refs/tags/collab-staging",
                "origin/main^{commit}",
                "tag_sha",
            ] {
                if !script.contains(guard) {
                    return Err(anyhow!(
                        "{} source verification lost guard {guard:?}",
                        target.job_id
                    ));
                }
            }
        }

        Ok(())
    }

    #[test]
    fn environment_transform_fails_when_a_deployment_job_is_missing() -> Result<()> {
        let mut workflow = raw_workflow()?;
        jobs_mut(&mut workflow)?.remove(&yaml_key(STAGING_DEPLOYMENT.job_id));
        assert_transform_error(workflow, "deployment jobs differ from the reviewed set")
    }

    #[test]
    fn environment_transform_fails_when_a_tag_condition_drifts() -> Result<()> {
        let mut workflow = raw_workflow()?;
        job_mut(jobs_mut(&mut workflow)?, PRODUCTION_DEPLOYMENT.job_id)?
            .insert(yaml_key("if"), Value::String(REPOSITORY_GUARD.to_owned()));
        assert_transform_error(workflow, "condition must be")
    }

    #[test]
    fn environment_transform_refuses_to_overwrite_an_environment() -> Result<()> {
        let mut workflow = raw_workflow()?;
        job_mut(jobs_mut(&mut workflow)?, PRODUCTION_DEPLOYMENT.job_id)?.insert(
            yaml_key("environment"),
            Value::String("unreviewed".to_owned()),
        );
        assert_transform_error(workflow, "refusing to overwrite it")
    }

    #[test]
    fn environment_transform_fails_when_publish_is_not_the_only_dependency() -> Result<()> {
        let mut workflow = raw_workflow()?;
        job_mut(jobs_mut(&mut workflow)?, PRODUCTION_DEPLOYMENT.job_id)?.insert(
            yaml_key("needs"),
            Value::Sequence(vec![Value::String("style".to_owned())]),
        );
        assert_transform_error(workflow, "must depend only on publish")
    }

    #[test]
    fn environment_transform_fails_when_runtime_target_drifts() -> Result<()> {
        let mut workflow = raw_workflow()?;
        let job = job_mut(jobs_mut(&mut workflow)?, PRODUCTION_DEPLOYMENT.job_id)?;
        let environment = job
            .get_mut(&yaml_key("env"))
            .and_then(Value::as_mapping_mut)
            .context("The production deploy job has no mutable env mapping")?;
        environment.insert(
            yaml_key("ORION_STUDIO_EXPECTED_DEPLOYMENT_ENVIRONMENT"),
            Value::String("staging".to_owned()),
        );
        assert_transform_error(workflow, "runtime environment must be")
    }
}
