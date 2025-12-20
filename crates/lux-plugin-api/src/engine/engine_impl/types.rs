//! Shared types for the query engine.
//!
//! This module contains types that are used across multiple engine submodules
//! to prevent circular dependencies.

/// Information about an applicable action.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActionInfo {
    pub plugin_name: String,
    pub action_index: usize,
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub bulk: bool,
}
