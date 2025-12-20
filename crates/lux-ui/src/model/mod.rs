//! State model for the Lux launcher UI.
//!
//! This module contains the state machine and data structures that drive the UI.
//! All types are GPUI-independent for testability.

mod state;

pub use state::{
    ActionMenuItem, ActionMenuState, ActiveState, ExecutionFeedback, LauncherPhase, ListEntry,
    ViewFrame, ViewId, ViewStack,
};
