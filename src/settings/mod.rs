use serde::{Deserialize, Serialize};
use std::fs;

const SETTINGS_FILE: &str = "/etc/aur-check-rebuild";

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
    let data = fs::read_to_string(SETTINGS_FILE)?;

    let settings: Settings = serde_json::from_str(&data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    Ok(settings)
}

pub fn save_settings(settings: &Settings) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    fs::write(SETTINGS_FILE, json)?;

    Ok(())
}
