fn main() {
    println!("cargo:rerun-if-env-changed=ORION_STUDIO_BUNDLE");
    println!("cargo:rerun-if-env-changed=ZED_BUNDLE");

    let bundled = match std::env::var("ORION_STUDIO_BUNDLE") {
        Err(std::env::VarError::NotPresent) => std::env::var("ZED_BUNDLE").ok(),
        Ok(bundled) => Some(bundled),
        Err(std::env::VarError::NotUnicode(_)) => None,
    };
    if let Some(bundled) = bundled {
        println!("cargo:rustc-env=ORION_STUDIO_BUNDLE={bundled}");
        // Keep emitting the old compile-time name for ABI consumers that have
        // not yet migrated their option_env! lookup.
        println!("cargo:rustc-env=ZED_BUNDLE={bundled}");
    }
}
