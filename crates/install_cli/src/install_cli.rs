#[cfg(not(target_os = "windows"))]
mod install_cli_binary;
mod register_application_schemes;

#[cfg(not(target_os = "windows"))]
pub use install_cli_binary::{InstallCliBinary, install_cli_binary};
pub use register_application_schemes::{RegisterApplicationSchemes, register_application_schemes};
