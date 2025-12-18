//! Context builders for Lua hooks.
//!
//! Each hook type in the Plugin API receives a context object with specific fields
//! and methods. This module provides builders to construct these contexts.
//!
//! ## Context Types
//!
//! | Hook | Fields | Methods |
//! |------|--------|---------|
//! | `trigger.match` | query | - |
//! | `trigger.run` | query, args | add_results, push, replace, dismiss |
//! | `source.search` | query, view_data | loading, resolve |
//! | `action.applies` | item | - |
//! | `action.run` | items, view_data | push, replace, pop, dismiss, progress, complete, fail |
//! | `view.on_select` | item, view_data | select, deselect, clear_selection, is_selected, get_selection |
//! | `view.on_submit` | query, view_data | push, replace, pop, dismiss |

use std::collections::HashSet;
use std::sync::Arc;

use mlua::{Lua, Result as LuaResult, Table, Value};
use parking_lot::Mutex;

use super::lua::json_to_lua_value;
use super::types::{Groups, Item, View};

// =============================================================================
// Helper Macro
// =============================================================================

/// Helper macro to reduce boilerplate when creating Lua context methods.
///
/// Creates a Lua function that captures state, locks it, and executes the body.
macro_rules! ctx_method {
    // For methods with no arguments: ctx_method!(lua, ctx, "name", state, |lua, s| { ... })
    ($lua:expr, $ctx:expr, $name:literal, $state:expr, |$lua_param:ident, $s:ident| $body:tt) => {{
        let state = Arc::clone(&$state);
        let func = $lua.create_function(move |$lua_param, ()| {
            let mut $s = state.lock();
            $body
        })?;
        $ctx.set($name, func)?;
    }};

    // For methods with arguments: ctx_method!(lua, ctx, "name", state, |lua, s, args: Type| { ... })
    ($lua:expr, $ctx:expr, $name:literal, $state:expr, |$lua_param:ident, $s:ident, $args:ident : $args_ty:ty| $body:tt) => {{
        let state = Arc::clone(&$state);
        let func = $lua.create_function(move |$lua_param, $args: $args_ty| {
            let mut $s = state.lock();
            $body
        })?;
        $ctx.set($name, func)?;
    }};
}

// =============================================================================
// Engine State (shared state for callbacks)
// =============================================================================

/// State that can be modified by context callbacks.
///
/// This is passed to context builders and captured by closure callbacks.
#[derive(Debug, Default)]
pub struct EngineState {
    /// Results added by triggers via ctx.add_results().
    pub added_results: Groups,

    /// View pushed via ctx.push(), if any.
    pub pushed_view: Option<PushedView>,

    /// Whether ctx.dismiss() was called.
    pub dismissed: bool,

    /// Whether ctx.pop() was called.
    pub popped: bool,

    /// Progress message from ctx.progress().
    pub progress_message: Option<String>,

    /// Completion result from ctx.complete().
    pub completion: Option<CompletionResult>,

    /// Error from ctx.fail().
    pub error: Option<String>,

    /// Loading state for async sources.
    pub loading: bool,

    /// Resolved results from async sources.
    pub resolved_results: Option<Groups>,

    /// Selection changes from view.on_select.
    pub selection_changes: SelectionChanges,
}

/// A view that was pushed via ctx.push().
#[derive(Debug)]
pub struct PushedView {
    pub view: View,
    pub initial_query: Option<String>,
    pub replace: bool,
}

/// Result from ctx.complete().
#[derive(Debug, Clone)]
pub struct CompletionResult {
    pub message: String,
    pub follow_up_actions: Vec<FollowUpAction>,
}

/// A follow-up action from completion.
#[derive(Debug, Clone)]
pub struct FollowUpAction {
    pub title: String,
    pub icon: Option<String>,
}

/// Selection changes from view.on_select hook.
#[derive(Debug, Default)]
pub struct SelectionChanges {
    pub selected: Vec<String>,
    pub deselected: Vec<String>,
    pub cleared: bool,
}

impl EngineState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

