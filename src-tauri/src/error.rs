//! Centralized error types for the Lux launcher.
//!
//! This module provides a unified error type that:
//! - Uses thiserror for ergonomic error handling
//! - Implements Serialize for Tauri command compatibility
//! - Provides meaningful error messages for the frontend

use serde::Serialize;
use std::path::PathBuf;
use thiserror::Error;

/// The main error type for all Lux operations.
///
/// This enum covers all error cases that can occur during:
/// - Plugin registration and execution
/// - Lua script loading and evaluation
/// - Search operations
/// - Action execution
/// - Configuration loading
#[derive(Debug, Error)]
pub enum AppError {
    /// A referenced plugin was not found in the registry.
    #[error("Plugin not found: {0}")]
    PluginNotFound(String),

    /// Plugin execution failed (trigger, source, or action).
    #[error("Plugin '{plugin_name}' failed: {message}")]
    PluginExecution {
        plugin_name: String,
        message: String,
    },

    /// A search query was malformed or invalid.
    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    /// A Lua error occurred during script execution.
    #[error("Lua error: {0}")]
    Lua(#[from] mlua::Error),

    /// An I/O error occurred (file read/write, etc.).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Configuration file error.
    #[error("Config error in {path:?}: {message}")]
    Config { path: PathBuf, message: String },

    /// An action was not found for the given plugin.
    #[error("Action not found: plugin '{plugin_name}', index {action_index}")]
    ActionNotFound {
        plugin_name: String,
        action_index: usize,
    },

    /// A trigger was not found.
    #[error("Trigger not found: {0}")]
    TriggerNotFound(String),

    /// A source was not found.
    #[error("Source not found: plugin '{plugin_name}', source '{source_name}'")]
    SourceNotFound {
        plugin_name: String,
        source_name: String,
    },

    /// View stack is empty when an operation required a view.
    #[error("View stack is empty")]
    EmptyViewStack,

    /// A Lua function reference was invalid or not found.
    #[error("Invalid Lua function reference: {0}")]
    InvalidFunctionRef(String),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// An internal error that shouldn't happen in normal operation.
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Serialize AppError as a tagged enum for structured frontend handling.
///
/// The frontend receives errors in the format:
/// ```json
/// { "kind": "PluginNotFound", "message": "Plugin not found: my-plugin" }
/// ```
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        use serde::ser::SerializeStruct;

        let (kind, message) = match self {
            AppError::PluginNotFound(name) => {
                ("PluginNotFound", format!("Plugin not found: {}", name))
            }
            AppError::PluginExecution {
                plugin_name,
                message,
            } => (
                "PluginExecution",
                format!("Plugin '{}' failed: {}", plugin_name, message),
            ),
            AppError::InvalidQuery(msg) => ("InvalidQuery", format!("Invalid query: {}", msg)),
            AppError::Lua(e) => ("LuaError", format!("Lua error: {}", e)),
            AppError::Io(e) => ("IoError", format!("I/O error: {}", e)),
            AppError::Config { path, message } => (
                "ConfigError",
                format!("Config error in {:?}: {}", path, message),
            ),
            AppError::ActionNotFound {
                plugin_name,
                action_index,
            } => (
                "ActionNotFound",
                format!(
                    "Action not found: plugin '{}', index {}",
                    plugin_name, action_index
                ),
            ),
            AppError::TriggerNotFound(name) => {
                ("TriggerNotFound", format!("Trigger not found: {}", name))
            }
            AppError::SourceNotFound {
                plugin_name,
                source_name,
            } => (
                "SourceNotFound",
                format!(
                    "Source not found: plugin '{}', source '{}'",
                    plugin_name, source_name
                ),
            ),
            AppError::EmptyViewStack => ("EmptyViewStack", "View stack is empty".to_string()),
            AppError::InvalidFunctionRef(key) => (
                "InvalidFunctionRef",
                format!("Invalid Lua function reference: {}", key),
            ),
            AppError::Json(e) => ("JsonError", format!("JSON error: {}", e)),
            AppError::Internal(msg) => ("Internal", format!("Internal error: {}", msg)),
        };

        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("kind", kind)?;
        state.serialize_field("message", &message)?;
        state.end()
    }
}

/// Result type alias using AppError.
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_serialization() {
        let error = AppError::PluginNotFound("test-plugin".to_string());
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("PluginNotFound"));
        assert!(json.contains("test-plugin"));
    }

    #[test]
    fn test_plugin_execution_error() {
        let error = AppError::PluginExecution {
            plugin_name: "my-plugin".to_string(),
            message: "something went wrong".to_string(),
        };
        assert!(error.to_string().contains("my-plugin"));
        assert!(error.to_string().contains("something went wrong"));
    }

    #[test]
    fn test_lua_error_conversion() {
        // Create a simple Lua error by loading invalid syntax
        let lua = mlua::Lua::new();
        let result: Result<(), mlua::Error> = lua.load("invalid syntax {{{{").exec();
        if let Err(lua_err) = result {
            let app_err: AppError = lua_err.into();
            assert!(matches!(app_err, AppError::Lua(_)));
        }
    }
}
