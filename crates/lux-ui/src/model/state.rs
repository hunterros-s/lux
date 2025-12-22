//! UI state model for the Lux launcher.
//!
//! This module defines the state machine and data structures for the UI layer.
//! These types are GPUI-independent to enable testing and clear separation.

use lux_core::{Group, Item, ItemId, SelectionMode};
use std::collections::{HashMap, HashSet};

// =============================================================================
// Launcher Phase State Machine
// =============================================================================

/// Top-level state machine. Invalid states are impossible.
#[derive(Debug, Default)]
pub enum LauncherPhase {
    /// Launcher is hidden, no UI visible.
    #[default]
    Hidden,
    /// Launcher is active with full state.
    Active(ActiveState),
}

impl LauncherPhase {
    /// Get the active state if the launcher is active.
    pub fn active(&self) -> Option<&ActiveState> {
        match self {
            LauncherPhase::Active(state) => Some(state),
            LauncherPhase::Hidden => None,
        }
    }

    /// Get mutable active state if the launcher is active.
    pub fn active_mut(&mut self) -> Option<&mut ActiveState> {
        match self {
            LauncherPhase::Active(state) => Some(state),
            LauncherPhase::Hidden => None,
        }
    }

    /// Get the current view frame if active.
    pub fn current_frame(&self) -> Option<&ViewFrame> {
        self.active().and_then(|s| s.view_stack.current())
    }

    /// Get mutable current view frame if active.
    pub fn current_frame_mut(&mut self) -> Option<&mut ViewFrame> {
        self.active_mut().and_then(|s| s.view_stack.current_mut())
    }

    /// Check if the launcher is active.
    pub fn is_active(&self) -> bool {
        matches!(self, LauncherPhase::Active(_))
    }
}

// =============================================================================
// Active State
// =============================================================================

/// State when the launcher is visible and interactive.
#[derive(Debug)]
pub struct ActiveState {
    /// Stack of views with full state preservation.
    pub view_stack: ViewStack,

    /// Action menu state when open (Tab pressed).
    pub action_menu: Option<ActionMenuState>,

    /// Execution feedback for long-running actions.
    pub execution: Option<ExecutionFeedback>,
}

impl Default for ActiveState {
    fn default() -> Self {
        Self {
            view_stack: ViewStack::new_root(),
            action_menu: None,
            execution: None,
        }
    }
}

// =============================================================================
// Action Menu State
// =============================================================================

/// State for the action menu overlay.
#[derive(Debug)]
pub struct ActionMenuState {
    /// Available actions for current selection.
    pub actions: Vec<ActionMenuItem>,

    /// Currently highlighted action index.
    pub cursor_index: usize,
}

impl ActionMenuState {
    /// Create a new action menu.
    pub fn new(actions: Vec<ActionMenuItem>) -> Self {
        Self {
            actions,
            cursor_index: 0,
        }
    }

    /// Move cursor up.
    pub fn cursor_up(&mut self) {
        if self.cursor_index > 0 {
            self.cursor_index -= 1;
        }
    }

    /// Move cursor down.
    pub fn cursor_down(&mut self) {
        if self.cursor_index + 1 < self.actions.len() {
            self.cursor_index += 1;
        }
    }

    /// Get the selected action.
    pub fn selected_action(&self) -> Option<&ActionMenuItem> {
        self.actions.get(self.cursor_index)
    }
}

/// An action in the menu.
#[derive(Debug, Clone)]
pub struct ActionMenuItem {
    /// View that provides this action.
    pub view_id: String,

    /// Action ID within the view.
    pub action_id: String,

    /// Lua registry key for the handler function.
    pub handler_key: Option<String>,

    /// Display title.
    pub title: String,

    /// Optional icon.
    pub icon: Option<String>,
}

// =============================================================================
// Execution Feedback
// =============================================================================

/// Feedback displayed during/after action execution.
#[derive(Debug, Clone)]
pub enum ExecutionFeedback {
    /// Action is in progress.
    Progress { message: String },

    /// Action completed successfully.
    Complete { message: String },

    /// Action failed.
    Failed { error: String },
}

// =============================================================================
// View Stack
// =============================================================================

/// Stack of view frames with full state preservation.
///
/// The root view is never popped. Each push preserves the current frame's
/// full state (query, cursor, selection, scroll) so pop restores exactly
/// where the user was.
#[derive(Debug)]
pub struct ViewStack {
    frames: Vec<ViewFrame>,
}

