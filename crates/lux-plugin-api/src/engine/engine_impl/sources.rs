//! Source searching and result aggregation logic.

use mlua::Lua;

use crate::effect::Effect;
use crate::engine::observable_view_stack::ObservableViewStack;
use crate::lua::call_source_search;
use crate::registry::PluginRegistry;
use lux_core::{Group, Groups};

/// Run the current view's source function.
///
/// Uses effect-based execution: the source collects effects,
/// we extract the SetGroups effect for the results.
pub fn run_current_view_source(
    registry: &PluginRegistry,
    view_stack: &ObservableViewStack,
    lua: &Lua,
    query: &str,
) -> Result<Groups, String> {
    // Check if we're at the root view with aggregated sources
    if view_stack.len() <= 1 {
        // Aggregate all root sources
        return search_root_sources(registry, lua, query);
    }

    // Get current view's source function and view_data
    let (source_key, view_data) = view_stack
        .with_top(|view| {
            (
                view.view.source_fn.key.clone(),
                view.view.view_data.clone(),
            )
        })
        .ok_or_else(|| "No current view".to_string())?;

    // Call via the bridge, which uses effect-based execution
    let effects = call_source_search(lua, &source_key, query, &view_data)
        .map_err(|e| format!("Source search failed: {}", e))?;

    // Extract groups from the SetGroups effect
    Ok(extract_groups_from_effects(effects))
}

/// Search all root sources and aggregate results.
pub fn search_root_sources(
    registry: &PluginRegistry,
    lua: &Lua,
    query: &str,
) -> Result<Groups, String> {
    let mut all_results = Groups::new();

    // Collect root sources
    let root_sources: Vec<(String, usize)> = registry.get_root_sources();
    tracing::debug!("Root sources found: {:?}", root_sources);

    for (plugin_name, source_index) in root_sources {
        let source_results = run_source(
            registry,
            lua,
            &plugin_name,
            source_index,
            query,
            &serde_json::Value::Null,
        )?;

        let total_items: usize = source_results.iter().map(|g| g.items.len()).sum();
        tracing::debug!(
            "Source {}:{} returned {} groups with {} items",
            plugin_name,
            source_index,
            source_results.len(),
            total_items
        );

        // Wrap results with source's group title if specified
        let group_title = registry
            .with_source(&plugin_name, source_index, |source| source.group.clone())
            .flatten();

        if let Some(title) = group_title {
            // Merge all items into a single group with the source's title
            let mut items = Vec::new();
            for group in source_results {
                items.extend(group.items);
            }
            if !items.is_empty() {
                all_results.push(Group {
                    title: Some(title),
                    items,
                });
            }
        } else {
            // Add groups as-is
            all_results.extend(source_results);
        }
    }

    let total_items: usize = all_results.iter().map(|g| g.items.len()).sum();
    tracing::debug!(
        "Total root source results: {} groups with {} items",
        all_results.len(),
        total_items
    );
    Ok(all_results)
}

/// Run a single source and return its results.
///
/// Uses effect-based execution: the source callback collects effects,
/// we extract the SetGroups effect for the results.
pub fn run_source(
    registry: &PluginRegistry,
    lua: &Lua,
    plugin_name: &str,
    source_index: usize,
    query: &str,
    view_data: &serde_json::Value,
) -> Result<Groups, String> {
    // Check min_query_length
    let min_len = registry
        .with_source(plugin_name, source_index, |source| source.min_query_length)
        .unwrap_or(0);

    if (query.len() as u32) < min_len {
        return Ok(Groups::new());
    }

    // Get the search function key
    let search_fn_key = registry
        .with_source(plugin_name, source_index, |source| {
            source.search_fn.key.clone()
        })
        .ok_or_else(|| format!("Source not found: {}:{}", plugin_name, source_index))?;

    // Call via the bridge, which uses effect-based execution
    let effects = call_source_search(lua, &search_fn_key, query, view_data)
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
