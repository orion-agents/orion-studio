fn main() {
    println!("cargo::rustc-check-cfg=cfg(__do_not_set_zed_release_channel)");

    // Canonical build-time channel override is `ORION_STUDIO_RELEASE_CHANNEL`.
    // The legacy `ZED_RELEASE_CHANNEL` is still honored as a compatibility
    // fallback (S02/S11). Either one triggers the same compile-time `cfg` that
    // switches `compile_time_release_channel_name` to read the env-var value
    // instead of the `RELEASE_CHANNEL` file.
    println!("cargo::rerun-if-env-changed=ORION_STUDIO_RELEASE_CHANNEL");
    println!("cargo::rerun-if-env-changed=ZED_RELEASE_CHANNEL");

    // Enabling this `cfg` will cause a runtime panic if neither channel env var
    // is set. TLDR: don't set the `cfg` directly, just set one of the env vars
    // (hence the name, which predates the Orion rename).
    if std::env::var("ORION_STUDIO_RELEASE_CHANNEL").is_ok()
        || std::env::var("ZED_RELEASE_CHANNEL").is_ok()
    {
        println!("cargo::rustc-cfg=__do_not_set_zed_release_channel");
    }
}
