//! Source searching and result aggregation logic.

use std::sync::Arc;

use mlua::Lua;
use parking_lot::{Mutex, RwLock};

use crate::plugin_api::context::{build_source_search_context, EngineState};
use crate::plugin_api::registry::PluginRegistry;
use crate::plugin_api::types::{Groups, Item, ViewInstance};

/// Run the current view's source function.
pub fn run_current_view_source(
    registry: &PluginRegistry,
    view_stack: &RwLock<Vec<ViewInstance>>,
    lua: &Lua,
    query: &str,
) -> Result<Groups, String> {
    // Check if we're at the root view with aggregated sources
    let is_root = {
        let stack = view_stack.read();
        stack.len() <= 1
    };

    if is_root {
        // Aggregate all root sources
        return search_root_sources(registry, lua, query);
    }

    // Get current view's source function and view_data
    let (source_key, view_data) = {
        let stack = view_stack.read();
        match stack.last() {
            Some(view) => (view.view.source_fn.key.clone(), view.view.view_data.clone()),
            None => return Ok(Groups::new()),
        }
    };

    // Build context
    let state = Arc::new(Mutex::new(EngineState::new()));
    let ctx = build_source_search_context(lua, query, &view_data, Arc::clone(&state))
        .map_err(|e| format!("Failed to build source context: {}", e))?;

    // Call the source function
    let result: mlua::Table = {
        let registry_key = lua
            .named_registry_value::<mlua::RegistryKey>(&source_key)
            .map_err(|e| format!("Source function not found: {}", e))?;
        let func: mlua::Function = lua
            .registry_value(&registry_key)
            .map_err(|e| format!("Failed to get source function: {}", e))?;
        func.call(ctx)
            .map_err(|e| format!("Source function failed: {}", e))?
    };

    // Parse the returned groups
    parse_groups_from_lua(lua, result)
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

    for (plugin_name, source_index) in root_sources {
        let source_results = run_source(
            registry,
            lua,
            &plugin_name,
            source_index,
            query,
            &serde_json::Value::Null,
        )?;

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
                all_results.push(crate::plugin_api::types::Group {
                    title: Some(title),
                    items,
                });
            }
        } else {
            // Add groups as-is
            all_results.extend(source_results);
        }
    }

    Ok(all_results)
}

/// Run a single source and return its results.
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

    // Build context
    let state = Arc::new(Mutex::new(EngineState::new()));
    let ctx = build_source_search_context(lua, query, view_data, Arc::clone(&state))
        .map_err(|e| format!("Failed to build source context: {}", e))?;

    // Call the source function
    let result = registry
        .with_source(plugin_name, source_index, |source| {
            source.search_fn.call::<_, mlua::Table>(lua, ctx)
        })
        .ok_or_else(|| format!("Source not found: {}:{}", plugin_name, source_index))?
        .map_err(|e| format!("Source search failed: {}", e))?;

    // Check if loading was called (async source)
    {
        let state = state.lock();
        if state.loading {
            // Async source - results will come via resolve()
            // For now, return empty and let frontend poll
            tracing::debug!("Source {}:{} is loading async", plugin_name, source_index);
        }
        if let Some(ref resolved) = state.resolved_results {
            return Ok(resolved.clone());
        }
    }

    // Parse the returned groups
    parse_groups_from_lua(lua, result)
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Parse Groups from a Lua table.
fn parse_groups_from_lua(lua: &Lua, table: mlua::Table) -> Result<Groups, String> {
    use crate::plugin_api::types::Group;

    let mut groups = Vec::new();

    for pair in table.pairs::<i64, mlua::Table>() {
        let (_, group_table) = pair.map_err(|e| format!("Failed to iterate groups: {}", e))?;

        let title: Option<String> = group_table
            .get("title")
            .map_err(|e| format!("Failed to get group title: {}", e))?;

        let items_table: mlua::Table = group_table
            .get("items")
            .map_err(|e| format!("Failed to get group items: {}", e))?;

        let mut items = Vec::new();
        for item_pair in items_table.pairs::<i64, mlua::Table>() {
            let (_, item_table) =
                item_pair.map_err(|e| format!("Failed to iterate items: {}", e))?;
            items.push(parse_item_from_lua(lua, item_table)?);
        }

        groups.push(Group { title, items });
    }

    Ok(groups)
}

/// Parse an Item from a Lua table.
fn parse_item_from_lua(lua: &Lua, table: mlua::Table) -> Result<Item, String> {
    // ID is optional - auto-generate UUID if not provided
    let id: String = table
        .get::<Option<String>>("id")
        .map_err(|e| format!("Failed to get id: {}", e))?
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let title: String = table
        .get("title")
        .map_err(|e| format!("Item missing title: {}", e))?;
    let subtitle: Option<String> = table.get("subtitle").ok();
    let icon: Option<String> = table.get("icon").ok();

    let types: Vec<String> = table
        .get::<Option<mlua::Table>>("types")
        .ok()
        .flatten()
        .map(|t| {
            t.pairs::<i64, String>()
                .filter_map(|r| r.ok().map(|(_, v)| v))
                .collect()
        })
        .unwrap_or_default();

    let data: Option<serde_json::Value> = table
        .get::<Option<mlua::Value>>("data")
        .ok()
        .flatten()
        .map(|v| crate::plugin_api::lua::lua_value_to_json(lua, v))
        .transpose()
        .map_err(|e| format!("Failed to parse item data: {}", e))?;

    Ok(Item {
        id,
        title,
        subtitle,
        icon,
        types,
        data,
    })
}
