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
use parking_lot::Mutex;
use tokio::sync::watch;

use crate::effect::{Effect, ViewSpec};
use crate::lua::cleanup_view_registry_keys;
use crate::registry::PluginRegistry;
use crate::types::{LuaFunctionRef, View, ViewInstance, ViewState};
use lux_core::{ActionResult, Group, Groups, Item, SelectionMode};

// Import submodules
mod engine_impl;
mod observable_view_stack;

// Re-export ActionInfo from submodules
pub use engine_impl::ActionInfo;
use observable_view_stack::ObservableViewStack;

// =============================================================================
// Query Engine
// =============================================================================

/// The QueryEngine orchestrates plugin execution.
///
/// It maintains the view stack, handles trigger/source/action execution,
/// and coordinates with the Lua runtime.
///
/// ## Reactive State
///
/// The view stack is observable - subscribe to changes via `subscribe()`.
/// All mutations (push, pop, replace) automatically broadcast to subscribers.
pub struct QueryEngine {
    /// Plugin registry containing all registered plugins.
    registry: Arc<PluginRegistry>,

    /// View stack (bottom = root, top = current).
    /// Observable - mutations auto-broadcast to subscribers.
    view_stack: ObservableViewStack,

    /// Current query generation for async cancellation.
    query_generation: Mutex<u64>,
}

impl QueryEngine {
    /// Create a new QueryEngine with the given registry.
    pub fn new(registry: Arc<PluginRegistry>) -> Self {
        Self {
            registry,
            view_stack: ObservableViewStack::new(),
            query_generation: Mutex::new(0),
        }
    }

    /// Subscribe to view stack changes.
    ///
    /// Returns a receiver that will be notified whenever the view stack changes.
    /// Clone the receiver for multiple subscribers.
    pub fn subscribe(&self) -> watch::Receiver<Vec<ViewState>> {
        self.view_stack.subscribe()
    }

    /// Initialize with the root view.
    ///
    /// This should be called after plugins are loaded to set up the initial view.
    /// Broadcasts the new state to subscribers.
    pub fn initialize(&self, lua: &Lua) {
        // Clear any existing views
        self.view_stack.clear();

        // Create the default root view and push it (auto-broadcasts)
        let root_view = self.create_default_root_view(lua);
        self.view_stack.push(ViewInstance::new(root_view));

        tracing::debug!("QueryEngine initialized with root view");
    }

    /// Create the default root view that aggregates all root sources.
    fn create_default_root_view(&self, _lua: &Lua) -> View {
        // Create a placeholder source function
        // The actual implementation will call search_root_sources
        let source_key = format!("engine:root_view:source:{}", uuid::Uuid::new_v4());

        // We can't easily create a Lua function here that calls back to Rust,
        // so we'll use a special marker and handle it in search()
        View {
            id: None, // Root view has no ID
            title: None,
            placeholder: Some("Search...".to_string()),
            source_fn: LuaFunctionRef::new(source_key),
            selection: SelectionMode::Single,
            on_select_fn: None,
            on_submit_fn: None,
            view_data: serde_json::Value::Null,
        }
    }

    // =========================================================================
    // View Stack Operations
    // =========================================================================

    /// Get the current view state for the frontend.
    pub fn get_current_view_state(&self) -> Option<ViewState> {
        self.view_stack.get_current_state()
    }

    /// Get the entire view stack state.
    pub fn get_view_stack(&self) -> Vec<ViewState> {
        self.view_stack.get_states()
    }

    /// Push a new view onto the stack.
    ///
    /// Broadcasts the new state to subscribers.
    pub fn push_view(&self, view: View) {
        self.view_stack.push(ViewInstance::new(view));
    }

    /// Replace the current view.
    ///
    /// Broadcasts the new state to subscribers.
    pub fn replace_view(&self, view: View) {
        self.view_stack.replace_top(ViewInstance::new(view));
    }

