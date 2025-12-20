//! Query engine submodules.
//!
//! This module contains the decomposed implementation of the query engine,
//! organized into focused submodules for better maintainability.
//!
//! Note: Selection and cursor state is owned by the UI, not the engine.
//! View stack operations are handled by ObservableViewStack in engine/observable_view_stack.rs.

mod actions;
mod sources;
mod triggers;
pub mod types;

// Re-export types
pub use types::*;

// Re-export functions for use by engine.rs
pub(super) use actions::{execute_action, get_applicable_actions, get_default_action};
pub(super) use sources::run_current_view_source;
pub(super) use triggers::{find_matching_triggers, run_trigger};