impl ViewStack {
    /// Create a new view stack with a root frame.
    pub fn new_root() -> Self {
        Self {
            frames: vec![ViewFrame::root()],
        }
    }

    /// Get the current (top) frame.
    pub fn current(&self) -> Option<&ViewFrame> {
        self.frames.last()
    }

    /// Get mutable current frame.
    pub fn current_mut(&mut self) -> Option<&mut ViewFrame> {
        self.frames.last_mut()
    }

    /// Push a new frame onto the stack.
    pub fn push(&mut self, frame: ViewFrame) {
        self.frames.push(frame);
    }

    /// Pop the top frame. Never pops the root.
    pub fn pop(&mut self) -> Option<ViewFrame> {
        if self.frames.len() > 1 {
            self.frames.pop()
        } else {
            None
        }
    }

    /// Replace the current frame (for replace_view effect).
    pub fn replace(&mut self, frame: ViewFrame) {
        if let Some(current) = self.frames.last_mut() {
            *current = frame;
        }
    }

    /// Get the depth of the stack.
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Check if we're at the root view.
    pub fn is_root(&self) -> bool {
        self.frames.len() == 1
    }

    /// Get breadcrumb titles for navigation display.
    pub fn breadcrumbs(&self) -> impl Iterator<Item = Option<&str>> {
        self.frames.iter().map(|f| f.title.as_deref())
    }
}

// =============================================================================
// View Frame
// =============================================================================

/// Unique identifier for a view frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewId(pub u64);

impl ViewId {
    /// Generate a new unique view ID.
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for ViewId {
    fn default() -> Self {
        Self::new()
    }
}

/// A single frame in the view stack with full UI state.
///
/// Each frame preserves all state when pushed, enabling perfect restoration
/// when popped. This includes query, cursor position, selection, and scroll.
#[derive(Debug)]
pub struct ViewFrame {
    /// Unique identifier for this frame.
    pub view_id: ViewId,

    // -------------------------------------------------------------------------
    // Search State
    // -------------------------------------------------------------------------
    /// Current search query.
    pub query: String,

    /// Groups returned by the source.
    pub groups: Vec<Group>,

    /// Flattened entries for rendering (cached).
    pub flat_entries: Vec<ListEntry>,

    /// Item IDs in display order (for cursor navigation).
    pub item_ids: Vec<ItemId>,

    /// Map from ID to Item for quick lookup.
    pub item_map: HashMap<ItemId, Item>,

    /// Whether a search is in progress.
    pub loading: bool,

    /// Generation counter for async cancellation.
    pub generation: u64,

    // -------------------------------------------------------------------------
    // Selection State
    // -------------------------------------------------------------------------
    /// Index into item_ids of the cursor position.
    pub cursor_index: usize,

    /// Selection mode for this view.
    pub selection_mode: SelectionMode,

    /// Selected item IDs.
    pub selected_ids: HashSet<ItemId>,

    // -------------------------------------------------------------------------
    // UI State
    // -------------------------------------------------------------------------
    /// Placeholder text for the search input.
    pub placeholder: String,

    /// Title shown in breadcrumbs/header.
    pub title: Option<String>,

    /// Scroll position to restore.
    pub scroll_position: f32,
}

impl ViewFrame {
    /// Create a root frame for the initial view.
    pub fn root() -> Self {
        Self {
            view_id: ViewId::new(),
            query: String::new(),
            groups: Vec::new(),
            flat_entries: Vec::new(),
            item_ids: Vec::new(),
            item_map: HashMap::new(),
            loading: false,
            generation: 0,
            cursor_index: 0,
            selection_mode: SelectionMode::Single,
            selected_ids: HashSet::new(),
            placeholder: "Search...".to_string(),
            title: None,
            scroll_position: 0.0,
        }
    }

    /// Create a new frame for a pushed view.
    pub fn new_push(
        placeholder: impl Into<String>,
        title: Option<String>,
        selection_mode: SelectionMode,
    ) -> Self {
        Self {
            view_id: ViewId::new(),
            query: String::new(),
            groups: Vec::new(),
            flat_entries: Vec::new(),
            item_ids: Vec::new(),
            item_map: HashMap::new(),
            loading: false,
            generation: 0,
            cursor_index: 0,
            selection_mode,
            selected_ids: HashSet::new(),
            placeholder: placeholder.into(),
            title,
            scroll_position: 0.0,
        }
    }

    /// Update groups and rebuild cached indices.
    pub fn set_groups(&mut self, groups: Vec<Group>) {
        self.groups = groups;
        self.rebuild_indices();
        self.clamp_cursor();
    }

