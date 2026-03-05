use crate::platform;
use anyhow::Result;

pub struct DnfLauncher;

impl DnfLauncher {
    pub fn launch_with_token(token: &str, plugins_dir: &str) -> Result<()> {
        // Process running check is performed inside platform::launch_dnf.
        tracing::info!("Starting DNF.exe with authentication token");
        platform::launch_dnf(token, plugins_dir)?;
        tracing::info!("DNF launched");
        Ok(())
    }
}
