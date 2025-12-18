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
            // Run trigger and get effects
            let effects =
                engine_impl::run_trigger(&self.registry, lua, &plugin_name, trigger_index, query)?;

            // Apply effects and get result
            let result = self.apply_effects(lua, effects);

            // Collect groups from SetGroups effects
            if let Some(groups) = result.groups {
                all_results.extend(groups);
            }

            // Check if a view was pushed (stack grew)
            let stack_len = self.view_stack.read().len();
            if stack_len > 1 {
                view_pushed = true;
            }

            // Handle dismiss
            if result.dismissed {
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
        // Get effects from the action
        let effects = engine_impl::execute_action(
            &self.registry,
            &self.view_stack,
            lua,
            plugin_name,
            action_index,
            items,
        )?;

        // Apply effects and convert to ActionResult
        let result = self.apply_effects(lua, effects);
        Ok(self.apply_result_to_action_result(result))
    }

    /// Convert ApplyResult to ActionResult.
    fn apply_result_to_action_result(&self, result: ApplyResult) -> ActionResult {
        // Check view stack to see if a view was pushed
        let stack_len = self.view_stack.read().len();

        if result.dismissed {
            return ActionResult::Dismiss;
        }

        if result.popped {
            return ActionResult::Pop;
        }

        if let Some(error) = result.error {
            return ActionResult::Fail { error };
        }

        if let Some(message) = result.completed {
            return ActionResult::Complete {
                message,
                actions: Vec::new(),
            };
        }

        if let Some(message) = result.progress {
            return ActionResult::Progress { message };
        }

        // If stack grew, a view was pushed
        if stack_len > 1 {
            return ActionResult::PushView {
                title: None,
                query: None,
            };
        }

        ActionResult::Continue
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
        match engine_impl::handle_keypress(&self.registry, &self.view_stack, lua, key, items)? {
            engine_impl::KeypressEffects::Handled(effects) => {
                // Apply effects (view push/pop, dismiss, etc.)
                self.apply_effects(lua, effects);
                Ok(KeypressResult::Handled)
            }
            engine_impl::KeypressEffects::NotHandled => Ok(KeypressResult::NotHandled),
        }
    }

    // =========================================================================
    // Selection Hook (Custom Mode)
    // =========================================================================

    /// Handle selection in custom mode by calling on_select hook.
    ///
    /// Uses effect-based execution: the callback collects effects,
    /// which are applied via `apply_effects()`.
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

        // Call via the bridge, which uses effect-based execution
        let effects = super::lua::call_view_on_select(
            lua,
            &on_select_key,
            item,
            &view_data,
            &current_selection,
        )
        .map_err(|e| format!("on_select failed: {}", e))?;

        // Apply effects (selection changes are handled in apply_effects)
        self.apply_effects(lua, effects);

        Ok(())
    }

    // =========================================================================
    // Submit Hook
    // =========================================================================

    /// Handle form submission by calling on_submit hook.
    ///
    /// Uses effect-based execution: the callback collects effects,
    /// which are applied via `apply_effects()`.
    ///
    /// Returns true if dismiss was called.
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

        // Call via the bridge, which uses effect-based execution
        let effects = super::lua::call_view_on_submit(lua, &on_submit_key, &query, &view_data)
            .map_err(|e| format!("on_submit failed: {}", e))?;

        // Apply effects and return whether dismiss was called
        let result = self.apply_effects(lua, effects);
        Ok(result.dismissed)
    }

    // =========================================================================
    // Effect-Based Execution (New)
    // =========================================================================

    /// Apply collected effects to the engine state.
    ///
    /// This is the single point of mutation for effect-based execution.
    /// Lua callbacks collect effects, then the engine applies them here.
    ///
    /// Returns information about what happened for the caller to act on.
    pub fn apply_effects(&self, lua: &Lua, effects: Vec<super::effect::Effect>) -> ApplyResult {
        use super::effect::Effect;
        use super::lua::cleanup_view_registry_keys;

        let mut result = ApplyResult::default();

        for effect in effects {
            match effect {
                Effect::SetGroups(groups) => {
                    result.groups = Some(groups);
                }
                Effect::PushView(spec) => {
                    let view = self.view_from_spec(&spec);
                    let registry_keys = spec.registry_keys.clone();

                    let mut stack = self.view_stack.write();
                    stack.push(ViewInstance::with_registry_keys(view, None, registry_keys));
                    tracing::debug!("Applied PushView, stack depth: {}", stack.len());
                }
                Effect::ReplaceView(spec) => {
                    let view = self.view_from_spec(&spec);
                    let registry_keys = spec.registry_keys.clone();

                    let mut stack = self.view_stack.write();

                    // Pop and cleanup the old view
                    if let Some(old_view) = stack.pop() {
                        cleanup_view_registry_keys(lua, &old_view.registry_keys);
                    }

                    stack.push(ViewInstance::with_registry_keys(view, None, registry_keys));
                    tracing::debug!("Applied ReplaceView, stack depth: {}", stack.len());
                }
                Effect::Pop => {
                    let mut stack = self.view_stack.write();
                    if stack.len() > 1 {
                        if let Some(old_view) = stack.pop() {
                            cleanup_view_registry_keys(lua, &old_view.registry_keys);
                        }
                        tracing::debug!("Applied Pop, stack depth: {}", stack.len());
                    }
                    result.popped = true;
                }
                Effect::Dismiss => {
                    result.dismissed = true;
                    tracing::debug!("Applied Dismiss");
                }
                Effect::Progress(message) => {
                    result.progress = Some(message);
                }
                Effect::Complete { message } => {
                    result.completed = Some(message);
                }
                Effect::Fail { error } => {
                    result.error = Some(error);
                }
                Effect::Select(ids) => {
                    let mut stack = self.view_stack.write();
                    if let Some(view) = stack.last_mut() {
                        for id in ids {
                            view.selected_ids.insert(id);
                        }
                    }
                }
                Effect::Deselect(ids) => {
                    let mut stack = self.view_stack.write();
                    if let Some(view) = stack.last_mut() {
                        for id in ids {
                            view.selected_ids.remove(&id);
                        }
                    }
                }
                Effect::ClearSelection => {
                    let mut stack = self.view_stack.write();
                    if let Some(view) = stack.last_mut() {
                        view.selected_ids.clear();
                    }
                }
            }
        }

        result
    }

    /// Convert a ViewSpec to a View.
    fn view_from_spec(&self, spec: &super::effect::ViewSpec) -> View {
        use super::types::LuaFunctionRef;

        let selection = match spec.selection_mode {
            super::effect::SelectionMode::Single => SelectionMode::Single,
            super::effect::SelectionMode::Multi => SelectionMode::Multi,
            super::effect::SelectionMode::Custom => SelectionMode::Custom,
        };

        View {
            title: spec.title.clone(),
            placeholder: spec.placeholder.clone(),
            source_fn: LuaFunctionRef::new(spec.source_fn_key.clone()),
            selection,
            on_select_fn: spec
                .on_select_fn_key
                .as_ref()
                .map(|k| LuaFunctionRef::new(k.clone())),
            on_submit_fn: spec
                .on_submit_fn_key
                .as_ref()
                .map(|k| LuaFunctionRef::new(k.clone())),
            view_data: spec.view_data.clone(),
            keys: std::collections::HashMap::new(),
        }
    }
}

/// Result of applying effects.
#[derive(Debug, Default)]
pub struct ApplyResult {
    /// Groups to display (from SetGroups effect).
    pub groups: Option<Vec<super::types::Group>>,
    /// Whether dismiss was called.
    pub dismissed: bool,
    /// Whether pop was called.
    pub popped: bool,
    /// Progress message, if any.
    pub progress: Option<String>,
    /// Completion message, if any.
    pub completed: Option<String>,
    /// Error message, if any.
    pub error: Option<String>,
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
