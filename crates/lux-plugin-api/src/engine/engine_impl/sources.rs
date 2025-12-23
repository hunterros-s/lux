//! Source searching for the query engine.
//!
//! This module handles running the current view's search function
//! and extracting results from the effects.

use mlua::Lua;

use crate::effect::Effect;
use crate::engine::observable_view_stack::ObservableViewStack;
use crate::lua::call_hooked_search;
use crate::registry::PluginRegistry;
use lux_core::Groups;

/// Run the current view's source function.
///
/// Uses effect-based execution: the source collects effects,
/// we extract the SetGroups effect for the results.
///
/// If hooks are registered for "search", they are executed in chain:
/// each hook receives `(query, ctx, original)` and can call `original()`
/// to continue to the next hook or the actual search function.
pub fn run_current_view_source(
    registry: &PluginRegistry,
    view_stack: &ObservableViewStack,
    lua: &Lua,
    query: &str,
) -> Result<Groups, String> {
    // Get current view's source function, view_data, and view_id
    let (source_key, view_data, view_id) = view_stack
        .with_top(|view| {
            (
                view.view.source_fn.key.clone(),
                view.view.view_data.clone(),
                view.view.id.clone(),
            )
        })
        .ok_or_else(|| "No current view".to_string())?;

    // Get hook chain for "search" (view-specific + global)
    let hook_registry = registry.hooks();
    let hooks = hook_registry.get_chain("search", view_id.as_deref());
    let hook_keys: Vec<String> = hooks.iter().map(|h| h.key.clone()).collect();

    // Call via the bridge with hook chain (handles empty case transparently)
    let effects = call_hooked_search(lua, &source_key, &hook_keys, query, &view_data)
        .map_err(|e| format!("Source search failed: {}", e))?;

    // Extract groups from the SetGroups effect
    Ok(extract_groups_from_effects(effects))
}

/// Extract groups from a list of effects.
///
/// Looks for the SetGroups effect and returns its contents.
/// If no SetGroups effect, returns empty groups.
fn extract_groups_from_effects(effects: Vec<Effect>) -> Groups {
    for effect in effects {
        if let Effect::SetGroups(groups) = effect {
            return groups;
        }
    }
    Groups::new()
}
