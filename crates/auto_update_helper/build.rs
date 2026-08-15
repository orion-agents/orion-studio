fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-env-changed=ORION_STUDIO_RELEASE_CHANNEL");
        println!("cargo:rerun-if-env-changed=ZED_RELEASE_CHANNEL");
        println!("cargo:rerun-if-env-changed=RELEASE_CHANNEL");
        println!("cargo:rerun-if-env-changed=ORION_STUDIO_COMMIT_SHA");
        println!("cargo:rerun-if-env-changed=ZED_COMMIT_SHA");
        println!("cargo:rerun-if-env-changed=ORION_STUDIO_RC_TOOLKIT_PATH");
        println!("cargo:rerun-if-env-changed=ZED_RC_TOOLKIT_PATH");
        println!("cargo:rerun-if-env-changed=GITHUB_RUN_NUMBER");

        windows_resources::compile(true)?;
    }

    Ok(())
}
