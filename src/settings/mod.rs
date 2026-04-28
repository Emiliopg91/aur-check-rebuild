use serde::{Deserialize, Serialize};
use std::fs;

const SETTINGS_FILE: &str = "/etc/aur-check-rebuild.conf";

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanSettings {
    #[serde(default)]
    pub recursive: bool,
}

impl Default for ScanSettings {
    fn default() -> Self {
        Self { recursive: true }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RebuildSettings {
    #[serde(default)]
    pub automatic: bool,
}

impl Default for RebuildSettings {
    fn default() -> Self {
        Self { automatic: true }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub scan: ScanSettings,
    #[serde(default)]
    pub rebuild: RebuildSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scan: ScanSettings::default(),
            rebuild: RebuildSettings::default(),
        }
    }
}

pub fn load_settings() -> std::io::Result<Settings> {
    let mut settings = Settings::default();

    if fs::exists(SETTINGS_FILE)? {
        let data = fs::read_to_string(SETTINGS_FILE)?;
        settings = toml::from_str(&data).unwrap_or_default();
    }

    Ok(settings)
}

pub fn save_settings(settings: &Settings) -> std::io::Result<()> {
    let toml = toml::to_string_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    std::fs::write(SETTINGS_FILE, toml)?;
    Ok(())
}
