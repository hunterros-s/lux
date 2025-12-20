//! Trigger matching and execution logic.

use mlua::Lua;

use crate::context::build_trigger_match_context;
use crate::effect::Effect;
use crate::lua::call_trigger_run;
use crate::registry::PluginRegistry;

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

/// Run a single trigger and return its effects.
///
/// Uses effect-based execution: the trigger callback collects effects,
/// which are returned for the engine to apply via `apply_effects()`.
pub fn run_trigger(
    registry: &PluginRegistry,
    lua: &Lua,
    plugin_name: &str,
    trigger_index: usize,
    query: &str,
) -> Result<Vec<Effect>, String> {
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

    // Get the run function key
    let run_fn_key = registry
        .with_trigger(plugin_name, trigger_index, |trigger| {
            trigger.run_fn.key.clone()
        })
        .ok_or_else(|| format!("Trigger not found: {}:{}", plugin_name, trigger_index))?;

    // Call via the bridge, which uses effect-based execution
    let effects = call_trigger_run(lua, &run_fn_key, query, &args)
        .map_err(|e| format!("Trigger run failed: {}", e))?;

    Ok(effects)
}
