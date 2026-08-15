use std::path::Path;

use crate::tasks::workflows::{
    release::ReleaseBundleJobs,
    runners::{Arch, Platform, ReleaseChannel},
    steps::{
        CommonPermissionSets, FluentBuilder, IfNoFilesFound, NamedJob, UploadArtifactStep,
        dependant_job, named,
    },
    vars::{self, assets, bundle_job_envs, bundle_step_envs},
};

use super::{runners, steps};
use gh_workflow::*;
use indoc::indoc;

pub fn run_bundling() -> Workflow {
    let bundle = ReleaseBundleJobs {
        linux_aarch64: bundle_linux(Arch::AARCH64, None, false, &[]),
        linux_x86_64: bundle_linux(Arch::X86_64, None, false, &[]),
        bwrap_linux_aarch64: build_static_bwrap(Arch::AARCH64, false, &[]),
        bwrap_linux_x86_64: build_static_bwrap(Arch::X86_64, false, &[]),
        mac_aarch64: bundle_mac(Arch::AARCH64, None, false, &[]),
        mac_x86_64: bundle_mac(Arch::X86_64, None, false, &[]),
        windows_aarch64: bundle_windows(Arch::AARCH64, None, false, &[]),
        windows_x86_64: bundle_windows(Arch::X86_64, None, false, &[]),
    };
    named::workflow()
        .with_minimal_permissions()
        .on(Event::default().pull_request(
            PullRequest::default().types([PullRequestType::Labeled, PullRequestType::Synchronize]),
        ))
        .concurrency(
            Concurrency::new(Expression::new(
                "${{ github.workflow }}-${{ github.head_ref || github.ref }}",
            ))
            .cancel_in_progress(true),
        )
        .add_env(("CARGO_TERM_COLOR", "always"))
        .add_env(("RUST_BACKTRACE", "1"))
        .map(|mut workflow| {
            for job in bundle.into_jobs() {
                workflow = workflow.add_job(job.name, job.job);
            }
            workflow
        })
}

fn bundle_job(deps: &[&NamedJob]) -> Job {
    dependant_job(deps)
        .when(deps.len() == 0, |job|
            job.cond(Expression::new(
                indoc! {
                    r#"(github.event.action == 'labeled' && github.event.label.name == 'run-bundling') ||
                    (github.event.action == 'synchronize' && contains(github.event.pull_request.labels.*.name, 'run-bundling'))"#,
                })))
        .timeout_minutes(60u32)
}

pub(crate) fn bundle_mac(
    arch: Arch,
    release_channel: Option<ReleaseChannel>,
    release_build: bool,
    deps: &[&NamedJob],
) -> NamedJob {
    pub fn bundle_mac(arch: Arch) -> Step<Run> {
        named::bash(&format!("./script/bundle-mac {arch}-apple-darwin"))
    }
    let platform = Platform::Mac;
    let artifact_name = match arch {
        Arch::X86_64 => assets::MAC_X86_64,
        Arch::AARCH64 => assets::MAC_AARCH64,
    };
    let remote_server_artifact_name = match arch {
        Arch::X86_64 => assets::REMOTE_SERVER_MAC_X86_64,
        Arch::AARCH64 => assets::REMOTE_SERVER_MAC_AARCH64,
    };
    NamedJob {
        name: format!("bundle_mac_{arch}"),
        job: bundle_job(deps)
            .runs_on(runners::MAC_DEFAULT)
            .envs(bundle_job_envs())
            .when(release_build, |job| {
                job.add_env(Env::new("ORION_STUDIO_REQUIRE_SIGNED_RELEASE", "1"))
                    .add_env(Env::new("ORION_STUDIO_REQUIRE_SENTRY", "1"))
            })
            .add_step(steps::checkout_repo().without_persisted_credentials())
            .add_step(steps::cache_rust_dependencies_namespace())
            .when_some(release_channel, |job, release_channel| {
                job.add_step(set_release_channel(platform, release_channel))
            })
            .when(release_build, |job| {
                job.add_step(validate_release_channel(platform))
            })
            .add_step(steps::setup_node())
            .when(release_build, |job| job.add_step(steps::setup_sentry()))
            .add_step(steps::clear_target_dir_if_large(runners::Platform::Mac))
            .add_step(bundle_mac(arch).envs(bundle_step_envs(platform, release_build)))
            .add_step(upload_artifact(&format!(
                "target/{arch}-apple-darwin/release/{artifact_name}"
            )))
            .add_step(upload_artifact(&format!(
                "target/{remote_server_artifact_name}"
            ))),
    }
}

pub fn upload_artifact(path: &str) -> UploadArtifactStep {
    let name = Path::new(path).file_name().unwrap().to_str().unwrap();
    steps::upload_artifact(name, path).if_no_files_found(IfNoFilesFound::Error)
}

