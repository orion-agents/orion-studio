use std::{env, error::Error, io};

const ORION_STUDIO_PKG_VERSION: &str = "ORION_STUDIO_PKG_VERSION";
const ZED_PKG_VERSION: &str = "ZED_PKG_VERSION";

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=../zed/Cargo.toml");
    println!("cargo:rerun-if-env-changed={ORION_STUDIO_PKG_VERSION}");
    println!("cargo:rerun-if-env-changed={ZED_PKG_VERSION}");

    let version = match read_canonical_or_legacy(ORION_STUDIO_PKG_VERSION, ZED_PKG_VERSION)? {
        Some(version) => version,
        None => package_version()?,
    };

    println!("cargo:rustc-env={ORION_STUDIO_PKG_VERSION}={version}");
    // Keep the legacy compile-time name for consumers that have not migrated
    // their option_env! lookup yet.
    println!("cargo:rustc-env={ZED_PKG_VERSION}={version}");
    Ok(())
}

fn package_version() -> Result<String, Box<dyn Error>> {
    let cargo_toml = std::fs::read_to_string("../zed/Cargo.toml")?;
    let version = cargo_toml
        .lines()
        .find_map(|line| line.strip_prefix("version = "))
        .map(|version| version.trim().trim_matches('"').to_string())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "version not found in crates/zed/Cargo.toml",
            )
        })?;
    Ok(version)
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
