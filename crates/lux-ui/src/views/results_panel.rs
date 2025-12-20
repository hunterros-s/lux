//! Results panel utilities.
//!
//! Helper functions for the results list.

use gpui::ScrollStrategy;
use gpui_component::VirtualListScrollHandle;

/// Scroll the results list to make the cursor visible.
///
/// Call this from the parent when cursor moves via keyboard.
pub fn scroll_to_cursor(scroll_handle: &VirtualListScrollHandle, cursor_list_index: usize) {
    scroll_handle.scroll_to_item(cursor_list_index, ScrollStrategy::Nearest);
}
