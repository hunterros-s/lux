//! Lua table parsing for Plugin API types.
//!
//! This module converts Lua tables into Rust types for the Plugin API.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use mlua::{Function, Lua, Result as LuaResult, Table, Value};

use crate::plugin_api::types::{
    Action, KeyBinding, LuaFunctionRef, Plugin, SelectionMode, Source, Trigger, View,
};

use super::lua_value_to_json;

/// Global counter for generating unique function keys.
static FUNCTION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique key for storing a function in the Lua registry.
fn generate_function_key(prefix: &str) -> String {
    let id = FUNCTION_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("{}:{}", prefix, id)
}

/// Store a function in Lua's registry and return a reference to it.
fn store_function(lua: &Lua, func: Function, prefix: &str) -> LuaResult<LuaFunctionRef> {
    let key = generate_function_key(prefix);
    LuaFunctionRef::from_function(lua, func, key)
}

/// Parse a plugin definition from a Lua table.
///
/// Expected table shape:
/// ```lua
/// {
///   name = "string",           -- required
///   triggers = { ... },        -- optional
///   sources = { ... },         -- optional
///   actions = { ... },         -- optional
///   setup = function(config),  -- optional
/// }
/// ```
pub fn parse_plugin(lua: &Lua, table: Table) -> LuaResult<Plugin> {
    // Required: name
    let name: String = table
        .get("name")
        .map_err(|_| mlua::Error::RuntimeError("Plugin missing required 'name' field".into()))?;

    // Optional: triggers
    let triggers = match table.get::<Option<Table>>("triggers")? {
        Some(triggers_table) => parse_triggers(lua, &name, triggers_table)?,
        None => Vec::new(),
    };

    // Optional: sources
    let sources = match table.get::<Option<Table>>("sources")? {
        Some(sources_table) => parse_sources(lua, &name, sources_table)?,
        None => Vec::new(),
    };

    // Optional: actions
    let actions = match table.get::<Option<Table>>("actions")? {
        Some(actions_table) => parse_actions(lua, &name, actions_table)?,
        None => Vec::new(),
    };

    // Optional: setup function
    let setup_fn = match table.get::<Option<Function>>("setup")? {
        Some(func) => Some(store_function(
            lua,
            func,
            &format!("plugin:{}:setup", name),
        )?),
        None => None,
    };

    tracing::debug!(
        "Parsed plugin '{}': {} triggers, {} sources, {} actions",
        name,
        triggers.len(),
        sources.len(),
        actions.len()
    );

    Ok(Plugin {
        name,
        triggers,
        sources,
        actions,
        setup_fn,
    })
}

/// Parse an array of trigger definitions.
fn parse_triggers(lua: &Lua, plugin_name: &str, table: Table) -> LuaResult<Vec<Trigger>> {
    let mut triggers = Vec::new();

    for pair in table.pairs::<i64, Table>() {
        let (idx, trigger_table) = pair?;
        triggers.push(parse_trigger(
            lua,
            plugin_name,
            idx as usize,
            trigger_table,
        )?);
    }

    Ok(triggers)
}

/// Parse a single trigger definition.
///
/// Expected table shape:
/// ```lua
/// {
///   match = function(ctx),  -- optional (one of match or prefix required)
///   prefix = ":",           -- optional (one of match or prefix required)
///   run = function(ctx),    -- required
/// }
/// ```
fn parse_trigger(lua: &Lua, plugin_name: &str, index: usize, table: Table) -> LuaResult<Trigger> {
    // Optional: match function
    let match_fn = match table.get::<Option<Function>>("match")? {
        Some(func) => Some(store_function(
            lua,
            func,
            &format!("plugin:{}:trigger:{}:match", plugin_name, index),
        )?),
        None => None,
    };

    // Optional: prefix
    let prefix: Option<String> = table.get("prefix")?;

    // Validate: at least one of match or prefix must be provided
    if match_fn.is_none() && prefix.is_none() {
        return Err(mlua::Error::RuntimeError(
            "Trigger must have either 'match' function or 'prefix' string".into(),
        ));
    }

    // Required: run function
    let run_fn = table
        .get::<Function>("run")
        .map_err(|_| mlua::Error::RuntimeError("Trigger missing required 'run' function".into()))?;
    let run_fn = store_function(
        lua,
        run_fn,
        &format!("plugin:{}:trigger:{}:run", plugin_name, index),
    )?;

    Ok(Trigger {
        match_fn,
        prefix,
        run_fn,
    })
}

