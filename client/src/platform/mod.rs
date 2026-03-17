#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(not(target_os = "windows"))]
pub fn get_mac_address() -> anyhow::Result<String> {
    anyhow::bail!("This application only supports Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn is_process_running(_process_name: &str) -> anyhow::Result<bool> {
    anyhow::bail!("This application only supports Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn launch_dnf(
    _token: &str,
    _plugins_path: &str,
    _inject_enabled: bool,
    _server_ip: &str,
) -> anyhow::Result<()> {
    anyhow::bail!("This application only supports Windows")
}
