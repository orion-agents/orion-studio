use std::{env, error::Error, io};

const ORION_STUDIO_REPO_DIR: &str = "ORION_STUDIO_REPO_DIR";
const ZED_REPO_DIR: &str = "ZED_REPO_DIR";

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-env-changed={ORION_STUDIO_REPO_DIR}");
    println!("cargo:rerun-if-env-changed={ZED_REPO_DIR}");

    let repo_dir = match read_canonical_or_legacy(ORION_STUDIO_REPO_DIR, ZED_REPO_DIR)? {
        Some(repo_dir) => repo_dir,
        None => repository_dir()?,
    };

    println!("cargo:rustc-env={ORION_STUDIO_REPO_DIR}={repo_dir}");
    // Keep the legacy compile-time name for consumers that have not migrated
    // their option_env! lookup yet.
    println!("cargo:rustc-env={ZED_REPO_DIR}={repo_dir}");
    Ok(())
}

fn repository_dir() -> Result<String, Box<dyn Error>> {
    let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR")?;
    let mut path = std::path::PathBuf::from(&cargo_manifest_dir);

    if path.file_name().as_ref().and_then(|name| name.to_str()) != Some("inspector_ui") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "expected CARGO_MANIFEST_DIR to end with crates/inspector_ui, but got {cargo_manifest_dir}"
            ),
        )
        .into());
    }
    path.pop();

    if path.file_name().as_ref().and_then(|name| name.to_str()) != Some("crates") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "expected CARGO_MANIFEST_DIR to end with crates/inspector_ui, but got {cargo_manifest_dir}"
            ),
        )
        .into());
    }
    path.pop();

    let path = path.into_os_string().into_string().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "repository path contains non-Unicode data",
        )
    })?;
    Ok(path)
}

fn read_canonical_or_legacy(
    canonical_name: &str,
    legacy_name: &str,
) -> Result<Option<String>, io::Error> {
    match read_unicode_environment_variable(canonical_name)? {
        Some(value) => Ok(Some(value)),
        None => read_unicode_environment_variable(legacy_name),
    }
}

fn read_unicode_environment_variable(name: &str) -> Result<Option<String>, io::Error> {
    let Some(value) = env::var_os(name) else {
        return Ok(None);
    };
    value.into_string().map(Some).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{name} contains non-Unicode data"),
        )
    })
}