    /// Rebuild flat_entries, item_ids, and item_map from groups.
    fn rebuild_indices(&mut self) {
        self.flat_entries = self.flatten_to_entries();
        self.item_ids.clear();
        self.item_map.clear();

        for group in &self.groups {
            for item in &group.items {
                let id = item.item_id();
                self.item_ids.push(id.clone());
                self.item_map.insert(id, item.clone());
            }
        }
    }

    /// Flatten groups into a list of entries for rendering.
    fn flatten_to_entries(&self) -> Vec<ListEntry> {
        let mut entries = Vec::new();
        let mut flat_index = 0;

        for group in &self.groups {
            // Add group header if it has a title
            if let Some(title) = &group.title {
                entries.push(ListEntry::GroupHeader {
                    title: title.clone(),
                });
            }

            // Add items
            for item in &group.items {
                entries.push(ListEntry::Item {
                    item: item.clone(),
                    flat_index,
                });
                flat_index += 1;
            }
        }

        entries
    }

    /// Clamp cursor to valid range.
    fn clamp_cursor(&mut self) {
        if self.cursor_index >= self.item_ids.len() {
            self.cursor_index = self.item_ids.len().saturating_sub(1);
        }
    }

    /// Get the item at the cursor position.
    pub fn cursor_item(&self) -> Option<&Item> {
        self.item_ids
            .get(self.cursor_index)
            .and_then(|id| self.item_map.get(id))
    }

    /// Get the cursor item's ID.
    pub fn cursor_id(&self) -> Option<&ItemId> {
        self.item_ids.get(self.cursor_index)
    }

    /// Move cursor up.
    pub fn cursor_up(&mut self) {
        if self.cursor_index > 0 {
            self.cursor_index -= 1;
        }
    }

    /// Move cursor down.
    pub fn cursor_down(&mut self) {
        if self.cursor_index + 1 < self.item_ids.len() {
            self.cursor_index += 1;
        }
    }

    /// Get the number of items.
    pub fn item_count(&self) -> usize {
        self.item_ids.len()
    }

    /// Check if the frame has any items.
    pub fn has_items(&self) -> bool {
        !self.item_ids.is_empty()
    }

    /// Toggle selection at cursor (for multi-select mode).
    pub fn toggle_selection_at_cursor(&mut self) {
        if let Some(id) = self.cursor_id().cloned() {
            if self.selected_ids.contains(&id) {
                self.selected_ids.remove(&id);
            } else {
                self.selected_ids.insert(id);
            }
        }
    }

    /// Get selected items.
    pub fn selected_items(&self) -> Vec<&Item> {
        self.selected_ids
            .iter()
            .filter_map(|id| self.item_map.get(id))
            .collect()
    }

    /// Clear selection.
    pub fn clear_selection(&mut self) {
        self.selected_ids.clear();
    }

    /// Convert cursor index to list entry index (accounting for headers).
    pub fn cursor_to_list_index(&self) -> usize {
        // Walk through entries to find the matching item
        for (i, entry) in self.flat_entries.iter().enumerate() {
            if let ListEntry::Item { flat_index, .. } = entry {
                if *flat_index == self.cursor_index {
                    return i;
                }
            }
        }
        0
    }
}

// =============================================================================
// List Entry
// =============================================================================

/// An entry in the flattened list for rendering.
#[derive(Debug, Clone)]
pub enum ListEntry {
    /// A group header row.
    GroupHeader { title: String },

    /// An item row.
    Item {
        item: Item,
        /// Index into the flat item list (for cursor matching).
        flat_index: usize,
    },
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_item(id: &str, title: &str) -> Item {
        Item::new(id, title)
    }

    fn test_groups() -> Vec<Group> {
        vec![
            Group::new(
                "Recent",
                vec![test_item("1", "Item 1"), test_item("2", "Item 2")],
            ),
            Group::new("All", vec![test_item("3", "Item 3")]),
        ]
    }

    #[test]
    fn test_launcher_phase_default() {
        let phase = LauncherPhase::default();
        assert!(!phase.is_active());
        assert!(phase.current_frame().is_none());
    }

    #[test]
    fn test_launcher_phase_active() {
        let phase = LauncherPhase::Active(ActiveState::default());
        assert!(phase.is_active());
        assert!(phase.current_frame().is_some());
    }