// =============================================================================
// Context Builders
// =============================================================================

/// Build a context for trigger.match hook.
///
/// Fields: query
/// Methods: none
pub fn build_trigger_match_context(lua: &Lua, query: &str) -> LuaResult<Table> {
    let ctx = lua.create_table()?;
    ctx.set("query", query)?;
    Ok(ctx)
}

/// Build a context for trigger.run hook.
///
/// Fields: query, args
/// Methods: add_results, push, replace, dismiss
pub fn build_trigger_run_context(
    lua: &Lua,
    query: &str,
    args: &str,
    state: Arc<Mutex<EngineState>>,
) -> LuaResult<Table> {
    let ctx = lua.create_table()?;

    // Fields
    ctx.set("query", query)?;
    ctx.set("args", args)?;

    // ctx.add_results(groups)
    ctx_method!(lua, ctx, "add_results", state, |lua, s, groups: Table| {
        let parsed_groups = parse_groups(lua, groups)?;
        s.added_results.extend(parsed_groups);
        Ok(())
    });

    // ctx.push(view_def)
    ctx_method!(lua, ctx, "push", state, |lua, s, view_def: Table| {
        let view = super::lua::parse_view(lua, view_def)?;
        s.pushed_view = Some(PushedView {
            view,
            initial_query: None,
            replace: false,
        });
        Ok(())
    });

    // ctx.replace(view_def)
    ctx_method!(lua, ctx, "replace", state, |lua, s, view_def: Table| {
        let view = super::lua::parse_view(lua, view_def)?;
        s.pushed_view = Some(PushedView {
            view,
            initial_query: None,
            replace: true,
        });
        Ok(())
    });

    // ctx.dismiss()
    ctx_method!(lua, ctx, "dismiss", state, |_lua, s| {
        s.dismissed = true;
        Ok(())
    });

    Ok(ctx)
}

/// Build a context for source.search hook.
///
/// Fields: query, view_data
/// Methods: loading, resolve
pub fn build_source_search_context(
    lua: &Lua,
    query: &str,
    view_data: &serde_json::Value,
    state: Arc<Mutex<EngineState>>,
) -> LuaResult<Table> {
    let ctx = lua.create_table()?;

    // Fields
    ctx.set("query", query)?;
    ctx.set("view_data", json_to_lua_value(lua, view_data)?)?;

    // ctx.loading()
    ctx_method!(lua, ctx, "loading", state, |_lua, s| {
        s.loading = true;
        Ok(())
    });

    // ctx.resolve(groups)
    ctx_method!(lua, ctx, "resolve", state, |lua, s, groups: Table| {
        let parsed_groups = parse_groups(lua, groups)?;
        s.loading = false;
        s.resolved_results = Some(parsed_groups);
        Ok(())
    });

    Ok(ctx)
}

/// Build a context for action.applies hook.
///
/// Fields: item
/// Methods: none
pub fn build_action_applies_context(lua: &Lua, item: &Item) -> LuaResult<Table> {
    let ctx = lua.create_table()?;
    ctx.set("item", item_to_lua(lua, item)?)?;
    Ok(ctx)
}

