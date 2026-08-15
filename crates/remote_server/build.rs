#![allow(clippy::disallowed_methods, reason = "build scripts are exempt")]
use std::process::Command;

const ZED_MANIFEST: &str = include_str!("../zed/Cargo.toml");

fn main() {
    let zed_cargo_toml: cargo_toml::Manifest =
        toml::from_str(ZED_MANIFEST).expect("failed to parse zed Cargo.toml");
    let pkg_version = zed_cargo_toml.package.unwrap().version.unwrap();
    // Emit both the canonical `ORION_STUDIO_PKG_VERSION` and the legacy
    // `ZED_PKG_VERSION` so binaries built before the rename keep resolving the
    // same version.
    println!("cargo:rustc-env=ORION_STUDIO_PKG_VERSION={pkg_version}");
    println!("cargo:rustc-env=ZED_PKG_VERSION={pkg_version}");
    println!(
        "cargo:rustc-env=TARGET={}",
        std::env::var("TARGET").unwrap()
    );

    // Populate git sha environment variable if git is available
    println!("cargo:rerun-if-changed=../../.git/logs/HEAD");
    println!("cargo:rerun-if-env-changed=ORION_STUDIO_COMMIT_SHA");
    println!("cargo:rerun-if-env-changed=ZED_COMMIT_SHA");
    println!("cargo:rerun-if-env-changed=ORION_STUDIO_BUILD_ID");
    println!("cargo:rerun-if-env-changed=ZED_BUILD_ID");

    // Canonical input is `ORION_STUDIO_COMMIT_SHA`; the legacy `ZED_COMMIT_SHA`
    // is still accepted as a fallback (S02/S11). When neither is injected we
    // fall back to reading the current git HEAD.
    let injected = std::env::var("ORION_STUDIO_COMMIT_SHA")
        .ok()
        .or_else(|| std::env::var("ZED_COMMIT_SHA").ok());
    let git_sha = match injected {
        Some(sha) => Some(sha.trim().to_string()),
        None => Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string()),
    };

    if let Some(git_sha) = git_sha {
        // Emit both the canonical `ORION_STUDIO_*` and the legacy `ZED_*` names
        // so existing consumers keep resolving the same value.
        println!("cargo:rustc-env=ORION_STUDIO_COMMIT_SHA={git_sha}");
        println!("cargo:rustc-env=ZED_COMMIT_SHA={git_sha}");
    }
    if let Some(build_identifier) = option_env!("GITHUB_RUN_NUMBER") {
        println!("cargo:rustc-env=ORION_STUDIO_BUILD_ID={build_identifier}");
        println!("cargo:rustc-env=ZED_BUILD_ID={build_identifier}");
    }
}
