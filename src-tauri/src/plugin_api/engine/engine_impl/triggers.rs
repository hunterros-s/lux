//! Trigger matching and execution logic.

use std::sync::Arc;

use mlua::Lua;
use parking_lot::Mutex;

use crate::plugin_api::context::{
    build_trigger_match_context, build_trigger_run_context, EngineState,
};
use crate::plugin_api::registry::PluginRegistry;

/// Find all triggers that match the current query.
pub fn find_matching_triggers(
    registry: &PluginRegistry,
    lua: &Lua,
    query: &str,
) -> Result<Vec<(String, usize)>, String> {
    let mut matching = Vec::new();

    registry.for_each_trigger(|plugin_name, trigger_index, trigger| {
        // Check prefix match first (fast path)
        if let Some(ref prefix) = trigger.prefix {
            if query.starts_with(prefix) {
                matching.push((plugin_name.to_string(), trigger_index));
                return;
            }
        }

        // Check match function
        if let Some(ref match_fn) = trigger.match_fn {
            let ctx = match build_trigger_match_context(lua, query) {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("Failed to build trigger match context: {}", e);
                    return;
                }
            };

            match match_fn.call::<_, bool>(lua, ctx) {
                Ok(true) => {
                    matching.push((plugin_name.to_string(), trigger_index));
                }
                Ok(false) => {}
                Err(e) => {
                    tracing::error!("Trigger match function failed: {}", e);
                }
            }
        }
    });

    Ok(matching)
}

/// Run a single trigger and return its results.
pub fn run_trigger(
    registry: &PluginRegistry,
    lua: &Lua,
    plugin_name: &str,
    trigger_index: usize,
    query: &str,
) -> Result<EngineState, String> {
    let state = Arc::new(Mutex::new(EngineState::new()));

    // Calculate args (query without prefix)
    let args = registry
        .with_trigger(plugin_name, trigger_index, |trigger| {
            trigger
                .prefix
                .as_ref()
                .map(|p| query.strip_prefix(p).unwrap_or(query).to_string())
                .unwrap_or_else(|| query.to_string())
        })
        .unwrap_or_else(|| query.to_string());

    // Build context and run
    let ctx = build_trigger_run_context(lua, query, &args, Arc::clone(&state))
        .map_err(|e| format!("Failed to build trigger context: {}", e))?;

    registry
        .with_trigger(plugin_name, trigger_index, |trigger| {
            trigger.run_fn.call::<_, ()>(lua, ctx)
        })
        .ok_or_else(|| format!("Trigger not found: {}:{}", plugin_name, trigger_index))?
        .map_err(|e| format!("Trigger run failed: {}", e))?;

    let result = match Arc::try_unwrap(state) {
        Ok(mutex) => mutex.into_inner(),
        Err(arc) => arc.lock().clone(),
    };

    Ok(result)
}