pub(crate) fn build_static_bwrap(arch: Arch, release_build: bool, deps: &[&NamedJob]) -> NamedJob {
    let artifact_name = match arch {
        Arch::X86_64 => assets::BWRAP_LINUX_X86_64,
        Arch::AARCH64 => assets::BWRAP_LINUX_AARCH64,
    };
    let binary_name = artifact_name
        .strip_suffix(".gz")
        .expect("static bwrap artifact name should end in .gz");
    let copy_artifact = indoc::formatdoc! {r#"
        cp result/bin/bwrap {binary_name}
        chmod 755 {binary_name}
        gzip -f --stdout --best {binary_name} > {artifact_name}
    "#};

    NamedJob {
        name: format!("build_static_bwrap_linux_{arch}"),
        job: bundle_job(deps)
            .runs_on(arch.linux_bundler())
            .timeout_minutes(60u32)
            .add_step(steps::cache_nix_dependencies_namespace())
            .add_step({
                let step = named::uses(
                    "cachix",
                    "install-nix-action",
                    "02a151ada4993995686f9ed4f1be7cfbb229e56f", // v31
                );
                if release_build {
                    step.add_with(("github_access_token", vars::GITHUB_TOKEN))
                } else {
                    step
                }
            })
            .add_step({
                let step = named::uses(
                    "cachix",
                    "cachix-action",
                    "0fc020193b5a1fa3ac4575aa3a7d3aa6a35435ad", // v16
                )
                .add_with(("name", vars::ORION_STUDIO_CACHIX_CACHE_NAME))
                .add_with(("cachixArgs", "-v"));
                if release_build {
                    step.add_with(("authToken", vars::CACHIX_AUTH_TOKEN))
                } else {
                    step
                }
            })
            .add_step(named::bash("nix build nixpkgs#pkgsStatic.bubblewrap -L"))
            .add_step(named::bash(&copy_artifact))
            .add_step(upload_artifact(artifact_name)),
    }
}

pub(crate) fn bundle_linux(
    arch: Arch,
    release_channel: Option<ReleaseChannel>,
    release_build: bool,
    deps: &[&NamedJob],
) -> NamedJob {
    let platform = Platform::Linux;
    let artifact_name = match arch {
        Arch::X86_64 => assets::LINUX_X86_64,
        Arch::AARCH64 => assets::LINUX_AARCH64,
    };
    let remote_server_artifact_name = match arch {
        Arch::X86_64 => assets::REMOTE_SERVER_LINUX_X86_64,
        Arch::AARCH64 => assets::REMOTE_SERVER_LINUX_AARCH64,
    };
    NamedJob {
        name: format!("bundle_linux_{arch}"),
        job: bundle_job(deps)
            .runs_on(arch.linux_bundler())
            .envs(bundle_job_envs())
            .add_env(Env::new("CC", "clang-18"))
            .add_env(Env::new("CXX", "clang++-18"))
            .when(release_build, |job| {
                job.add_env(Env::new("ORION_STUDIO_REQUIRE_SENTRY", "1"))
            })
            .add_step(steps::checkout_repo().without_persisted_credentials())
            .add_step(steps::cache_rust_dependencies_namespace())
            .when_some(release_channel, |job, release_channel| {
                job.add_step(set_release_channel(platform, release_channel))
            })
            .when(release_build, |job| {
                job.add_step(validate_release_channel(platform))
            })
            .when(release_build, |job| job.add_step(steps::setup_sentry()))
            .map(steps::install_linux_dependencies)
            .add_step(
                steps::script("./script/bundle-linux")
                    .envs(bundle_step_envs(platform, release_build)),
            )
            .add_step(upload_artifact(&format!("target/release/{artifact_name}")))
            .add_step(upload_artifact(&format!(
                "target/{remote_server_artifact_name}"
            ))),
    }
}

pub(crate) fn bundle_windows(
    arch: Arch,
    release_channel: Option<ReleaseChannel>,
    release_build: bool,
    deps: &[&NamedJob],
) -> NamedJob {
    let platform = Platform::Windows;
    pub fn bundle_windows(arch: Arch) -> Step<Run> {
        let step = match arch {
            Arch::X86_64 => named::pwsh("script/bundle-windows.ps1 -Architecture x86_64"),
            Arch::AARCH64 => named::pwsh("script/bundle-windows.ps1 -Architecture aarch64"),
        };
        step.working_directory("${{ github.workspace }}")
    }
    let artifact_name = match (arch, release_build) {
        (Arch::X86_64, true) => assets::WINDOWS_X86_64,
        (Arch::AARCH64, true) => assets::WINDOWS_AARCH64,
        (Arch::X86_64, false) => "Orion-Studio-Dev-Unsigned-x86_64.exe",
        (Arch::AARCH64, false) => "Orion-Studio-Dev-Unsigned-aarch64.exe",
    };
    let remote_server_artifact_name = match arch {
        Arch::X86_64 => assets::REMOTE_SERVER_WINDOWS_X86_64,
        Arch::AARCH64 => assets::REMOTE_SERVER_WINDOWS_AARCH64,
    };
    NamedJob {
        name: format!("bundle_windows_{arch}"),
        job: bundle_job(deps)
            .runs_on(runners::WINDOWS_DEFAULT)
            .envs(bundle_job_envs())
            .when(release_build, |job| {
                job.add_env(Env::new("ORION_STUDIO_REQUIRE_SENTRY", "1"))
                    .add_env(Env::new("ORION_STUDIO_REQUIRE_SIGNED_RELEASE", "1"))
            })
            .add_step(steps::checkout_repo().without_persisted_credentials())
            .when_some(release_channel, |job, release_channel| {
                job.add_step(set_release_channel(platform, release_channel))
            })
            .when(release_build, |job| {
                job.add_step(validate_release_channel(platform))
            })
            .when(release_build, |job| job.add_step(steps::setup_sentry()))
            .add_step(steps::clear_target_dir_if_large(platform))
            .add_step(bundle_windows(arch).envs(bundle_step_envs(platform, release_build)))
            .add_step(upload_artifact(&format!("target/{artifact_name}")))
            .add_step(upload_artifact(&format!(
                "target/{remote_server_artifact_name}"
            ))),
    }
}

fn set_release_channel(platform: Platform, release_channel: ReleaseChannel) -> Step<Run> {
    match release_channel {
        ReleaseChannel::Nightly => set_release_channel_to_nightly(platform),
    }
}

fn validate_release_channel(platform: Platform) -> Step<Run> {
    match platform {
        Platform::Linux | Platform::Mac => named::bash(indoc::indoc! {r#"
            set -euo pipefail
            channel=$(tr -d '\r\n' < crates/zed/RELEASE_CHANNEL)
            case "$channel" in
                stable)
                    if [[ "${GITHUB_REF_TYPE:-}" != "tag" || ! "${GITHUB_REF_NAME:-}" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
                        echo "::error::stable artifacts require an exact vMAJOR.MINOR.PATCH tag"
                        exit 1
                    fi
                    ;;
                preview)
                    if [[ "${GITHUB_REF_TYPE:-}" != "tag" || ! "${GITHUB_REF_NAME:-}" =~ ^v[0-9]+\.[0-9]+\.[0-9]+-pre$ ]]; then
                        echo "::error::preview artifacts require an exact vMAJOR.MINOR.PATCH-pre tag"
                        exit 1
                    fi
                    ;;
                nightly)
                    if [[ "${GITHUB_REF:-}" != "refs/heads/main" ]]; then
                        echo "::error::nightly artifacts must be built from refs/heads/main"
                        exit 1
                    fi
                    ;;
                *)
                    echo "::error::release artifacts cannot use channel '$channel'"
                    exit 1
                    ;;
            esac
        "#}),
        Platform::Windows => named::pwsh(indoc::indoc! {r#"
            $ErrorActionPreference = "Stop"
            $channel = (Get-Content "crates/zed/RELEASE_CHANNEL" -Raw).Trim()
            switch ($channel) {
                "stable" {
                    if ($env:GITHUB_REF_TYPE -ne "tag" -or $env:GITHUB_REF_NAME -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') {
                        throw "stable artifacts require an exact vMAJOR.MINOR.PATCH tag"
                    }
                }
                "preview" {
                    if ($env:GITHUB_REF_TYPE -ne "tag" -or $env:GITHUB_REF_NAME -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+-pre$') {
                        throw "preview artifacts require an exact vMAJOR.MINOR.PATCH-pre tag"
                    }
                }
                "nightly" {
                    if ($env:GITHUB_REF -ne "refs/heads/main") {
                        throw "nightly artifacts must be built from refs/heads/main"
                    }
                }
                default {
                    throw "release artifacts cannot use channel '$channel'"
                }
            }
        "#})
        .working_directory("${{ github.workspace }}"),
    }
}

fn set_release_channel_to_nightly(platform: Platform) -> Step<Run> {
    match platform {
        Platform::Linux | Platform::Mac => named::bash(indoc::indoc! {r#"
            set -eu
            version=$(git rev-parse --short HEAD)
            echo "Publishing version: ${version} on release channel nightly"
            echo "nightly" > crates/zed/RELEASE_CHANNEL
        "#}),
        Platform::Windows => named::pwsh(indoc::indoc! {r#"
            $ErrorActionPreference = "Stop"
            $version = git rev-parse --short HEAD
            Write-Host "Publishing version: $version on release channel nightly"
            "nightly" | Set-Content -Path "crates/zed/RELEASE_CHANNEL"
        "#})
        .working_directory("${{ github.workspace }}"),
    }
}
