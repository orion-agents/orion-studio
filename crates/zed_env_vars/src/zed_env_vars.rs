pub use env_var::{EnvVar, bool_env_var, env_var};
use std::sync::LazyLock;

/// Whether Orion Studio is running in stateless mode.
/// When true, it uses in-memory databases instead of persistent storage.
///
/// Canonical env var: `ORION_STUDIO_STATELESS`.
/// The legacy `ZED_STATELESS` variable is still read as a compatibility
/// fallback (never written back). It will be removed once all callers and
/// tooling migrate to the canonical variable (tracked in S06/S11).
fn stateless_from_env() -> bool {
    EnvVar::new("ORION_STUDIO_STATELESS".into()).value.is_some()
        || EnvVar::new("ZED_STATELESS".into()).value.is_some()
}

/// Canonical flag for stateless mode. See [`stateless_from_env`].
pub static ORION_STUDIO_STATELESS: LazyLock<bool> = LazyLock::new(stateless_from_env);

/// Deprecated compatibility alias for [`ORION_STUDIO_STATELESS`].
///
/// Still reads the legacy `ZED_STATELESS` env var. Do not use in new code;
/// scheduled for removal after the migration window (S06/S11).
pub static ZED_STATELESS: LazyLock<bool> = LazyLock::new(stateless_from_env);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_env_takes_precedence_and_legacy_still_reads() {
        unsafe {
            std::env::remove_var("ZED_STATELESS");
            std::env::set_var("ORION_STUDIO_STATELESS", "1");
        }
        assert!(stateless_from_env());

        unsafe {
            std::env::remove_var("ORION_STUDIO_STATELESS");
            std::env::set_var("ZED_STATELESS", "1");
        }
        assert!(stateless_from_env());

        // An empty legacy value must not count as set (matches EnvVar semantics).
        unsafe {
            std::env::set_var("ZED_STATELESS", "");
        }
        assert!(!stateless_from_env());

        unsafe {
            std::env::remove_var("ZED_STATELESS");
            std::env::remove_var("ORION_STUDIO_STATELESS");
        }
        assert!(!stateless_from_env());
    }
}
