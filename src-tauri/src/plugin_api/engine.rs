//! Query Engine
//!
//! The QueryEngine orchestrates the Plugin API execution flow:
//! - Trigger matching and execution
//! - Source searching and result aggregation
//! - Action filtering and execution
//! - View stack management
//!
//! ## Query Flow
//!
//! ```text
//! User types query
//!        │
//!        ▼
//! ┌──────────────────┐
//! │ Test all triggers │
//! │ (match or prefix) │
//! └────────┬─────────┘
//!          │
//!     ┌────┴────┐
//!     │         │
//!     ▼         ▼
//! Triggers    No triggers
//! matched     matched
//!     │         │
//!     ▼         ▼
//! Run all    Current view's
//! triggers   source(ctx)
//!     │         │
//!     │    ┌────┘
//!     ▼    ▼
//! Merge results
//! (add_results + source)
//!     │
//!     ▼
//! If any push() called,
//! switch to pushed view
//!     │
//!     ▼
//! Return Groups to frontend
//! ```

use std::sync::Arc;

use mlua::Lua;
use parking_lot::{Mutex, RwLock};

use super::context::{build_view_select_context, build_view_submit_context, EngineState};
use super::registry::PluginRegistry;
use super::types::{
    ActionResult, Direction, Groups, Item, KeypressResult, SelectionMode, View, ViewInstance,
    ViewState,
};

// Import submodules
mod engine_impl;

// Re-export ActionInfo from submodules
pub use engine_impl::ActionInfo;

// =============================================================================
// Query Engine
// =============================================================================

/// The QueryEngine orchestrates plugin execution.
///
/// It maintains the view stack, handles trigger/source/action execution,
/// and coordinates with the Lua runtime.
pub struct QueryEngine {
    /// Plugin registry containing all registered plugins.
    registry: Arc<PluginRegistry>,

    /// View stack (bottom = root, top = current).
    view_stack: RwLock<Vec<ViewInstance>>,

    /// Current query generation for async cancellation.
    query_generation: Mutex<u64>,
}

impl QueryEngine {
    /// Create a new QueryEngine with the given registry.
    pub fn new(registry: Arc<PluginRegistry>) -> Self {
        Self {
            registry,
            view_stack: RwLock::new(Vec::new()),
            query_generation: Mutex::new(0),
        }
    }

    /// Initialize with the root view.
    ///
    /// This should be called after plugins are loaded to set up the initial view.
    pub fn initialize(&self, lua: &Lua) {
        let mut stack = self.view_stack.write();

        // Clear any existing views
        stack.clear();

        // Create the default root view
        let root_view = self.create_default_root_view(lua);
        stack.push(ViewInstance::new(root_view, None));

        tracing::debug!("QueryEngine initialized with root view");
    }

    /// Create the default root view that aggregates all root sources.
    fn create_default_root_view(&self, _lua: &Lua) -> View {
        use super::types::LuaFunctionRef;

        // Create a placeholder source function
        // The actual implementation will call search_root_sources
        let source_key = format!("engine:root_view:source:{}", uuid::Uuid::new_v4());

        // We can't easily create a Lua function here that calls back to Rust,
        // so we'll use a special marker and handle it in search()
        View {
            title: None,
            placeholder: Some("Search...".to_string()),
            source_fn: LuaFunctionRef::new(source_key),
            selection: SelectionMode::Single,
            on_select_fn: None,
            on_submit_fn: None,
            view_data: serde_json::Value::Null,
            keys: std::collections::HashMap::new(),
        }
    }

    // =========================================================================
    // View Stack Operations
    // =========================================================================

    /// Get the current view state for the frontend.
    pub fn get_current_view_state(&self) -> Option<ViewState> {
        engine_impl::get_current_view_state(&self.view_stack)
    }

    /// Get the entire view stack state.
    pub fn get_view_stack(&self) -> Vec<ViewState> {
        engine_impl::get_view_stack(&self.view_stack)
    }

    /// Push a new view onto the stack.
    pub fn push_view(&self, view: View, initial_query: Option<String>) {
        engine_impl::push_view(&self.view_stack, view, initial_query)
    }

    /// Replace the current view.
    pub fn replace_view(&self, view: View, initial_query: Option<String>) {
        engine_impl::replace_view(&self.view_stack, view, initial_query)
    }

    /// Pop the current view and return to the previous one.
    pub fn pop_view(&self) -> bool {
        engine_impl::pop_view(&self.view_stack)
    }

