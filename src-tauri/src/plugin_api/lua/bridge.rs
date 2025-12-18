//! Lua bridge for effect-based execution.
//!
//! This module provides Lua-callable wrappers that delegate to Rust typestate contexts.
//! All effect collection happens through `EffectCollector`, and the engine applies
//! effects after the Lua call completes.

use mlua::{Lua, Result as LuaResult, Table, UserData, UserDataMethods};

use crate::plugin_api::context::{ActionContext, SourceContext, TriggerContext};
use crate::plugin_api::effect::{Effect, EffectCollector, SelectionMode, ViewSpec};
use crate::plugin_api::lua::json_to_lua_value;
use crate::plugin_api::types::{Group, Item};

// =============================================================================
// Lua Wrappers (delegate to Rust contexts)
// =============================================================================

/// Lua-visible wrapper for TriggerContext.
///
/// Delegates all method calls to the inner `TriggerContext`.
pub struct TriggerContextLua<'a> {
    pub inner: TriggerContext<'a>,
}

impl UserData for TriggerContextLua<'_> {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("query", |_, this| Ok(this.inner.query().to_string()));
        fields.add_field_method_get("args", |_, this| Ok(this.inner.args().to_string()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Convenience: wrap items in a single ungrouped group
        methods.add_method("set_items", |lua, this, items: Table| {
            let items = parse_items(lua, items)?;
            this.inner.set_groups(vec![Group { title: None, items }]);
            Ok(())
        });

        // Full control: set groups directly
        methods.add_method("set_groups", |lua, this, groups: Table| {
            let groups = parse_groups(lua, groups)?;
            this.inner.set_groups(groups);
            Ok(())
        });

        methods.add_method("push", |lua, this, view_def: Table| {
            let spec = parse_view_spec(lua, view_def)?;
            this.inner.push_view(spec);
            Ok(())
        });

        methods.add_method("replace", |lua, this, view_def: Table| {
            let spec = parse_view_spec(lua, view_def)?;
            this.inner.replace_view(spec);
            Ok(())
        });

        methods.add_method("dismiss", |_, this, ()| {
            this.inner.dismiss();
            Ok(())
        });
    }
}

/// Lua-visible wrapper for SourceContext.
pub struct SourceContextLua<'a> {
    pub inner: SourceContext<'a>,
}

impl UserData for SourceContextLua<'_> {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("query", |_, this| Ok(this.inner.query().to_string()));
        fields.add_field_method_get("view_data", |lua, this| {
            json_to_lua_value(lua, this.inner.view_data())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Convenience: wrap items in a single ungrouped group
        methods.add_method("set_items", |lua, this, items: Table| {
            let items = parse_items(lua, items)?;
            this.inner.set_groups(vec![Group { title: None, items }]);
            Ok(())
        });

        // Full control: set groups directly
        methods.add_method("set_groups", |lua, this, groups: Table| {
            let groups = parse_groups(lua, groups)?;
            this.inner.set_groups(groups);
            Ok(())
        });

        // Note: No push, replace, dismiss - sources just return items
    }
}

/// Lua-visible wrapper for ActionContext.
pub struct ActionContextLua<'a> {
    pub inner: ActionContext<'a>,
}

impl UserData for ActionContextLua<'_> {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("items", |lua, this| items_to_lua(lua, this.inner.items()));
        fields.add_field_method_get("item", |lua, this| match this.inner.item() {
            Some(item) => Ok(Some(item_to_lua(lua, item)?)),
            None => Ok(None),
        });
        fields.add_field_method_get("view_data", |lua, this| {
            json_to_lua_value(lua, this.inner.view_data())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("push", |lua, this, view_def: Table| {
            let spec = parse_view_spec(lua, view_def)?;
            this.inner.push_view(spec);
            Ok(())
        });

        methods.add_method("replace", |lua, this, view_def: Table| {
            let spec = parse_view_spec(lua, view_def)?;
            this.inner.replace_view(spec);
            Ok(())
        });

        methods.add_method("pop", |_, this, ()| {
            this.inner.pop();
            Ok(())
        });

        methods.add_method("dismiss", |_, this, ()| {
            this.inner.dismiss();
            Ok(())
        });

        methods.add_method("progress", |_, this, message: String| {
            this.inner.progress(message);
            Ok(())
        });

        methods.add_method("complete", |_, this, message: String| {
            this.inner.complete(message);
            Ok(())
        });

        methods.add_method("fail", |_, this, error: String| {
            this.inner.fail(error);
            Ok(())
        });

        // Note: No set_items - actions consume items, don't produce them
    }
}

// =============================================================================
// Execution Functions
// =============================================================================