/// Parse an array of source definitions.
fn parse_sources(lua: &Lua, plugin_name: &str, table: Table) -> LuaResult<Vec<Source>> {
    let mut sources = Vec::new();

    for pair in table.pairs::<i64, Table>() {
        let (idx, source_table) = pair?;
        sources.push(parse_source(lua, plugin_name, idx as usize, source_table)?);
    }

    Ok(sources)
}

/// Parse a single source definition.
///
/// Expected table shape:
/// ```lua
/// {
///   name = "string",          -- optional
///   root = true,              -- optional, default false
///   group = "string",         -- optional
///   search = function(ctx),   -- required
///   debounce_ms = 0,          -- optional, default 0
///   min_query_length = 0,     -- optional, default 0
/// }
/// ```
fn parse_source(lua: &Lua, plugin_name: &str, index: usize, table: Table) -> LuaResult<Source> {
    // Optional: name
    let name: Option<String> = table.get("name")?;

    // Optional: root (default false)
    let root: bool = table.get("root").unwrap_or(false);

    // Optional: group
    let group: Option<String> = table.get("group")?;

    // Required: search function
    let search_fn = table.get::<Function>("search").map_err(|_| {
        mlua::Error::RuntimeError("Source missing required 'search' function".into())
    })?;
    let search_fn = store_function(
        lua,
        search_fn,
        &format!("plugin:{}:source:{}:search", plugin_name, index),
    )?;

    // Optional: debounce_ms (default 0)
    let debounce_ms: u32 = table.get("debounce_ms").unwrap_or(0);

    // Optional: min_query_length (default 0)
    let min_query_length: u32 = table.get("min_query_length").unwrap_or(0);

    Ok(Source {
        name,
        root,
        group,
        search_fn,
        debounce_ms,
        min_query_length,
    })
}

/// Parse an array of action definitions.
fn parse_actions(lua: &Lua, plugin_name: &str, table: Table) -> LuaResult<Vec<Action>> {
    let mut actions = Vec::new();

    for pair in table.pairs::<i64, Table>() {
        let (idx, action_table) = pair?;
        actions.push(parse_action(lua, plugin_name, idx as usize, action_table)?);
    }

    Ok(actions)
}

/// Parse a single action definition.
///
/// Expected table shape:
/// ```lua
/// {
///   id = "string",            -- required
///   title = "string",         -- required
///   icon = "string",          -- optional
///   bulk = false,             -- optional, default false
///   applies = function(ctx),  -- required
///   run = function(ctx),      -- required
/// }
/// ```
fn parse_action(lua: &Lua, plugin_name: &str, index: usize, table: Table) -> LuaResult<Action> {
    // Required: id
    let id: String = table
        .get("id")
        .map_err(|_| mlua::Error::RuntimeError("Action missing required 'id' field".into()))?;

    // Required: title
    let title: String = table
        .get("title")
        .map_err(|_| mlua::Error::RuntimeError("Action missing required 'title' field".into()))?;

    // Optional: icon
    let icon: Option<String> = table.get("icon")?;

    // Optional: bulk (default false)
    let bulk: bool = table.get("bulk").unwrap_or(false);

    // Required: applies function
    let applies_fn = table.get::<Function>("applies").map_err(|_| {
        mlua::Error::RuntimeError("Action missing required 'applies' function".into())
    })?;
    let applies_fn = store_function(
        lua,
        applies_fn,
        &format!("plugin:{}:action:{}:applies", plugin_name, index),
    )?;

    // Required: run function
    let run_fn = table
        .get::<Function>("run")
        .map_err(|_| mlua::Error::RuntimeError("Action missing required 'run' function".into()))?;
    let run_fn = store_function(
        lua,
        run_fn,
        &format!("plugin:{}:action:{}:run", plugin_name, index),
    )?;

    Ok(Action {
        id,
        title,
        icon,
        bulk,
        applies_fn,
        run_fn,
    })
}