    /// Get the current query from the view stack.
    pub fn get_current_query(&self) -> String {
        engine_impl::get_current_query(&self.view_stack)
    }

    /// Update the query for the current view.
    pub fn set_current_query(&self, query: String) {
        engine_impl::set_current_query(&self.view_stack, query)
    }

    // =========================================================================
    // Cursor & Selection
    // =========================================================================

    /// Move the cursor in the given direction.
    pub fn move_cursor(&self, direction: Direction, item_ids: &[String]) {
        engine_impl::move_cursor(&self.view_stack, direction, item_ids)
    }

    /// Get the currently focused item ID.
    pub fn get_cursor_id(&self) -> Option<String> {
        engine_impl::get_cursor_id(&self.view_stack)
    }

    /// Set the cursor to a specific item.
    pub fn set_cursor(&self, item_id: Option<String>) {
        engine_impl::set_cursor(&self.view_stack, item_id)
    }

    /// Toggle selection of the item at cursor (for single/multi modes).
    pub fn toggle_selection_at_cursor(&self) {
        engine_impl::toggle_selection_at_cursor(&self.view_stack)
    }

    /// Get the selected item IDs.
    pub fn get_selected_ids(&self) -> Vec<String> {
        engine_impl::get_selected_ids(&self.view_stack)
    }

    /// Clear selection.
    pub fn clear_selection(&self) {
        engine_impl::clear_selection(&self.view_stack)
    }

    // =========================================================================
    // Search Flow
    // =========================================================================

    /// Execute a search query.
    ///
    /// This is the main entry point for the query flow:
    /// 1. Increment query generation (for async cancellation)
    /// 2. Test all triggers
    /// 3. If triggers match, run them and collect results
    /// 4. If no triggers or no push, run current view's source
    /// 5. Handle any view push/replace from triggers
    /// 6. Return merged results
    pub fn search(&self, lua: &Lua, query: &str) -> Result<Groups, String> {
        // Increment generation for async cancellation
        {
            let mut gen = self.query_generation.lock();
            *gen += 1;
        }

        // Update current view's query
        self.set_current_query(query.to_string());

        let mut all_results = Groups::new();
        let mut view_pushed = false;

        // Step 1: Find and run matching triggers
        let matching_triggers = engine_impl::find_matching_triggers(&self.registry, lua, query)?;

        for (plugin_name, trigger_index) in matching_triggers {
            let trigger_results =
                engine_impl::run_trigger(&self.registry, lua, &plugin_name, trigger_index, query)?;

            // Collect added results
            all_results.extend(trigger_results.added_results);

            // Handle view push/replace
            if let Some(pushed) = trigger_results.pushed_view {
                if pushed.replace {
                    self.replace_view(pushed.view, pushed.initial_query);
                } else {
                    self.push_view(pushed.view, pushed.initial_query);
                }
                view_pushed = true;
            }

            // Handle dismiss
            if trigger_results.dismissed {
                // Signal to frontend to dismiss
                return Ok(all_results);
            }
        }

        // Step 2: If no view was pushed, run current view's source
        if !view_pushed {
            let source_results =
                engine_impl::run_current_view_source(&self.registry, &self.view_stack, lua, query)?;
            all_results.extend(source_results);
        }

        Ok(all_results)
    }

    // =========================================================================
    // Action Flow
    // =========================================================================

    /// Get actions that apply to the given items.
    pub fn get_applicable_actions(
        &self,
        lua: &Lua,
        items: &[Item],
    ) -> Result<Vec<ActionInfo>, String> {
        engine_impl::get_applicable_actions(&self.registry, lua, items)
    }

    /// Get the default action for the given items (first applicable).
    pub fn get_default_action(
        &self,
        lua: &Lua,
        items: &[Item],
    ) -> Result<Option<ActionInfo>, String> {
        engine_impl::get_default_action(&self.registry, lua, items)
    }

    /// Execute an action on the given items.
    pub fn execute_action(
        &self,
        lua: &Lua,
        plugin_name: &str,
        action_index: usize,
        items: &[Item],
    ) -> Result<ActionResult, String> {
        engine_impl::execute_action(
            &self.registry,
            &self.view_stack,
            lua,
            plugin_name,
            action_index,
            items,
        )
    }

    // =========================================================================
    // Keypress Handling
    // =========================================================================

    /// Handle a keypress, checking view-specific bindings.
    pub fn handle_keypress(
        &self,
        lua: &Lua,
        key: &str,
        items: &[Item],
    ) -> Result<KeypressResult, String> {
        engine_impl::handle_keypress(&self.registry, &self.view_stack, lua, key, items)
    }

