//! Lua table parsing for Plugin API types.
//!
//! This module converts Lua tables into Rust types for views.

use std::sync::atomic::{AtomicU64, Ordering};

use mlua::{Function, Lua, Result as LuaResult, Table, Value};

use crate::types::{LuaFunctionRef, View};
use crate::views::ViewDefinition;
use lux_core::SelectionMode;

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

/// Parse a view definition (for lux.set_root or ctx:push).
///
/// Expected table shape:
/// ```lua
/// {
///   id = "string",            -- optional: stable view identifier
///   title = "string",         -- optional
///   placeholder = "string",   -- optional
///   search = function(query, ctx), -- required
///   selection = "single",     -- optional: "single" | "multi" | "custom"
///   on_select = function(ctx),-- optional (required if selection = "custom")
///   on_submit = function(ctx),-- optional
///   view_data = { ... },      -- optional
/// }
/// ```
pub fn parse_view(lua: &Lua, table: Table) -> LuaResult<View> {
    // Generate a unique view key for function storage
    let view_key = generate_function_key("view");

    // Optional: id (stable view identifier)
    let id: Option<String> = table.get("id")?;

    // Optional: title
    let title: Option<String> = table.get("title")?;

    // Optional: placeholder
    let placeholder: Option<String> = table.get("placeholder")?;

    // Required: search function (accepts both 'search' and 'source' for compatibility)
    let search_fn = table
        .get::<Function>("search")
        .or_else(|_| table.get::<Function>("source"))
        .map_err(|_| mlua::Error::RuntimeError("View missing required 'search' function".into()))?;
    let source_fn = store_function(lua, search_fn, &format!("{}:search", view_key))?;

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

    // Optional: get_actions function
    let get_actions_fn = match table.get::<Option<Function>>("get_actions")? {
        Some(func) => Some(store_function(
            lua,
            func,
            &format!("{}:get_actions", view_key),
        )?),
        None => None,
    };

    // Optional: view_data
    let view_data = match table.get::<Option<Table>>("view_data")? {
        Some(data_table) => lua_value_to_json(lua, Value::Table(data_table))?,
        None => serde_json::Value::Null,
    };

    Ok(View {
        id,
        title,
        placeholder,
        source_fn,
        get_actions_fn,
        selection,
        on_select_fn,
        on_submit_fn,
        view_data,
    })
}

/// Parse a view definition for the new API (lux.views.add).
///
/// Expected table shape:
/// ```lua
/// {
///   id = "string",              -- required: unique view identifier
///   title = "string",           -- optional: displayed in view header
///   placeholder = "string",     -- optional: input hint
///   selection = "single",       -- optional: "single" | "multi"
///   search = function(query, ctx),    -- required: returns items
///   get_actions = function(item, ctx),-- required: returns actions
/// }
/// ```
pub fn parse_view_definition(lua: &Lua, table: Table) -> LuaResult<ViewDefinition> {
    // Required: id
    let id: String = table
        .get("id")
        .map_err(|_| mlua::Error::RuntimeError("View missing required 'id' field".into()))?;

    // Optional: title
    let title: Option<String> = table.get("title")?;

    // Optional: placeholder
    let placeholder: Option<String> = table.get("placeholder")?;

    // Optional: selection mode (default "single")
    let selection = match table.get::<Option<String>>("selection")? {
        Some(s) => match s.as_str() {
            "single" => SelectionMode::Single,
            "multi" => SelectionMode::Multi,
            _ => {
                return Err(mlua::Error::RuntimeError(format!(
                    "Invalid selection mode '{}'. Expected 'single' or 'multi'",
                    s
                )))
            }
        },
        None => SelectionMode::Single,
    };

    // Required: search function
    let search_fn = table
        .get::<Function>("search")
        .map_err(|_| mlua::Error::RuntimeError("View missing required 'search' function".into()))?;
    let search_fn = store_function(lua, search_fn, &format!("view:{}:search", id))?;

    // Required: get_actions function
    let get_actions_fn = table.get::<Function>("get_actions").map_err(|_| {
        mlua::Error::RuntimeError("View missing required 'get_actions' function".into())
    })?;
    let get_actions_fn = store_function(lua, get_actions_fn, &format!("view:{}:get_actions", id))?;

    tracing::debug!(
        "Parsed view definition '{}': title={:?}, placeholder={:?}, selection={:?}",
        id,
        title,
        placeholder,
        selection
    );

    Ok(ViewDefinition {
        id,
        title,
        placeholder,
        selection,
        search_fn,
        get_actions_fn,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_view() {
        let lua = Lua::new();

        let result = lua
            .load(
                r#"
            return {
                title = "Test View",
                source = function(ctx) return {} end,
            }
        "#,
            )
            .eval::<Table>()
            .unwrap();

        let view = parse_view(&lua, result).unwrap();
        assert_eq!(view.title, Some("Test View".to_string()));
        assert_eq!(view.selection, SelectionMode::Single);
    }

    #[test]
    fn test_parse_view_missing_search() {
        let lua = Lua::new();

        let result = lua
            .load(
                r#"
            return {
                title = "Test View",
            }
        "#,
            )
            .eval::<Table>()
            .unwrap();

        let err = parse_view(&lua, result).unwrap_err();
        assert!(err.to_string().contains("search"));
    }

    #[test]
    fn test_parse_view_definition() {
        let lua = Lua::new();

        let result = lua
            .load(
                r#"
            return {
                id = "test-view",
                title = "Test View",
                search = function(query, ctx) return {} end,
                get_actions = function(item, ctx) return {} end,
            }
        "#,
            )
            .eval::<Table>()
            .unwrap();

        let view_def = parse_view_definition(&lua, result).unwrap();
        assert_eq!(view_def.id, "test-view");
        assert_eq!(view_def.title, Some("Test View".to_string()));
    }

    #[test]
    fn test_parse_view_definition_missing_id() {
        let lua = Lua::new();

        let result = lua
            .load(
                r#"
            return {
                search = function(query, ctx) return {} end,
                get_actions = function(item, ctx) return {} end,
            }
        "#,
            )
            .eval::<Table>()
            .unwrap();

        let err = parse_view_definition(&lua, result).unwrap_err();
        assert!(err.to_string().contains("id"));
    }
}
