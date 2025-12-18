//! Cursor and selection state management.

use parking_lot::RwLock;

use crate::plugin_api::types::{Direction, SelectionMode, ViewInstance};

/// Move the cursor in the given direction.
pub fn move_cursor(
    view_stack: &RwLock<Vec<ViewInstance>>,
    direction: Direction,
    item_ids: &[String],
) {
    let mut stack = view_stack.write();
    if let Some(view) = stack.last_mut() {
        if item_ids.is_empty() {
            view.cursor_id = None;
            return;
        }

        let current_index = view
            .cursor_id
            .as_ref()
            .and_then(|id| item_ids.iter().position(|i| i == id));

        let new_index = match (current_index, direction) {
            (None, _) => Some(0),
            (Some(i), Direction::Down) => {
                if i + 1 < item_ids.len() {
                    Some(i + 1)
                } else {
                    Some(i)
                }
            }
            (Some(i), Direction::Up) => {
                if i > 0 {
                    Some(i - 1)
                } else {
                    Some(0)
                }
            }
        };

        view.cursor_id = new_index.map(|i| item_ids[i].clone());
    }
}

/// Get the currently focused item ID.
pub fn get_cursor_id(view_stack: &RwLock<Vec<ViewInstance>>) -> Option<String> {
    let stack = view_stack.read();
    stack.last().and_then(|v| v.cursor_id.clone())
}

/// Set the cursor to a specific item.
pub fn set_cursor(view_stack: &RwLock<Vec<ViewInstance>>, item_id: Option<String>) {
    let mut stack = view_stack.write();
    if let Some(view) = stack.last_mut() {
        view.cursor_id = item_id;
    }
}

/// Toggle selection of the item at cursor (for single/multi modes).
pub fn toggle_selection_at_cursor(view_stack: &RwLock<Vec<ViewInstance>>) {
    let mut stack = view_stack.write();
    if let Some(view) = stack.last_mut() {
        if let Some(ref cursor_id) = view.cursor_id {
            match view.view.selection {
                SelectionMode::Single => {
                    view.selected_ids.clear();
                    view.selected_ids.insert(cursor_id.clone());
                }
                SelectionMode::Multi => {
                    if view.selected_ids.contains(cursor_id) {
                        view.selected_ids.remove(cursor_id);
                    } else {
                        view.selected_ids.insert(cursor_id.clone());
                    }
                }
                SelectionMode::Custom => {
                    // Custom mode is handled by on_select hook
                }
            }
        }
    }
}

/// Get the selected item IDs.
pub fn get_selected_ids(view_stack: &RwLock<Vec<ViewInstance>>) -> Vec<String> {
    let stack = view_stack.read();
    stack
        .last()
        .map(|v| v.selected_ids.iter().cloned().collect())
        .unwrap_or_default()
}

/// Clear selection.
pub fn clear_selection(view_stack: &RwLock<Vec<ViewInstance>>) {
    let mut stack = view_stack.write();
    if let Some(view) = stack.last_mut() {
        view.selected_ids.clear();
    }
}