/// Parse a view definition.
///
/// Expected table shape:
/// ```lua
/// {
///   title = "string",         -- optional
///   placeholder = "string",   -- optional
///   source = function(ctx),   -- required
///   selection = "single",     -- optional: "single" | "multi" | "custom"
///   on_select = function(ctx),-- optional (required if selection = "custom")
///   on_submit = function(ctx),-- optional
///   view_data = { ... },      -- optional
///   keys = { ... },           -- optional
/// }
/// ```
pub fn parse_view(lua: &Lua, table: Table) -> LuaResult<View> {
    // Generate a unique view key
    let view_key = generate_function_key("view");

    // Optional: title
    let title: Option<String> = table.get("title")?;

    // Optional: placeholder
    let placeholder: Option<String> = table.get("placeholder")?;

    // Required: source function
    let source_fn = table
        .get::<Function>("source")
        .map_err(|_| mlua::Error::RuntimeError("View missing required 'source' function".into()))?;
    let source_fn = store_function(lua, source_fn, &format!("{}:source", view_key))?;

    // Optional: selection mode (default "single")
    let selection = match table.get::<Option<String>>("selection")? {
        Some(s) => match s.as_str() {
            "single" => SelectionMode::Single,
            "multi" => SelectionMode::Multi,
            "custom" => SelectionMode::Custom,
            _ => {
                return Err(mlua::Error::RuntimeError(format!(
                    "Invalid selection mode '{}'. Expected 'single', 'multi', or 'custom'",
                    s
                )))
            }
        },
        None => SelectionMode::Single,
    };

    // Optional: on_select function
    let on_select_fn = match table.get::<Option<Function>>("on_select")? {
        Some(func) => Some(store_function(
            lua,
            func,
            &format!("{}:on_select", view_key),
        )?),
        None => None,
    };

    // Validate: custom selection requires on_select
    if selection == SelectionMode::Custom && on_select_fn.is_none() {
        return Err(mlua::Error::RuntimeError(
            "View with selection='custom' must have 'on_select' function".into(),
        ));
    }

    // Optional: on_submit function
    let on_submit_fn = match table.get::<Option<Function>>("on_submit")? {
        Some(func) => Some(store_function(
            lua,
            func,
            &format!("{}:on_submit", view_key),
        )?),
        None => None,
    };

    // Optional: view_data
    let view_data = match table.get::<Option<Table>>("view_data")? {
        Some(data_table) => lua_value_to_json(lua, Value::Table(data_table))?,
        None => serde_json::Value::Null,
    };

    // Optional: keys
    let keys = match table.get::<Option<Table>>("keys")? {
        Some(keys_table) => parse_key_bindings(lua, &view_key, keys_table)?,
        None => HashMap::new(),
    };

    Ok(View {
        title,
        placeholder,
        source_fn,
        selection,
        on_select_fn,
        on_submit_fn,
        view_data,
        keys,
    })
}

