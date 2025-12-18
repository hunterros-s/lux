//! View stack management operations.

use parking_lot::RwLock;

use crate::plugin_api::types::{View, ViewInstance, ViewState};

/// Get the current view state for the frontend.
pub fn get_current_view_state(view_stack: &RwLock<Vec<ViewInstance>>) -> Option<ViewState> {
    let stack = view_stack.read();
    stack.last().map(ViewState::from)
}

/// Get the entire view stack state.
pub fn get_view_stack(view_stack: &RwLock<Vec<ViewInstance>>) -> Vec<ViewState> {
    let stack = view_stack.read();
    stack.iter().map(ViewState::from).collect()
}

/// Push a new view onto the stack.
pub fn push_view(
    view_stack: &RwLock<Vec<ViewInstance>>,
    view: View,
    initial_query: Option<String>,
) {
    let mut stack = view_stack.write();
    stack.push(ViewInstance::new(view, initial_query));
    tracing::debug!("Pushed view, stack depth: {}", stack.len());
}

/// Replace the current view.
pub fn replace_view(
    view_stack: &RwLock<Vec<ViewInstance>>,
    view: View,
    initial_query: Option<String>,
) {
    let mut stack = view_stack.write();
    if !stack.is_empty() {
        stack.pop();
    }
    stack.push(ViewInstance::new(view, initial_query));
    tracing::debug!("Replaced view, stack depth: {}", stack.len());
}

/// Pop the current view and return to the previous one.
pub fn pop_view(view_stack: &RwLock<Vec<ViewInstance>>) -> bool {
    let mut stack = view_stack.write();
    if stack.len() > 1 {
        stack.pop();
        tracing::debug!("Popped view, stack depth: {}", stack.len());
        true
    } else {
        tracing::debug!("Cannot pop: already at root view");
        false
    }
}

/// Get the current query from the view stack.
pub fn get_current_query(view_stack: &RwLock<Vec<ViewInstance>>) -> String {
    let stack = view_stack.read();
    stack.last().map(|v| v.query.clone()).unwrap_or_default()
}

/// Update the query for the current view.
pub fn set_current_query(view_stack: &RwLock<Vec<ViewInstance>>, query: String) {
    let mut stack = view_stack.write();
    if let Some(view) = stack.last_mut() {
        view.query = query;
    }
}
