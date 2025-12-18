//! Query engine submodules.
//!
//! This module contains the decomposed implementation of the query engine,
//! organized into focused submodules for better maintainability.

mod actions;
mod selection;
mod sources;
mod triggers;
pub mod types;
pub(crate) mod view_stack;

// Re-export types
pub use types::*;

// Re-export functions for use by engine.rs
pub(super) use actions::{
    execute_action, get_applicable_actions, get_default_action, handle_keypress, KeypressEffects,
};
pub(super) use selection::{
    clear_selection, get_cursor_id, get_selected_ids, move_cursor, set_cursor,
    toggle_selection_at_cursor,
};
pub(super) use sources::run_current_view_source;
pub(super) use triggers::{find_matching_triggers, run_trigger};
pub(super) use view_stack::{
    get_current_query, get_current_view_state, get_view_stack, pop_view, push_view, replace_view,
    set_current_query,
};
