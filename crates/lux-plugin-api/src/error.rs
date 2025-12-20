//! Error types for the Plugin API.
//!
//! All public APIs return `Result<T, PluginError>` for explicit error handling.
//! Errors are also convertible to `mlua::Error` for use in Lua callbacks.

use thiserror::Error;

/// Error type for Plugin API operations.
#[derive(Debug, Error)]
pub enum PluginError {
    /// Plugin with the given name was not found.
    #[error("Plugin '{0}' not found")]
    PluginNotFound(String),

    /// Invalid handle (component was unregistered).
    #[error("Invalid handle (component was unregistered)")]
    InvalidHandle,

    /// Lua runtime error.
    #[error("Lua error: {0}")]
    Lua(#[from] mlua::Error),

    /// View stack is empty (can't pop root).
    #[error("View stack is empty")]
    EmptyViewStack,

    /// Trigger not found.
    #[error("Trigger not found in plugin '{plugin}'")]
    TriggerNotFound { plugin: String },

    /// Source not found.
    #[error("Source not found in plugin '{plugin}'")]
    SourceNotFound { plugin: String },

    /// Action not found.
    #[error("Action not found in plugin '{plugin}'")]
    ActionNotFound { plugin: String },

    /// Channel send error.
    #[error("Channel send failed: {0}")]
    ChannelSend(String),

    /// Channel receive error.
    #[error("Channel receive failed: {0}")]
    ChannelRecv(String),
}

impl From<PluginError> for mlua::Error {
    fn from(e: PluginError) -> Self {
        mlua::Error::RuntimeError(e.to_string())
    }
}

/// Result type alias for Plugin API operations.
pub type PluginResult<T> = Result<T, PluginError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = PluginError::PluginNotFound("my-plugin".to_string());
        assert_eq!(err.to_string(), "Plugin 'my-plugin' not found");
    }

    #[test]
    fn test_into_mlua_error() {
        let err = PluginError::InvalidHandle;
        let lua_err: mlua::Error = err.into();

        match lua_err {
            mlua::Error::RuntimeError(msg) => {
                assert!(msg.contains("Invalid handle"));
            }
            _ => panic!("Expected RuntimeError"),
        }
    }
}
