//! Action execution and filtering logic.

use mlua::Lua;
use parking_lot::RwLock;

use crate::plugin_api::context::build_action_applies_context;
use crate::plugin_api::effect::Effect;
use crate::plugin_api::lua::call_action_run;
use crate::plugin_api::registry::PluginRegistry;
use crate::plugin_api::types::{Item, KeyBinding, ViewInstance};

use super::types::ActionInfo;

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

/// Execute an action on the given items and return effects.
///
/// Uses effect-based execution: the action callback collects effects,
/// which are returned for the engine to apply via `apply_effects()`.
pub fn execute_action(
    registry: &PluginRegistry,
    view_stack: &RwLock<Vec<ViewInstance>>,
    lua: &Lua,
    plugin_name: &str,
    action_index: usize,
    items: &[Item],
) -> Result<Vec<Effect>, String> {
    let view_data = {
        let stack = view_stack.read();
        stack
            .last()
            .map(|v| v.view.view_data.clone())
            .unwrap_or(serde_json::Value::Null)
    };

    // Get the run function key
    let run_fn_key = registry
        .with_action(plugin_name, action_index, |action| action.run_fn.key.clone())
        .ok_or_else(|| format!("Action not found: {}:{}", plugin_name, action_index))?;

    // Call via the bridge, which uses effect-based execution
    let effects = call_action_run(lua, &run_fn_key, items, &view_data)
        .map_err(|e| format!("Action execution failed: {}", e))?;

    Ok(effects)
}

/// Result of handling a keypress.
pub enum KeypressEffects {
    /// Key was handled, effects should be applied.
    Handled(Vec<Effect>),
    /// Key was not handled, frontend should process.
    NotHandled,
}

/// Handle a keypress, checking view-specific bindings.
///
/// Returns effects if the key was handled, or NotHandled if the key
/// should be processed by the frontend.
pub fn handle_keypress(
    registry: &PluginRegistry,
    view_stack: &RwLock<Vec<ViewInstance>>,
    lua: &Lua,
    key: &str,
    items: &[Item],
) -> Result<KeypressEffects, String> {
    // Get current view's key bindings
    let binding = {
        let stack = view_stack.read();
        stack.last().and_then(|v| v.view.keys.get(key).cloned())
    };

    match binding {
        Some(KeyBinding::Function(func_ref)) => {
            // Get view_data for the action context
            let view_data = {
                let stack = view_stack.read();
                stack
                    .last()
                    .map(|v| v.view.view_data.clone())
                    .unwrap_or(serde_json::Value::Null)
            };

            // Call via the bridge, which uses effect-based execution
            let effects = call_action_run(lua, &func_ref.key, items, &view_data)
                .map_err(|e| format!("Key handler failed: {}", e))?;

            Ok(KeypressEffects::Handled(effects))
        }
        Some(KeyBinding::ActionId(action_id)) => {
            // Find and execute the action by ID
            let action_info = find_action_by_id(registry, &action_id);
            if let Some((plugin_name, action_index)) = action_info {
                let effects =
                    execute_action(registry, view_stack, lua, &plugin_name, action_index, items)?;
                Ok(KeypressEffects::Handled(effects))
            } else {
                tracing::warn!("Action not found for key binding: {}", action_id);
                Ok(KeypressEffects::NotHandled)
            }
        }
        None => Ok(KeypressEffects::NotHandled),
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