/// Call a trigger's run function using effect-based execution.
///
/// Returns the collected effects for the engine to apply.
pub fn call_trigger_run(lua: &Lua, run_fn_key: &str, query: &str, args: &str) -> LuaResult<Vec<Effect>> {
    let collector = EffectCollector::new();

    // Use lua.scope to create a context with the collector reference
    lua.scope(|scope| {
        let ctx = TriggerContext::new(query, args, &collector);
        let wrapper = scope.create_userdata(TriggerContextLua { inner: ctx })?;

        // Get the function directly from named registry
        let func: mlua::Function = lua.named_registry_value(run_fn_key)?;
        func.call::<()>(wrapper)?;
        Ok(())
    })?;

    Ok(collector.take())
}

/// Call a source's search function using effect-based execution.
///
/// Returns the collected effects for the engine to apply.
pub fn call_source_search(
    lua: &Lua,
    search_fn_key: &str,
    query: &str,
    view_data: &serde_json::Value,
) -> LuaResult<Vec<Effect>> {
    let collector = EffectCollector::new();

    lua.scope(|scope| {
        let ctx = SourceContext::new(query, view_data, &collector);
        let wrapper = scope.create_userdata(SourceContextLua { inner: ctx })?;

        let func: mlua::Function = lua.named_registry_value(search_fn_key)?;
        func.call::<()>(wrapper)?;
        Ok(())
    })?;

    Ok(collector.take())
}

/// Call an action's run function using effect-based execution.
///
/// Returns the collected effects for the engine to apply.
pub fn call_action_run(
    lua: &Lua,
    run_fn_key: &str,
    items: &[Item],
    view_data: &serde_json::Value,
) -> LuaResult<Vec<Effect>> {
    let collector = EffectCollector::new();

    lua.scope(|scope| {
        let ctx = ActionContext::new(items, view_data, &collector);
        let wrapper = scope.create_userdata(ActionContextLua { inner: ctx })?;

        let func: mlua::Function = lua.named_registry_value(run_fn_key)?;
        func.call::<()>(wrapper)?;
        Ok(())
    })?;

    Ok(collector.take())
}

/// Lua-visible wrapper for SelectContext.
pub struct SelectContextLua<'a> {
    pub inner: crate::plugin_api::context::SelectContext<'a>,
}

impl UserData for SelectContextLua<'_> {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("item", |lua, this| item_to_lua(lua, this.inner.item()));
        fields.add_field_method_get("view_data", |lua, this| {
            json_to_lua_value(lua, this.inner.view_data())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("select", |_, this, id: String| {
            this.inner.select(id);
            Ok(())
        });

        methods.add_method("deselect", |_, this, id: String| {
            this.inner.deselect(id);
            Ok(())
        });

        methods.add_method("clear_selection", |_, this, ()| {
            this.inner.clear_selection();
            Ok(())
        });

        methods.add_method("is_selected", |_, this, id: String| {
            Ok(this.inner.is_selected(&id))
        });

        methods.add_method("get_selection", |lua, this, ()| {
            let selection = this.inner.get_selection();
            let table = lua.create_table()?;
            for (i, id) in selection.iter().enumerate() {
                table.set(i + 1, id.as_str())?;
            }
            Ok(table)
        });
    }
}

/// Call a view's on_select function using effect-based execution.
///
/// Returns the collected effects for the engine to apply.
pub fn call_view_on_select(
    lua: &Lua,
    on_select_fn_key: &str,
    item: &Item,
    view_data: &serde_json::Value,
    current_selection: &std::collections::HashSet<String>,
) -> LuaResult<Vec<Effect>> {
    let collector = EffectCollector::new();

    lua.scope(|scope| {
        let ctx =
            crate::plugin_api::context::SelectContext::new(item, view_data, current_selection, &collector);
        let wrapper = scope.create_userdata(SelectContextLua { inner: ctx })?;

        let func: mlua::Function = lua.named_registry_value(on_select_fn_key)?;
        func.call::<()>(wrapper)?;
        Ok(())
    })?;

    Ok(collector.take())
}

/// Lua-visible wrapper for SubmitContext.
pub struct SubmitContextLua<'a> {
    pub inner: crate::plugin_api::context::SubmitContext<'a>,
}

