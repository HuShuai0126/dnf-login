use crate::platform;
use anyhow::Result;

pub struct DnfLauncher;

impl DnfLauncher {
    pub fn launch_with_token(
        token: &str,
        plugins_path: &str,
        inject_enabled: bool,
        server_ip: &str,
    ) -> Result<()> {
        // Process running check is performed inside platform::launch_dnf.
        platform::launch_dnf(token, plugins_path, inject_enabled, server_ip)?;
        tracing::info!("DNF launched");
        Ok(())
    }
}