    #[test]
    fn test_view_stack_root() {
        let stack = ViewStack::new_root();
        assert!(stack.is_root());
        assert_eq!(stack.depth(), 1);
        assert!(stack.current().is_some());
    }

    #[test]
    fn test_view_stack_push_pop() {
        let mut stack = ViewStack::new_root();
        assert!(stack.is_root());

        stack.push(ViewFrame::new_push(
            "Search files",
            Some("Files".to_string()),
            SelectionMode::Single,
        ));
        assert!(!stack.is_root());
        assert_eq!(stack.depth(), 2);

        let popped = stack.pop();
        assert!(popped.is_some());
        assert!(stack.is_root());
        assert_eq!(stack.depth(), 1);

        // Can't pop root
        let popped = stack.pop();
        assert!(popped.is_none());
        assert!(stack.is_root());
    }

    #[test]
    fn test_view_frame_set_groups() {
        let mut frame = ViewFrame::root();
        assert!(!frame.has_items());

        frame.set_groups(test_groups());
        assert!(frame.has_items());
        assert_eq!(frame.item_count(), 3);

        // Check flat entries include headers
        assert_eq!(frame.flat_entries.len(), 5); // 2 headers + 3 items
    }

    #[test]
    fn test_view_frame_cursor_navigation() {
        let mut frame = ViewFrame::root();
        frame.set_groups(test_groups());

        assert_eq!(frame.cursor_index, 0);
        assert_eq!(frame.cursor_item().unwrap().id, "1");

        frame.cursor_down();
        assert_eq!(frame.cursor_index, 1);
        assert_eq!(frame.cursor_item().unwrap().id, "2");

        frame.cursor_down();
        assert_eq!(frame.cursor_index, 2);
        assert_eq!(frame.cursor_item().unwrap().id, "3");

        // Can't go past end
        frame.cursor_down();
        assert_eq!(frame.cursor_index, 2);

        frame.cursor_up();
        assert_eq!(frame.cursor_index, 1);

        // Go all the way up
        frame.cursor_up();
        frame.cursor_up();
        assert_eq!(frame.cursor_index, 0);
    }

    #[test]
    fn test_view_frame_selection() {
        let mut frame = ViewFrame::root();
        frame.set_groups(test_groups());

        assert!(frame.selected_ids.is_empty());

        frame.toggle_selection_at_cursor();
        assert_eq!(frame.selected_ids.len(), 1);

        frame.cursor_down();
        frame.toggle_selection_at_cursor();
        assert_eq!(frame.selected_ids.len(), 2);

        // Toggle off
        frame.cursor_up();
        frame.toggle_selection_at_cursor();
        assert_eq!(frame.selected_ids.len(), 1);

        frame.clear_selection();
        assert!(frame.selected_ids.is_empty());
    }

    #[test]
    fn test_action_menu_navigation() {
        let actions = vec![
            ActionMenuItem {
                view_id: "test".to_string(),
                action_id: "open".to_string(),
                handler_key: None,
                title: "Open".to_string(),
                icon: None,
            },
            ActionMenuItem {
                view_id: "test".to_string(),
                action_id: "delete".to_string(),
                handler_key: None,
                title: "Delete".to_string(),
                icon: None,
            },
        ];

        let mut menu = ActionMenuState::new(actions);
        assert_eq!(menu.cursor_index, 0);
        assert_eq!(menu.selected_action().unwrap().title, "Open");

        menu.cursor_down();
        assert_eq!(menu.cursor_index, 1);
        assert_eq!(menu.selected_action().unwrap().title, "Delete");

        // Can't go past end
        menu.cursor_down();
        assert_eq!(menu.cursor_index, 1);

        menu.cursor_up();
        assert_eq!(menu.cursor_index, 0);
    }

    #[test]
    fn test_breadcrumbs() {
        let mut stack = ViewStack::new_root();

        let crumbs: Vec<_> = stack.breadcrumbs().collect();
        assert_eq!(crumbs.len(), 1);
        assert_eq!(crumbs[0], None); // Root has no title

        stack.push(ViewFrame::new_push(
            "",
            Some("Files".to_string()),
            SelectionMode::Single,
        ));
        stack.push(ViewFrame::new_push(
            "",
            Some("Recent".to_string()),
            SelectionMode::Single,
        ));

        let crumbs: Vec<_> = stack.breadcrumbs().collect();
        assert_eq!(crumbs.len(), 3);
        assert_eq!(crumbs[1], Some("Files"));
        assert_eq!(crumbs[2], Some("Recent"));
    }
}