/// Build a context for action.run hook.
///
/// Fields: items, view_data
/// Methods: push, replace, pop, dismiss, progress, complete, fail
pub fn build_action_run_context(
    lua: &Lua,
    items: &[Item],
    view_data: &serde_json::Value,
    state: Arc<Mutex<EngineState>>,
) -> LuaResult<Table> {
    let ctx = lua.create_table()?;

    // Fields
    ctx.set("items", items_to_lua(lua, items)?)?;
    ctx.set("view_data", json_to_lua_value(lua, view_data)?)?;

    // Also provide single item for convenience
    if let Some(first) = items.first() {
        ctx.set("item", item_to_lua(lua, first)?)?;
    }

    // ctx.push(view_def)
    ctx_method!(lua, ctx, "push", state, |lua, s, view_def: Table| {
        // Get query first before parse_view consumes the table
        let initial_query: Option<String> = view_def.get("query").ok();
        let view = super::lua::parse_view(lua, view_def)?;
        s.pushed_view = Some(PushedView {
            view,
            initial_query,
            replace: false,
        });
        Ok(())
    });

    // ctx.replace(view_def)
    ctx_method!(lua, ctx, "replace", state, |lua, s, view_def: Table| {
        let view = super::lua::parse_view(lua, view_def)?;
        s.pushed_view = Some(PushedView {
            view,
            initial_query: None,
            replace: true,
        });
        Ok(())
    });

    // ctx.pop()
    ctx_method!(lua, ctx, "pop", state, |_lua, s| {
        s.popped = true;
        Ok(())
    });

    // ctx.dismiss()
    ctx_method!(lua, ctx, "dismiss", state, |_lua, s| {
        s.dismissed = true;
        Ok(())
    });

    // ctx.progress(message)
    ctx_method!(lua, ctx, "progress", state, |_lua, s, message: String| {
        s.progress_message = Some(message);
        Ok(())
    });

    // ctx.complete(message, actions?)
    ctx_method!(lua, ctx, "complete", state, |lua,
                                              s,
                                              args: (
        String,
        Option<Table>
    )| {
        let (message, actions) = args;
        let follow_up_actions = if let Some(actions_table) = actions {
            parse_follow_up_actions(lua, actions_table)?
        } else {
            Vec::new()
        };
        s.completion = Some(CompletionResult {
            message,
            follow_up_actions,
        });
        Ok(())
    });

    // ctx.fail(error)
    ctx_method!(lua, ctx, "fail", state, |_lua, s, error: String| {
        s.error = Some(error);
        Ok(())
    });

    Ok(ctx)
}

/// Build a context for view.on_select hook.
///
/// Fields: item, view_data
/// Methods: select, deselect, clear_selection, is_selected, get_selection
pub fn build_view_select_context(
    lua: &Lua,
    item: &Item,
    view_data: &serde_json::Value,
    current_selection: &HashSet<String>,
    state: Arc<Mutex<EngineState>>,
) -> LuaResult<Table> {
    let ctx = lua.create_table()?;

    // Fields
    ctx.set("item", item_to_lua(lua, item)?)?;
    ctx.set("view_data", json_to_lua_value(lua, view_data)?)?;

    // ctx.select(id)
    ctx_method!(lua, ctx, "select", state, |_lua, s, id: String| {
        s.selection_changes.selected.push(id);
        Ok(())
    });

    // ctx.deselect(id)
    ctx_method!(lua, ctx, "deselect", state, |_lua, s, id: String| {
        s.selection_changes.deselected.push(id);
        Ok(())
    });

    // ctx.clear_selection()
    ctx_method!(lua, ctx, "clear_selection", state, |_lua, s| {
        s.selection_changes.cleared = true;
        Ok(())
    });

    // ctx.is_selected(id) -> bool
    {
        let selection = current_selection.clone();
        let is_selected_fn =
            lua.create_function(move |_lua, id: String| Ok(selection.contains(&id)))?;
        ctx.set("is_selected", is_selected_fn)?;
    }

    // ctx.get_selection() -> array of ids
    {
        let selection: Vec<String> = current_selection.iter().cloned().collect();
        let get_selection_fn = lua.create_function(move |lua, ()| {
            let table = lua.create_table()?;
            for (i, id) in selection.iter().enumerate() {
                table.set(i + 1, id.as_str())?;
            }
            Ok(table)
        })?;
        ctx.set("get_selection", get_selection_fn)?;
    }

    Ok(ctx)
}