    /// Pop the current view and return to the previous one.
    ///
    /// Returns false if already at root. Broadcasts the new state to subscribers.
    pub fn pop_view(&self) -> bool {
        self.view_stack.pop_if_not_root()
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
            if self.view_stack.len() > 1 {
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

    /// Execute a Lua callback with action-style context.
    ///
    /// This is used for keybindings that map to Lua functions, but the engine
    /// doesn't know about keybindings specifically - it just executes the
    /// callback with the current view's context.
    ///
    /// The callback receives: items, view_data (via ActionContextLua bridge).
    /// Any effects the callback collects are applied and converted to ActionResult.
    pub fn execute_lua_callback(
        &self,
        lua: &Lua,
        func_ref: &crate::types::LuaFunctionRef,
        items: &[Item],
    ) -> Result<ActionResult, String> {
        let view_data = self
            .view_stack
            .with_top(|v| v.view.view_data.clone())
            .unwrap_or(serde_json::Value::Null);

        let effects = crate::lua::call_action_run(lua, &func_ref.key, items, &view_data)
            .map_err(|e| format!("Lua callback failed: {}", e))?;

        let result = self.apply_effects(lua, effects);
        Ok(self.apply_result_to_action_result(result))
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
        if self.view_stack.len() > 1 {
            return ActionResult::PushView {
                title: None,
                query: None,
            };
        }

        ActionResult::Continue
    }

    // =========================================================================
    // Selection Hook (Custom Mode)
    // =========================================================================

    /// Handle selection in custom mode by calling on_select hook.
    ///
    /// Uses effect-based execution: the callback collects effects,
    /// which are applied via `apply_effects()`.
    ///
    /// The UI passes the current selection since it owns that state.
    pub fn handle_custom_select(
        &self,
        lua: &Lua,
        item: &Item,
        current_selection: &[String],
    ) -> Result<(), String> {
        let (on_select_key, view_data) = self
            .view_stack
            .with_top(|view| {
                let key = view.view.on_select_fn.as_ref().map(|f| f.key.clone());
                (key, view.view.view_data.clone())
            })
            .unwrap_or((None, serde_json::Value::Null));

        let on_select_key = match on_select_key {
            Some(k) => k,
            None => return Ok(()), // No custom handler
        };

        // Convert slice to HashSet for the Lua bridge
        let selection_set: std::collections::HashSet<String> =
            current_selection.iter().cloned().collect();

        // Call via the bridge, which uses effect-based execution
        let effects = crate::lua::call_view_on_select(
            lua,
            &on_select_key,
            item,
            &view_data,
            &selection_set,
        )
        .map_err(|e| format!("on_select failed: {}", e))?;

        // Apply effects
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
    /// The UI passes the current query since it owns that state.
    /// Returns true if dismiss was called.
    pub fn handle_submit(&self, lua: &Lua, query: &str) -> Result<bool, String> {
        let (on_submit_key, view_data) = self
            .view_stack
            .with_top(|view| {
                let key = view.view.on_submit_fn.as_ref().map(|f| f.key.clone());
                (key, view.view.view_data.clone())
            })
            .unwrap_or((None, serde_json::Value::Null));

        let on_submit_key = match on_submit_key {
            Some(k) => k,
            None => return Ok(false), // No submit handler
        };

        // Call via the bridge, which uses effect-based execution
        let effects = crate::lua::call_view_on_submit(lua, &on_submit_key, query, &view_data)
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
    /// View stack mutations (push/pop/replace) auto-broadcast to subscribers.
    /// Selection effects are ignored - UI owns selection state.
    ///
    /// Returns information about what happened for the caller to act on.
    pub fn apply_effects(&self, lua: &Lua, effects: Vec<Effect>) -> ApplyResult {
        let mut result = ApplyResult::default();

        for effect in effects {
            match effect {
                Effect::SetGroups(groups) => {
                    result.groups = Some(groups);
                }
                Effect::PushView(spec) => {
                    let view = self.view_from_spec(&spec);
                    let registry_keys = spec.registry_keys.clone();
                    let instance = ViewInstance::with_registry_keys(view, registry_keys);
                    self.view_stack.push(instance);
                    tracing::debug!("Applied PushView, stack depth: {}", self.view_stack.len());
                }
                Effect::ReplaceView(spec) => {
                    let view = self.view_from_spec(&spec);
                    let registry_keys = spec.registry_keys.clone();
                    let instance = ViewInstance::with_registry_keys(view, registry_keys);

                    // Replace and cleanup old view's registry keys
                    if let Some(old_view) = self.view_stack.replace_top(instance) {
                        cleanup_view_registry_keys(lua, &old_view.registry_keys);
                    }
                    tracing::debug!(
                        "Applied ReplaceView, stack depth: {}",
                        self.view_stack.len()
                    );
                }
                Effect::Pop => {
                    if self.view_stack.len() > 1 {
                        if let Some(old_view) = self.view_stack.pop() {
                            cleanup_view_registry_keys(lua, &old_view.registry_keys);
                        }
                        tracing::debug!("Applied Pop, stack depth: {}", self.view_stack.len());
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
                // Selection effects are ignored - UI owns selection state
                Effect::Select(_) | Effect::Deselect(_) | Effect::ClearSelection => {
                    tracing::debug!("Ignoring selection effect - UI owns selection state");
                }
            }
        }

        result
    }

    /// Convert a ViewSpec to a View.
    fn view_from_spec(&self, spec: &ViewSpec) -> View {
        View {
            id: spec.id.clone(),
            title: spec.title.clone(),
            placeholder: spec.placeholder.clone(),
            source_fn: LuaFunctionRef::new(spec.source_fn_key.clone()),
            selection: spec.selection_mode,
            on_select_fn: spec
                .on_select_fn_key
                .as_ref()
                .map(|k| LuaFunctionRef::new(k.clone())),
            on_submit_fn: spec
                .on_submit_fn_key
                .as_ref()
                .map(|k| LuaFunctionRef::new(k.clone())),
            view_data: spec.view_data.clone(),
        }
    }
}

/// Result of applying effects.
#[derive(Debug, Default)]
pub struct ApplyResult {
    /// Groups to display (from SetGroups effect).
    pub groups: Option<Vec<Group>>,
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

        // Create test views
        let view1 = View {
            id: Some("view1".to_string()),
            title: Some("View 1".to_string()),
            placeholder: None,
            source_fn: LuaFunctionRef::new("test:source:1".to_string()),
            selection: SelectionMode::Single,
            on_select_fn: None,
            on_submit_fn: None,
            view_data: serde_json::Value::Null,
        };

        let view2 = View {
            id: Some("view2".to_string()),
            title: Some("View 2".to_string()),
            placeholder: None,
            source_fn: LuaFunctionRef::new("test:source:2".to_string()),
            selection: SelectionMode::Multi,
            on_select_fn: None,
            on_submit_fn: None,
            view_data: serde_json::Value::Null,
        };

        // Push views
        engine.push_view(view1);
        assert_eq!(engine.get_view_stack().len(), 1);

        engine.push_view(view2);
        assert_eq!(engine.get_view_stack().len(), 2);

        // Check current view
        let current = engine.get_current_view_state().unwrap();
        assert_eq!(current.title, Some("View 2".to_string()));
        assert_eq!(current.selection, SelectionMode::Multi);

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
    fn test_subscribe_broadcasts_changes() {
        let registry = Arc::new(PluginRegistry::new());
        let engine = QueryEngine::new(registry);

        // Subscribe before any changes
        let rx = engine.subscribe();
        assert!(rx.borrow().is_empty());

        // Push a view
        let view = View {
            id: Some("test_view".to_string()),
            title: Some("Test View".to_string()),
            placeholder: Some("Search...".to_string()),
            source_fn: LuaFunctionRef::new("test:source".to_string()),
            selection: SelectionMode::Single,
            on_select_fn: None,
            on_submit_fn: None,
            view_data: serde_json::Value::Null,
        };

        engine.push_view(view);

        // Subscriber should see the change
        let states = rx.borrow().clone();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].title, Some("Test View".to_string()));
        assert_eq!(states[0].placeholder, Some("Search...".to_string()));
    }
}