/// Parse key bindings from a table.
///
/// Expected table shape:
/// ```lua
/// {
///   ["ctrl+a"] = function(ctx),  -- function binding
///   ["ctrl+d"] = "action-id",    -- action ID binding
/// }
/// ```
fn parse_key_bindings(
    lua: &Lua,
    view_key: &str,
    table: Table,
) -> LuaResult<HashMap<String, KeyBinding>> {
    let mut bindings = HashMap::new();

    for pair in table.pairs::<String, Value>() {
        let (key, value) = pair?;

        let binding = match value {
            Value::Function(func) => KeyBinding::Function(store_function(
                lua,
                func,
                &format!("{}:key:{}", view_key, key),
            )?),
            Value::String(s) => KeyBinding::ActionId(s.to_str()?.to_string()),
            _ => {
                return Err(mlua::Error::RuntimeError(format!(
                    "Key binding for '{}' must be a function or action ID string",
                    key
                )))
            }
        };

        bindings.insert(key, binding);
    }

    Ok(bindings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_plugin() {
        let lua = Lua::new();

        let result = lua
            .load(
                r#"
            return {
                name = "test-plugin",
            }
        "#,
            )
            .eval::<Table>()
            .unwrap();

        let plugin = parse_plugin(&lua, result).unwrap();
        assert_eq!(plugin.name, "test-plugin");
        assert!(plugin.triggers.is_empty());
        assert!(plugin.sources.is_empty());
        assert!(plugin.actions.is_empty());
        assert!(plugin.setup_fn.is_none());
    }

    #[test]
    fn test_parse_plugin_with_trigger() {
        let lua = Lua::new();

        let result = lua
            .load(
                r#"
            return {
                name = "calc",
                triggers = {
                    {
                        prefix = "=",
                        run = function(ctx) end,
                    },
                },
            }
        "#,
            )
            .eval::<Table>()
            .unwrap();

        let plugin = parse_plugin(&lua, result).unwrap();
        assert_eq!(plugin.name, "calc");
        assert_eq!(plugin.triggers.len(), 1);
        assert_eq!(plugin.triggers[0].prefix, Some("=".to_string()));
    }

    #[test]
    fn test_parse_plugin_with_source() {
        let lua = Lua::new();

        let result = lua
            .load(
                r#"
            return {
                name = "files",
                sources = {
                    {
                        name = "recent",
                        root = true,
                        group = "Recent Files",
                        search = function(ctx) return {} end,
                    },
                },
            }
        "#,
            )
            .eval::<Table>()
            .unwrap();

        let plugin = parse_plugin(&lua, result).unwrap();
        assert_eq!(plugin.name, "files");
        assert_eq!(plugin.sources.len(), 1);
        assert_eq!(plugin.sources[0].name, Some("recent".to_string()));
        assert!(plugin.sources[0].root);
        assert_eq!(plugin.sources[0].group, Some("Recent Files".to_string()));
    }

    #[test]
    fn test_parse_plugin_with_action() {
        let lua = Lua::new();

        let result = lua
            .load(
                r#"
            return {
                name = "files",
                actions = {
                    {
                        id = "open",
                        title = "Open",
                        applies = function(ctx) return true end,
                        run = function(ctx) end,
                    },
                },
            }
        "#,
            )
            .eval::<Table>()
            .unwrap();

        let plugin = parse_plugin(&lua, result).unwrap();
        assert_eq!(plugin.name, "files");
        assert_eq!(plugin.actions.len(), 1);
        assert_eq!(plugin.actions[0].id, "open");
        assert_eq!(plugin.actions[0].title, "Open");
    }

    #[test]
    fn test_parse_trigger_missing_run() {
        let lua = Lua::new();

        let result = lua
            .load(
                r#"
            return {
                name = "bad",
                triggers = {
                    {
                        prefix = ":",
                        -- missing run
                    },
                },
            }
        "#,
            )
            .eval::<Table>()
            .unwrap();

        let err = parse_plugin(&lua, result).unwrap_err();
        assert!(err.to_string().contains("run"));
    }

    #[test]
    fn test_parse_trigger_missing_match_and_prefix() {
        let lua = Lua::new();

        let result = lua
            .load(
                r#"
            return {
                name = "bad",
                triggers = {
                    {
                        run = function(ctx) end,
                        -- missing both match and prefix
                    },
                },
            }
        "#,
            )
            .eval::<Table>()
            .unwrap();

        let err = parse_plugin(&lua, result).unwrap_err();
        assert!(err.to_string().contains("match") || err.to_string().contains("prefix"));
    }
}
