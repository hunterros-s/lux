//! Action-related types.

use serde::{Deserialize, Serialize};

/// Information about an available action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionInfo {
    /// Plugin that provides this action.
    pub plugin_name: String,

    /// Index of the action within the plugin.
    pub action_index: usize,

    /// Unique identifier for the action.
    pub id: String,

    /// Display text in action list.
    pub title: String,

    /// Icon identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// Result returned by action execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ActionResult {
    /// Close Lux entirely.
    Dismiss,

    /// Push a new view onto the stack.
    PushView {
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        query: Option<String>,
    },

    /// Replace current view.
    ReplaceView {
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },

    /// Pop current view, return to previous.
    Pop,

    /// Keep launcher open, continue.
    Continue,

    /// Show progress message.
    Progress { message: String },

    /// Action completed successfully.
    Complete {
        message: String,
        #[serde(default)]
        actions: Vec<FollowUpAction>,
    },

    /// Action failed.
    Fail { error: String },
}

/// A follow-up action shown after completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowUpAction {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}