impl UserData for SubmitContextLua<'_> {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("query", |_, this| Ok(this.inner.query().to_string()));
        fields.add_field_method_get("view_data", |lua, this| {
            json_to_lua_value(lua, this.inner.view_data())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("push", |lua, this, view_def: Table| {
            let spec = parse_view_spec(lua, view_def)?;
            this.inner.push_view(spec);
            Ok(())
        });

        methods.add_method("replace", |lua, this, view_def: Table| {
            let spec = parse_view_spec(lua, view_def)?;
            this.inner.replace_view(spec);
            Ok(())
        });

        methods.add_method("pop", |_, this, ()| {
            this.inner.pop();
            Ok(())
        });

        methods.add_method("dismiss", |_, this, ()| {
            this.inner.dismiss();
            Ok(())
        });
    }
}

/// Call a view's on_submit function using effect-based execution.
///
/// Returns the collected effects for the engine to apply.
pub fn call_view_on_submit(
    lua: &Lua,
    on_submit_fn_key: &str,
    query: &str,
    view_data: &serde_json::Value,
) -> LuaResult<Vec<Effect>> {
    let collector = EffectCollector::new();

    lua.scope(|scope| {
        let ctx = crate::plugin_api::context::SubmitContext::new(query, view_data, &collector);
        let wrapper = scope.create_userdata(SubmitContextLua { inner: ctx })?;

        let func: mlua::Function = lua.named_registry_value(on_submit_fn_key)?;
        func.call::<()>(wrapper)?;
        Ok(())
    })?;

    Ok(collector.take())
}

// =============================================================================
// Parsing Helpers
// =============================================================================

/// Parse a ViewSpec from a Lua table.
///
/// Uses inline source functions stored in Lua registry.
fn parse_view_spec(lua: &Lua, table: Table) -> LuaResult<ViewSpec> {
    let title: Option<String> = table.get("title")?;
    let placeholder: Option<String> = table.get("placeholder")?;

    // Get the source function and store it directly in registry
    let source_fn: mlua::Function = table.get("source").map_err(|e| {
        mlua::Error::RuntimeError(format!("ViewSpec requires 'source' function: {}", e))
    })?;
    let source_key = format!("view:source:{}", uuid::Uuid::new_v4());
    lua.set_named_registry_value(&source_key, source_fn)?;

    // Parse selection mode
    let selection_mode = match table.get::<Option<String>>("selection")? {
        Some(s) => match s.as_str() {
            "single" => SelectionMode::Single,
            "multi" => SelectionMode::Multi,
            "custom" => SelectionMode::Custom,
            _ => SelectionMode::Single,
        },
        None => SelectionMode::Single,
    };

    // Parse on_select callback
    let on_select_fn_key = match table.get::<Option<mlua::Function>>("on_select")? {
        Some(func) => {
            let key = format!("view:on_select:{}", uuid::Uuid::new_v4());
            lua.set_named_registry_value(&key, func)?;
            Some(key)
        }
        None => None,
    };

    // Parse on_submit callback
    let on_submit_fn_key = match table.get::<Option<mlua::Function>>("on_submit")? {
        Some(func) => {
            let key = format!("view:on_submit:{}", uuid::Uuid::new_v4());
            lua.set_named_registry_value(&key, func)?;
            Some(key)
        }
        None => None,
    };

    // Parse view_data
    let view_data = match table.get::<Option<Table>>("view_data")? {
        Some(data_table) => super::lua_value_to_json(lua, mlua::Value::Table(data_table))?,
        None => serde_json::Value::Null,
    };

    let mut spec = ViewSpec::new(source_key)
        .with_selection_mode(selection_mode)
        .with_view_data(view_data);

    if let Some(t) = title {
        spec = spec.with_title(t);
    }
    if let Some(p) = placeholder {
        spec = spec.with_placeholder(p);
    }
    if let Some(k) = on_select_fn_key {
        spec = spec.with_on_select(k);
    }
    if let Some(k) = on_submit_fn_key {
        spec = spec.with_on_submit(k);
    }

    Ok(spec)
}

/// Parse items from a Lua table.
fn parse_items(lua: &Lua, table: Table) -> LuaResult<Vec<Item>> {
    let mut items = Vec::new();

    for pair in table.pairs::<i64, Table>() {
        let (_, item_table) = pair?;
        items.push(parse_item(lua, item_table)?);
    }

    Ok(items)
}

/// Parse groups from a Lua table.
fn parse_groups(lua: &Lua, table: Table) -> LuaResult<Vec<Group>> {
    let mut groups = Vec::new();

    for pair in table.pairs::<i64, Table>() {
        let (_, group_table) = pair?;

        let title: Option<String> = group_table.get("title")?;
        let items_table: Table = group_table.get("items").map_err(|e| {
            mlua::Error::RuntimeError(format!("Group requires 'items' field: {}", e))
        })?;
        let items = parse_items(lua, items_table)?;

        groups.push(Group { title, items });
    }

    Ok(groups)
}