/// Build a context for view.on_submit hook.
///
/// Fields: query, view_data
/// Methods: push, replace, pop, dismiss
pub fn build_view_submit_context(
    lua: &Lua,
    query: &str,
    view_data: &serde_json::Value,
    state: Arc<Mutex<EngineState>>,
) -> LuaResult<Table> {
    let ctx = lua.create_table()?;

    // Fields
    ctx.set("query", query)?;
    ctx.set("view_data", json_to_lua_value(lua, view_data)?)?;

    // ctx.push(view_def)
    ctx_method!(lua, ctx, "push", state, |lua, s, view_def: Table| {
        // Get query first before parse_view consumes the table
        let initial_query: Option<String> = view_def.get("query").ok();
        let view = super::lua::parse_view(lua, view_def)?;
        s.pushed_view = Some(PushedView {
            view,
            initial_query,
            replace: false,
        });
        Ok(())
    });

    // ctx.replace(view_def)
    ctx_method!(lua, ctx, "replace", state, |lua, s, view_def: Table| {
        let view = super::lua::parse_view(lua, view_def)?;
        s.pushed_view = Some(PushedView {
            view,
            initial_query: None,
            replace: true,
        });
        Ok(())
    });

    // ctx.pop()
    ctx_method!(lua, ctx, "pop", state, |_lua, s| {
        s.popped = true;
        Ok(())
    });

    // ctx.dismiss()
    ctx_method!(lua, ctx, "dismiss", state, |_lua, s| {
        s.dismissed = true;
        Ok(())
    });

    Ok(ctx)
}

// =============================================================================
// Helper Functions
// =============================================================================

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

    // types array
    let types_table = lua.create_table()?;
    for (i, t) in item.types.iter().enumerate() {
        types_table.set(i + 1, t.as_str())?;
    }
    table.set("types", types_table)?;

    // data
    if let Some(ref data) = item.data {
        table.set("data", json_to_lua_value(lua, data)?)?;
    }

    Ok(table)
}

/// Convert a slice of Items to a Lua table (array).
fn items_to_lua(lua: &Lua, items: &[Item]) -> LuaResult<Table> {
    let table = lua.create_table()?;
    for (i, item) in items.iter().enumerate() {
        table.set(i + 1, item_to_lua(lua, item)?)?;
    }
    Ok(table)
}

/// Parse a Lua table into Groups.
fn parse_groups(lua: &Lua, table: Table) -> LuaResult<Groups> {
    use super::types::Group;

    let mut groups = Vec::new();

    for pair in table.pairs::<i64, Table>() {
        let (_, group_table) = pair?;

        let title: Option<String> = group_table.get("title")?;
        let items_table: Table = group_table.get("items")?;

        let mut items = Vec::new();
        for item_pair in items_table.pairs::<i64, Table>() {
            let (_, item_table) = item_pair?;
            items.push(parse_item(lua, item_table)?);
        }

        groups.push(Group { title, items });
    }

    Ok(groups)
}

