//! Configuration types.
//!
//! All configuration is managed through init.lua. These types represent
//! the runtime configuration that can be set via Lua.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Runtime configuration set via init.lua.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// Hotkey configuration
    #[serde(default)]
    pub hotkey: HotkeyConfig,

    /// Appearance settings
    #[serde(default)]
    pub appearance: AppearanceConfig,
}

/// Hotkey configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    /// Toggle hotkey string, e.g., "cmd+space"
    pub toggle: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            toggle: "cmd+space".to_string(),
        }
    }
}

/// Appearance configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppearanceConfig {
    /// Theme mode: "light", "dark", or "system"
    #[serde(default)]
    pub theme: ThemeMode,

    /// Accent color (hex string)
    pub accent_color: Option<String>,
}

/// Theme mode selection.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    System,
}

/// Get the path to init.lua.
pub fn init_lua_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("lux/init.lua"))
}

/// Get the config directory path.
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("lux"))
}

/// Ensure the config directory exists.
pub fn ensure_config_dir() -> std::io::Result<()> {
    if let Some(dir) = config_dir() {
        std::fs::create_dir_all(dir)?;
    }
    Ok(())
}
