//! Action execution and filtering logic.

use mlua::Lua;

use crate::context::build_action_applies_context;
use crate::effect::Effect;
use crate::engine::observable_view_stack::ObservableViewStack;
use crate::lua::call_action_run;
use crate::registry::PluginRegistry;
use lux_core::Item;

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
    view_stack: &ObservableViewStack,
    lua: &Lua,
    plugin_name: &str,
    action_index: usize,
    items: &[Item],
) -> Result<Vec<Effect>, String> {
    let view_data = view_stack
        .with_top(|v| v.view.view_data.clone())
        .unwrap_or(serde_json::Value::Null);

    // Get the run function key
    let run_fn_key = registry
        .with_action(plugin_name, action_index, |action| {
            action.run_fn.key.clone()
        })
        .ok_or_else(|| format!("Action not found: {}:{}", plugin_name, action_index))?;

    // Call via the bridge, which uses effect-based execution
    let effects = call_action_run(lua, &run_fn_key, items, &view_data)
        .map_err(|e| format!("Action execution failed: {}", e))?;

    Ok(effects)
}