/// Parse a Lua table into an Item.
fn parse_item(lua: &Lua, table: Table) -> LuaResult<Item> {
    let id: String = table.get("id")?;
    let title: String = table.get("title")?;
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
        .get::<Option<Value>>("data")?
        .map(|v| super::lua::lua_value_to_json(lua, v))
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

/// Parse follow-up actions from a Lua table.
fn parse_follow_up_actions(_lua: &Lua, table: Table) -> LuaResult<Vec<FollowUpAction>> {
    let mut actions = Vec::new();

    for pair in table.pairs::<i64, Table>() {
        let (_, action_table) = pair?;
        let title: String = action_table.get("title")?;
        let icon: Option<String> = action_table.get("icon")?;
        actions.push(FollowUpAction { title, icon });
    }

    Ok(actions)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_match_context() {
        let lua = Lua::new();
        let ctx = build_trigger_match_context(&lua, "test query").unwrap();

        let query: String = ctx.get("query").unwrap();
        assert_eq!(query, "test query");
    }

    #[test]
    fn test_trigger_run_context_add_results() {
        let lua = Lua::new();
        let state = Arc::new(Mutex::new(EngineState::new()));

        let ctx = build_trigger_run_context(&lua, "test", "args", Arc::clone(&state)).unwrap();

        // Call add_results from Lua
        lua.globals().set("ctx", ctx).unwrap();
        lua.load(
            r#"
            ctx.add_results({
                { title = "Group 1", items = {
                    { id = "1", title = "Item 1" },
                    { id = "2", title = "Item 2" },
                }},
            })
        "#,
        )
        .exec()
        .unwrap();

        let state = state.lock();
        assert_eq!(state.added_results.len(), 1);
        assert_eq!(state.added_results[0].items.len(), 2);
    }

    #[test]
    fn test_trigger_run_context_dismiss() {
        let lua = Lua::new();
        let state = Arc::new(Mutex::new(EngineState::new()));

        let ctx = build_trigger_run_context(&lua, "test", "", Arc::clone(&state)).unwrap();

        lua.globals().set("ctx", ctx).unwrap();
        lua.load("ctx.dismiss()").exec().unwrap();

        let state = state.lock();
        assert!(state.dismissed);
    }

    #[test]
    fn test_source_search_context_loading() {
        let lua = Lua::new();
        let state = Arc::new(Mutex::new(EngineState::new()));

        let ctx = build_source_search_context(
            &lua,
            "query",
            &serde_json::json!({"key": "value"}),
            Arc::clone(&state),
        )
        .unwrap();

        lua.globals().set("ctx", ctx).unwrap();
        lua.load("ctx.loading()").exec().unwrap();

        let state = state.lock();
        assert!(state.loading);
    }

    #[test]
    fn test_action_applies_context() {
        let lua = Lua::new();

        let item = Item {
            id: "test-id".to_string(),
            title: "Test Item".to_string(),
            subtitle: Some("Subtitle".to_string()),
            icon: None,
            types: vec!["file".to_string()],
            data: None,
        };

        let ctx = build_action_applies_context(&lua, &item).unwrap();

        lua.globals().set("ctx", ctx).unwrap();
        let result: String = lua.load("return ctx.item.id").eval().unwrap();
        assert_eq!(result, "test-id");

        let title: String = lua.load("return ctx.item.title").eval().unwrap();
        assert_eq!(title, "Test Item");
    }

    #[test]
    fn test_action_run_context_complete() {
        let lua = Lua::new();
        let state = Arc::new(Mutex::new(EngineState::new()));

        let items = vec![Item {
            id: "1".to_string(),
            title: "Test".to_string(),
            subtitle: None,
            icon: None,
            types: vec![],
            data: None,
        }];

        let ctx =
            build_action_run_context(&lua, &items, &serde_json::Value::Null, Arc::clone(&state))
                .unwrap();

        lua.globals().set("ctx", ctx).unwrap();
        lua.load(r#"ctx.complete("Done!", {{ title = "Undo" }})"#)
            .exec()
            .unwrap();

        let state = state.lock();
        assert!(state.completion.is_some());
        let completion = state.completion.as_ref().unwrap();
        assert_eq!(completion.message, "Done!");
        assert_eq!(completion.follow_up_actions.len(), 1);
    }

    #[test]
    fn test_view_select_context() {
        let lua = Lua::new();
        let state = Arc::new(Mutex::new(EngineState::new()));

        let item = Item {
            id: "item-1".to_string(),
            title: "Item 1".to_string(),
            subtitle: None,
            icon: None,
            types: vec![],
            data: None,
        };

        let mut selection = HashSet::new();
        selection.insert("existing".to_string());

        let ctx = build_view_select_context(
            &lua,
            &item,
            &serde_json::Value::Null,
            &selection,
            Arc::clone(&state),
        )
        .unwrap();

        lua.globals().set("ctx", ctx).unwrap();

        // Test is_selected
        let is_selected: bool = lua
            .load(r#"return ctx.is_selected("existing")"#)
            .eval()
            .unwrap();
        assert!(is_selected);

        let is_not_selected: bool = lua
            .load(r#"return ctx.is_selected("other")"#)
            .eval()
            .unwrap();
        assert!(!is_not_selected);

        // Test select
        lua.load(r#"ctx.select("new-item")"#).exec().unwrap();
        let state = state.lock();
        assert!(state
            .selection_changes
            .selected
            .contains(&"new-item".to_string()));
    }
}
