//! Error types for the Lux launcher.

use std::time::Duration;
use thiserror::Error;

/// Backend errors - surfaced to UI.
#[derive(Debug, Error)]
pub enum BackendError {
    /// Lua script error.
    #[error("Lua error: {0}")]
    Lua(String),

    /// Plugin-specific error.
    #[error("Plugin '{plugin}' error: {message}")]
    Plugin { plugin: String, message: String },

    /// Lua runtime timeout.
    #[error("Lua runtime timeout after {duration:?}")]
    Timeout { duration: Duration },

    /// Lua runtime unavailable (e.g., not initialized).
    #[error("Lua runtime unavailable")]
    RuntimeUnavailable,

    /// Channel communication error.
    #[error("Channel error: {0}")]
    Channel(String),
}

/// Configuration errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// No config directory found.
    #[error("Config directory not found")]
    NoConfigDir,

    /// IO error.
    #[error("IO error: {0}")]
    Io(String),

    /// Parse error.
    #[error("Parse error: {0}")]
    Parse(String),

    /// Invalid hotkey format.
    #[error("Invalid hotkey: {0}")]
    InvalidHotkey(String),
}