/// Parse a single item from a Lua table.
fn parse_item(lua: &Lua, table: Table) -> LuaResult<Item> {
    let id: String = table
        .get::<Option<String>>("id")?
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let title: String = table.get("title").map_err(|e| {
        mlua::Error::RuntimeError(format!("Item requires 'title' field: {}", e))
    })?;

    let subtitle: Option<String> = table.get("subtitle")?;
    let icon: Option<String> = table.get("icon")?;

    let types: Vec<String> = table
        .get::<Option<Table>>("types")?
        .map(|t| {
            t.pairs::<i64, String>()
                .filter_map(|r| r.ok().map(|(_, v)| v))
                .collect()
        })
        .unwrap_or_default();

    let data: Option<serde_json::Value> = table
        .get::<Option<mlua::Value>>("data")?
        .map(|v| super::lua_value_to_json(lua, v))
        .transpose()?;

    Ok(Item {
        id,
        title,
        subtitle,
        icon,
        types,
        data,
    })
}

/// Convert an Item to a Lua table.
fn item_to_lua(lua: &Lua, item: &Item) -> LuaResult<Table> {
    let table = lua.create_table()?;
    table.set("id", item.id.as_str())?;
    table.set("title", item.title.as_str())?;

    if let Some(ref subtitle) = item.subtitle {
        table.set("subtitle", subtitle.as_str())?;
    }

    if let Some(ref icon) = item.icon {
        table.set("icon", icon.as_str())?;
    }

    let types_table = lua.create_table()?;
    for (i, t) in item.types.iter().enumerate() {
        types_table.set(i + 1, t.as_str())?;
    }
    table.set("types", types_table)?;

    if let Some(ref data) = item.data {
        table.set("data", json_to_lua_value(lua, data)?)?;
    }

    Ok(table)
}

/// Convert a slice of Items to a Lua table.
fn items_to_lua(lua: &Lua, items: &[Item]) -> LuaResult<Table> {
    let table = lua.create_table()?;
    for (i, item) in items.iter().enumerate() {
        table.set(i + 1, item_to_lua(lua, item)?)?;
    }
    Ok(table)
}

/// Clean up registry keys for a view.
///
/// Call this when popping a view to prevent memory leaks.
pub fn cleanup_view_registry_keys(lua: &Lua, keys: &[String]) {
    for key in keys {
        // Set to nil to remove from registry
        let _ = lua.set_named_registry_value(key, mlua::Value::Nil);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_view_spec() {
        let lua = Lua::new();

        let table = lua
            .load(
                r#"
            return {
                title = "Test View",
                placeholder = "Search...",
                source = function(ctx) return {} end,
                selection = "multi",
            }
        "#,
            )
            .eval::<Table>()
            .unwrap();

        let spec = parse_view_spec(&lua, table).unwrap();
        assert_eq!(spec.title, Some("Test View".to_string()));
        assert_eq!(spec.placeholder, Some("Search...".to_string()));
        assert_eq!(spec.selection_mode, SelectionMode::Multi);
        // Registry keys should be tracked
        assert_eq!(spec.registry_keys.len(), 1); // Just source
    }

    #[test]
    fn test_parse_view_spec_with_callbacks() {
        let lua = Lua::new();

        let table = lua
            .load(
                r#"
            return {
                title = "Test",
                source = function(ctx) return {} end,
                on_select = function(ctx) end,
                on_submit = function(ctx) end,
            }
        "#,
            )
            .eval::<Table>()
            .unwrap();

        let spec = parse_view_spec(&lua, table).unwrap();
        // Registry keys should track source + on_select + on_submit
        assert_eq!(spec.registry_keys.len(), 3);
    }

    #[test]
    fn test_parse_items() {
        let lua = Lua::new();

        let table = lua
            .load(
                r#"
            return {
                { id = "1", title = "Item 1", types = {"file"} },
                { id = "2", title = "Item 2", subtitle = "Sub" },
            }
        "#,
            )
            .eval::<Table>()
            .unwrap();

        let items = parse_items(&lua, table).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "1");
        assert_eq!(items[0].title, "Item 1");
        assert_eq!(items[0].types, vec!["file"]);
        assert_eq!(items[1].subtitle, Some("Sub".to_string()));
    }

    #[test]
    fn test_parse_view_spec_missing_source() {
        let lua = Lua::new();

        let table = lua
            .load(
                r#"
            return {
                title = "No Source",
            }
        "#,
            )
            .eval::<Table>()
            .unwrap();

        let err = parse_view_spec(&lua, table).unwrap_err();
        assert!(err.to_string().contains("source"));
    }
}
