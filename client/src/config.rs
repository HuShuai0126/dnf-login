use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::i18n::Language;

/// Controls how a background image is scaled to fit the window,
/// matching the five modes available in Windows desktop wallpaper settings.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum BgFillMode {
    /// Repeat the image in a grid without scaling.
    Tile,
    /// Stretch the image to exactly fill the window, ignoring aspect ratio.
    Stretch,
    /// Scale the image uniformly until it covers the window, then center-crop.
    #[default]
    Fill,
    /// Display the image at its decoded pixel size, centered; bars show for smaller images.
    Center,
    /// Scale the image uniformly to fit within the window; letterbox bars fill the remainder.
    Fit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub server_url: String,
    pub aes_key: String,
    pub language: Language,
    /// Directory scanned for additional JPG background images at load time.
    /// Relative to the working directory when the launcher starts.
    pub bg_custom_path: String,
    /// When true, custom images are placed before the built-in set; otherwise they follow.
    pub bg_custom_prepend: bool,
    /// How background images are scaled to fill the window.
    pub bg_fill_mode: BgFillMode,
    /// Index of the last selected background image.
    #[serde(default)]
    pub bg_index: usize,
    /// Plugin directory path passed to DNF.exe via environment variable.
    pub plugins_path: String,
    /// Controls the DNF_PLUGIN_ENABLED environment variable passed to DNF.exe.
    pub plugin_inject_enabled: bool,
    /// When true, the launcher fetches the game server IP at login
    /// and passes it as the GAME_SERVER_IP environment variable to DNF.exe.
    #[serde(default = "default_true")]
    pub game_server_ip_enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            aes_key: String::new(),
            language: Language::default(),
            bg_custom_path: "assets/bg".to_string(),
            bg_custom_prepend: false,
            bg_fill_mode: BgFillMode::Fill,
            bg_index: 0,
            plugins_path: "plugins".to_string(),
            plugin_inject_enabled: true,
            game_server_ip_enabled: true,
        }
    }
}

impl AppConfig {
    fn config_path() -> Result<PathBuf> {
        let config_path = std::env::current_dir()?.join("Config.toml");
        tracing::debug!("Config file path: {}", config_path.display());
        Ok(config_path)
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if !path.exists() {
            tracing::info!("Config file not found at: {}", path.display());
            let config = Self::default();
            return Ok(config);
        }

        tracing::info!("Loading config from: {}", path.display());
        let content = std::fs::read_to_string(&path)?;
        let config: AppConfig = toml::from_str(&content)?;
        tracing::info!(
            "Config loaded: server_url={}, aes_key_len={}",
            config.server_url,
            config.aes_key.len()
        );
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        tracing::info!("Config saved to: {}", path.display());
        Ok(())
    }

    /// Parse the AES key into 32 bytes.
    /// Only accepts exactly 64 hexadecimal characters (0–9, a–f, A–F).
    pub fn get_aes_key_bytes(&self) -> Result<[u8; 32]> {
        let decoded = hex::decode(&self.aes_key)
            .map_err(|e| anyhow::anyhow!("AES key is not valid hex: {}", e))?;
        if decoded.len() != 32 {
            anyhow::bail!(
                "AES key must decode to exactly 32 bytes (got {})",
                decoded.len()
            );
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&decoded);
        Ok(key)
    }

    pub fn validate(&self) -> Result<()> {
        if self.server_url.is_empty() {
            anyhow::bail!("Server URL must not be empty");
        }
        if !self.server_url.starts_with("http://") && !self.server_url.starts_with("https://") {
            anyhow::bail!("Server URL must begin with http:// or https://");
        }
        if self.aes_key.is_empty() {
            anyhow::bail!("AES key must not be empty");
        }
        if self.aes_key.len() != 64 {
            anyhow::bail!(
                "AES key must be exactly 64 hex characters (got {})",
                self.aes_key.len()
            );
        }
        if !self.aes_key.chars().all(|c| c.is_ascii_hexdigit()) {
            anyhow::bail!("AES key must contain only hex characters (0-9, a-f, A-F)");
        }
        Ok(())
    }

    pub fn is_configured(&self) -> bool {
        !self.server_url.is_empty() && !self.aes_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert!(config.server_url.is_empty());
        assert!(config.aes_key.is_empty());
        assert!(!config.is_configured());
    }

    #[test]
    fn test_validate() {
        let mut config = AppConfig::default();
        // Default config has no URL or key set — should fail.
        assert!(config.validate().is_err());

        // Invalid URL scheme.
        config.server_url = "ftp://example.com".to_string();
        assert!(config.validate().is_err());

        // Valid URL but still no AES key — should fail.
        config.server_url = "https://example.com".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_aes_key_bytes() {
        let mut config = AppConfig::default();

        // Valid 64-char hex key
        config.aes_key =
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string();
        let key_bytes = config.get_aes_key_bytes().unwrap();
        assert_eq!(key_bytes.len(), 32);
        assert_eq!(key_bytes[0], 0x01);
        assert_eq!(key_bytes[1], 0x23);

        // Too short — must fail
        config.aes_key = "deadbeef".to_string();
        assert!(config.get_aes_key_bytes().is_err());

        // Non-hex characters — must fail
        config.aes_key =
            "gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg".to_string();
        assert!(config.get_aes_key_bytes().is_err());
    }

    #[test]
    fn test_bg_index_defaults_when_absent() {
        let toml_str = r#"
server_url = ""
aes_key = ""
language = "en"
bg_custom_path = "assets/bg"
bg_custom_prepend = false
bg_fill_mode = "Fill"
plugins_path = "plugins"
plugin_inject_enabled = true
game_server_ip_enabled = true
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.bg_index, 0);
    }

    #[test]
    fn test_bg_index_round_trip() {
        let mut config = AppConfig::default();
        config.bg_index = 7;
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: AppConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.bg_index, 7);
    }
}
