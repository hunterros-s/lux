//! Shared types for the query engine.
//!
//! This module contains types that are used across multiple engine submodules
//! to prevent circular dependencies.

/// Information about an applicable action.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActionInfo {
    /// View ID that provides this action.
    pub view_id: String,
    /// Action ID within the view.
    pub id: String,
    /// Display title.
    pub title: String,
    /// Optional icon.
    pub icon: Option<String>,
    /// Whether this action supports bulk selection.
    pub bulk: bool,
    /// Lua registry key for the action handler function.
    pub handler_key: Option<String>,
}