    // =========================================================================
    // Selection Hook (Custom Mode)
    // =========================================================================

    /// Handle selection in custom mode by calling on_select hook.
    pub fn handle_custom_select(&self, lua: &Lua, item: &Item) -> Result<(), String> {
        let (on_select_key, view_data, current_selection) = {
            let stack = self.view_stack.read();
            match stack.last() {
                Some(view) => {
                    let key = view.view.on_select_fn.as_ref().map(|f| f.key.clone());
                    (key, view.view.view_data.clone(), view.selected_ids.clone())
                }
                None => return Ok(()),
            }
        };

        let on_select_key = match on_select_key {
            Some(k) => k,
            None => return Ok(()), // No custom handler
        };

        let state = Arc::new(Mutex::new(EngineState::new()));
        let ctx = build_view_select_context(
            lua,
            item,
            &view_data,
            &current_selection,
            Arc::clone(&state),
        )
        .map_err(|e| format!("Failed to build select context: {}", e))?;

        // Call the on_select function
        let registry_key = lua
            .named_registry_value::<mlua::RegistryKey>(&on_select_key)
            .map_err(|e| format!("on_select function not found: {}", e))?;
        let func: mlua::Function = lua
            .registry_value(&registry_key)
            .map_err(|e| format!("Failed to get on_select function: {}", e))?;
        func.call::<()>(ctx)
            .map_err(|e| format!("on_select failed: {}", e))?;

        // Apply selection changes
        let state = state.lock();
        let mut stack = self.view_stack.write();
        if let Some(view) = stack.last_mut() {
            if state.selection_changes.cleared {
                view.selected_ids.clear();
            }
            for id in &state.selection_changes.deselected {
                view.selected_ids.remove(id);
            }
            for id in &state.selection_changes.selected {
                view.selected_ids.insert(id.clone());
            }
        }

        Ok(())
    }

    // =========================================================================
    // Submit Hook
    // =========================================================================

    /// Handle form submission by calling on_submit hook.
    pub fn handle_submit(&self, lua: &Lua) -> Result<bool, String> {
        let (on_submit_key, view_data, query) = {
            let stack = self.view_stack.read();
            match stack.last() {
                Some(view) => {
                    let key = view.view.on_submit_fn.as_ref().map(|f| f.key.clone());
                    (key, view.view.view_data.clone(), view.query.clone())
                }
                None => return Ok(false),
            }
        };

        let on_submit_key = match on_submit_key {
            Some(k) => k,
            None => return Ok(false), // No submit handler
        };

        let state = Arc::new(Mutex::new(EngineState::new()));
        let ctx = build_view_submit_context(lua, &query, &view_data, Arc::clone(&state))
            .map_err(|e| format!("Failed to build submit context: {}", e))?;

        // Call the on_submit function
        let registry_key = lua
            .named_registry_value::<mlua::RegistryKey>(&on_submit_key)
            .map_err(|e| format!("on_submit function not found: {}", e))?;
        let func: mlua::Function = lua
            .registry_value(&registry_key)
            .map_err(|e| format!("Failed to get on_submit function: {}", e))?;
        func.call::<()>(ctx)
            .map_err(|e| format!("on_submit failed: {}", e))?;

        // Process state changes
        let state = match Arc::try_unwrap(state) {
            Ok(mutex) => mutex.into_inner(),
            Err(arc) => arc.lock().clone(),
        };

        if let Some(pushed) = state.pushed_view {
            if pushed.replace {
                self.replace_view(pushed.view, pushed.initial_query);
            } else {
                self.push_view(pushed.view, pushed.initial_query);
            }
        }

        if state.popped {
            self.pop_view();
        }

        Ok(state.dismissed)
    }
}

