//! Action execution and filtering logic.

use std::sync::Arc;

use mlua::Lua;
use parking_lot::{Mutex, RwLock};

use crate::plugin_api::context::{
    build_action_applies_context, build_action_run_context, EngineState,
};
use crate::plugin_api::registry::PluginRegistry;
use crate::plugin_api::types::{ActionResult, Item, KeyBinding, KeypressResult, ViewInstance};

use super::types::ActionInfo;
use super::view_stack::{self};

/// Get actions that apply to the given items.
pub fn get_applicable_actions(
    registry: &PluginRegistry,
    lua: &Lua,
    items: &[Item],
) -> Result<Vec<ActionInfo>, String> {
    let mut applicable = Vec::new();

    if items.is_empty() {
        return Ok(applicable);
    }

    // For single item, check all actions
    // For multiple items, only check bulk actions
    let check_bulk_only = items.len() > 1;

    registry.for_each_action(|plugin_name, action_index, action| {
        // Skip non-bulk actions for multi-select
        if check_bulk_only && !action.bulk {
            return;
        }

        // Check if action applies to the first item
        // (For bulk, we assume if it applies to one, it applies to all of same type)
        let ctx = match build_action_applies_context(lua, &items[0]) {
            Ok(ctx) => ctx,
            Err(e) => {
                tracing::error!("Failed to build action applies context: {}", e);
                return;
            }
        };

        match action.applies_fn.call::<_, bool>(lua, ctx) {
            Ok(true) => {
                applicable.push(ActionInfo {
                    plugin_name: plugin_name.to_string(),
                    action_index,
                    id: action.id.clone(),
                    title: action.title.clone(),
                    icon: action.icon.clone(),
                    bulk: action.bulk,
                });
            }
            Ok(false) => {}
            Err(e) => {
                tracing::error!("Action applies check failed: {}", e);
            }
        }
    });

    Ok(applicable)
}

/// Get the default action for the given items (first applicable).
pub fn get_default_action(
    registry: &PluginRegistry,
    lua: &Lua,
    items: &[Item],
) -> Result<Option<ActionInfo>, String> {
    let actions = get_applicable_actions(registry, lua, items)?;
    Ok(actions.into_iter().next())
}

/// Execute an action on the given items.
pub fn execute_action(
    registry: &PluginRegistry,
    view_stack: &RwLock<Vec<ViewInstance>>,
    lua: &Lua,
    plugin_name: &str,
    action_index: usize,
    items: &[Item],
) -> Result<ActionResult, String> {
    let view_data = {
        let stack = view_stack.read();
        stack
            .last()
            .map(|v| v.view.view_data.clone())
            .unwrap_or(serde_json::Value::Null)
    };

    let state = Arc::new(Mutex::new(EngineState::new()));
    let ctx = build_action_run_context(lua, items, &view_data, Arc::clone(&state))
        .map_err(|e| format!("Failed to build action context: {}", e))?;

    // Run the action
    registry
        .with_action(plugin_name, action_index, |action| {
            action.run_fn.call::<_, ()>(lua, ctx)
        })
        .ok_or_else(|| format!("Action not found: {}:{}", plugin_name, action_index))?
        .map_err(|e| format!("Action execution failed: {}", e))?;

    // Process state changes
    let state = match Arc::try_unwrap(state) {
        Ok(mutex) => mutex.into_inner(),
        Err(arc) => arc.lock().clone(),
    };

    // Handle view operations
    if let Some(pushed) = state.pushed_view {
        if pushed.replace {
            view_stack::replace_view(view_stack, pushed.view, pushed.initial_query);
        } else {
            view_stack::push_view(view_stack, pushed.view, pushed.initial_query);
        }
        return Ok(ActionResult::PushView {
            title: None,
            query: None,
        });
    }

    if state.popped {
        view_stack::pop_view(view_stack);
        return Ok(ActionResult::Pop);
    }

    if state.dismissed {
        return Ok(ActionResult::Dismiss);
    }

    // Handle completion states
    if let Some(error) = state.error {
        return Ok(ActionResult::Fail { error });
    }

    if let Some(completion) = state.completion {
        return Ok(ActionResult::Complete {
            message: completion.message,
            actions: completion
                .follow_up_actions
                .into_iter()
                .map(|a| crate::plugin_api::types::FollowUpAction {
                    title: a.title,
                    icon: a.icon,
                })
                .collect(),
        });
    }

    if let Some(message) = state.progress_message {
        return Ok(ActionResult::Progress { message });
    }

    // Default: continue
    Ok(ActionResult::Continue)
}

/// Handle a keypress, checking view-specific bindings.
pub fn handle_keypress(
    registry: &PluginRegistry,
    view_stack: &RwLock<Vec<ViewInstance>>,
    lua: &Lua,
    key: &str,
    items: &[Item],
) -> Result<KeypressResult, String> {
    // Get current view's key bindings
    let binding = {
        let stack = view_stack.read();
        stack.last().and_then(|v| v.view.keys.get(key).cloned())
    };

    match binding {
        Some(KeyBinding::Function(func_ref)) => {
            // Build a generic context with items
            let view_data = {
                let stack = view_stack.read();
                stack
                    .last()
                    .map(|v| v.view.view_data.clone())
                    .unwrap_or(serde_json::Value::Null)
            };

            let state = Arc::new(Mutex::new(EngineState::new()));
            let ctx = build_action_run_context(lua, items, &view_data, Arc::clone(&state))
                .map_err(|e| format!("Failed to build key context: {}", e))?;

            func_ref
                .call::<_, ()>(lua, ctx)
                .map_err(|e| format!("Key handler failed: {}", e))?;

            // Process any state changes from the handler
            let state = state.lock();
            if let Some(ref _pushed) = state.pushed_view {
                // Would need to handle view push here
            }
            if state.dismissed {
                // Would signal dismiss
            }

            Ok(KeypressResult::Handled)
        }
        Some(KeyBinding::ActionId(action_id)) => {
            // Find and execute the action by ID
            let action_info = find_action_by_id(registry, &action_id);
            if let Some((plugin_name, action_index)) = action_info {
                execute_action(registry, view_stack, lua, &plugin_name, action_index, items)?;
                Ok(KeypressResult::Handled)
            } else {
                tracing::warn!("Action not found for key binding: {}", action_id);
                Ok(KeypressResult::NotHandled)
            }
        }
        None => Ok(KeypressResult::NotHandled),
    }
}

/// Find an action by its ID.
fn find_action_by_id(registry: &PluginRegistry, action_id: &str) -> Option<(String, usize)> {
    let mut found = None;
    registry.for_each_action(|plugin_name, action_index, action| {
        if action.id == action_id && found.is_none() {
            found = Some((plugin_name.to_string(), action_index));
        }
    });
    found
}