// Implement Clone for EngineState (needed for Arc::try_unwrap fallback)
impl Clone for EngineState {
    fn clone(&self) -> Self {
        Self {
            added_results: self.added_results.clone(),
            pushed_view: None, // Can't clone View easily
            dismissed: self.dismissed,
            popped: self.popped,
            progress_message: self.progress_message.clone(),
            completion: self.completion.clone(),
            error: self.error.clone(),
            loading: self.loading,
            resolved_results: self.resolved_results.clone(),
            selection_changes: super::context::SelectionChanges {
                selected: self.selection_changes.selected.clone(),
                deselected: self.selection_changes.deselected.clone(),
                cleared: self.selection_changes.cleared,
            },
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_new() {
        let registry = Arc::new(PluginRegistry::new());
        let engine = QueryEngine::new(registry);

        assert!(engine.get_current_view_state().is_none());
        assert!(engine.get_view_stack().is_empty());
    }

    #[test]
    fn test_view_stack_operations() {
        let registry = Arc::new(PluginRegistry::new());
        let engine = QueryEngine::new(registry);

        // Create a test view
        let view1 = View {
            title: Some("View 1".to_string()),
            placeholder: None,
            source_fn: super::super::types::LuaFunctionRef::new("test:source:1".to_string()),
            selection: SelectionMode::Single,
            on_select_fn: None,
            on_submit_fn: None,
            view_data: serde_json::Value::Null,
            keys: std::collections::HashMap::new(),
        };

        let view2 = View {
            title: Some("View 2".to_string()),
            placeholder: None,
            source_fn: super::super::types::LuaFunctionRef::new("test:source:2".to_string()),
            selection: SelectionMode::Multi,
            on_select_fn: None,
            on_submit_fn: None,
            view_data: serde_json::Value::Null,
            keys: std::collections::HashMap::new(),
        };

        // Push views
        engine.push_view(view1, None);
        assert_eq!(engine.get_view_stack().len(), 1);

        engine.push_view(view2, Some("initial query".to_string()));
        assert_eq!(engine.get_view_stack().len(), 2);

        // Check current view
        let current = engine.get_current_view_state().unwrap();
        assert_eq!(current.title, Some("View 2".to_string()));
        assert_eq!(current.query, "initial query");

        // Pop view
        assert!(engine.pop_view());
        assert_eq!(engine.get_view_stack().len(), 1);

        let current = engine.get_current_view_state().unwrap();
        assert_eq!(current.title, Some("View 1".to_string()));

        // Can't pop last view
        assert!(!engine.pop_view());
        assert_eq!(engine.get_view_stack().len(), 1);
    }

    #[test]
    fn test_cursor_movement() {
        let registry = Arc::new(PluginRegistry::new());
        let engine = QueryEngine::new(registry);

        let view = View {
            title: None,
            placeholder: None,
            source_fn: super::super::types::LuaFunctionRef::new("test:source".to_string()),
            selection: SelectionMode::Single,
            on_select_fn: None,
            on_submit_fn: None,
            view_data: serde_json::Value::Null,
            keys: std::collections::HashMap::new(),
        };

        engine.push_view(view, None);

        let item_ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];

        // Initial cursor should be None
        assert!(engine.get_cursor_id().is_none());

        // Move down should go to first item
        engine.move_cursor(Direction::Down, &item_ids);
        assert_eq!(engine.get_cursor_id(), Some("a".to_string()));

        // Move down again
        engine.move_cursor(Direction::Down, &item_ids);
        assert_eq!(engine.get_cursor_id(), Some("b".to_string()));

        // Move up
        engine.move_cursor(Direction::Up, &item_ids);
        assert_eq!(engine.get_cursor_id(), Some("a".to_string()));

        // Move up at top should stay at top
        engine.move_cursor(Direction::Up, &item_ids);
        assert_eq!(engine.get_cursor_id(), Some("a".to_string()));
    }

    #[test]
    fn test_selection() {
        let registry = Arc::new(PluginRegistry::new());
        let engine = QueryEngine::new(registry);

        let view = View {
            title: None,
            placeholder: None,
            source_fn: super::super::types::LuaFunctionRef::new("test:source".to_string()),
            selection: SelectionMode::Multi,
            on_select_fn: None,
            on_submit_fn: None,
            view_data: serde_json::Value::Null,
            keys: std::collections::HashMap::new(),
        };

        engine.push_view(view, None);

        // Set cursor and select
        engine.set_cursor(Some("item1".to_string()));
        engine.toggle_selection_at_cursor();
        assert!(engine.get_selected_ids().contains(&"item1".to_string()));

        // Select another
        engine.set_cursor(Some("item2".to_string()));
        engine.toggle_selection_at_cursor();
        assert_eq!(engine.get_selected_ids().len(), 2);

        // Deselect first
        engine.set_cursor(Some("item1".to_string()));
        engine.toggle_selection_at_cursor();
        assert_eq!(engine.get_selected_ids().len(), 1);
        assert!(!engine.get_selected_ids().contains(&"item1".to_string()));

        // Clear
        engine.clear_selection();
        assert!(engine.get_selected_ids().is_empty());
    }
}
