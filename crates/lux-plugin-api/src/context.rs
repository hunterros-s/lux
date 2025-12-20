//! Context builders for Lua hooks.
//!
//! This module provides:
//! - Table-based contexts for simple hooks (match, applies)
//! - Typestate contexts for effect-based execution (run, search, select, submit)
//!
//! ## Context Types
//!
//! | Hook | Context Type | Methods |
//! |------|--------------|---------|
//! | `trigger.match` | Table | query (field only) |
//! | `trigger.run` | TriggerContext | set_groups, push_view, replace_view, dismiss |
//! | `source.search` | SourceContext | set_groups |
//! | `action.applies` | Table | item (field only) |
//! | `action.run` | ActionContext | push_view, replace_view, pop, dismiss, progress, complete, fail |
//! | `view.on_select` | SelectContext | select, deselect, clear_selection, is_selected, get_selection |
//! | `view.on_submit` | SubmitContext | push_view, replace_view, pop, dismiss |

use std::collections::HashSet;

use mlua::{Lua, Result as LuaResult, Table};

use crate::effect::{Effect, EffectCollector, ViewSpec};
use crate::lua::json_to_lua_value;
use lux_core::{Group, Item};

// =============================================================================
// Table-Based Context Builders (for simple hooks)
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

/// Build a context for action.applies hook.
///
/// Fields: item
/// Methods: none
pub fn build_action_applies_context(lua: &Lua, item: &Item) -> LuaResult<Table> {
    let ctx = lua.create_table()?;
    ctx.set("item", item_to_lua(lua, item)?)?;
    Ok(ctx)
}

// =============================================================================
// Helpers
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

// =============================================================================
// Typestate Contexts (for effect-based execution)
// =============================================================================

/// Context for trigger.run callbacks.
///
/// Can: set_groups, push_view, replace_view, dismiss
/// Cannot: pop, progress, complete, fail (those are for actions)
pub struct TriggerContext<'a> {
    query: &'a str,
    args: &'a str,
    effects: &'a EffectCollector,
}

impl<'a> TriggerContext<'a> {
    /// Create a new trigger context.
    pub fn new(query: &'a str, args: &'a str, effects: &'a EffectCollector) -> Self {
        Self {
            query,
            args,
            effects,
        }
    }

    /// Get the query string.
    pub fn query(&self) -> &str {
        self.query
    }

    /// Get the arguments (portion after trigger prefix).
    pub fn args(&self) -> &str {
        self.args
    }

    /// Set grouped results.
    pub fn set_groups(&self, groups: Vec<Group>) {
        self.effects.push(Effect::SetGroups(groups));
    }

    /// Push a new view onto the stack.
    pub fn push_view(&self, spec: ViewSpec) {
        self.effects.push(Effect::PushView(spec));
    }

    /// Replace the current view.
    pub fn replace_view(&self, spec: ViewSpec) {
        self.effects.push(Effect::ReplaceView(spec));
    }

    /// Dismiss the launcher.
    pub fn dismiss(&self) {
        self.effects.push(Effect::Dismiss);
    }
}

/// Context for source.search callbacks.
///
/// Can: set_groups
/// Cannot: push_view, pop, dismiss (sources just return items)
pub struct SourceContext<'a> {
    query: &'a str,
    view_data: &'a serde_json::Value,
    effects: &'a EffectCollector,
}

impl<'a> SourceContext<'a> {
    /// Create a new source context.
    pub fn new(
        query: &'a str,
        view_data: &'a serde_json::Value,
        effects: &'a EffectCollector,
    ) -> Self {
        Self {
            query,
            view_data,
            effects,
        }
    }

    /// Get the query string.
    pub fn query(&self) -> &str {
        self.query
    }

    /// Get the view data.
    pub fn view_data(&self) -> &serde_json::Value {
        self.view_data
    }

    /// Set grouped results.
    pub fn set_groups(&self, groups: Vec<Group>) {
        self.effects.push(Effect::SetGroups(groups));
    }

    // Note: No push_view, pop, dismiss - sources just return items
}

/// Context for action.run callbacks.
///
/// Can: push_view, replace_view, pop, dismiss, progress, complete, fail
/// Cannot: set_groups (actions operate on items, don't produce them)
pub struct ActionContext<'a> {
    items: &'a [Item],
    view_data: &'a serde_json::Value,
    effects: &'a EffectCollector,
}

impl<'a> ActionContext<'a> {
    /// Create a new action context.
    pub fn new(
        items: &'a [Item],
        view_data: &'a serde_json::Value,
        effects: &'a EffectCollector,
    ) -> Self {
        Self {
            items,
            view_data,
            effects,
        }
    }

    /// Get the items the action is operating on.
    pub fn items(&self) -> &[Item] {
        self.items
    }

    /// Get the first item (convenience method).
    pub fn item(&self) -> Option<&Item> {
        self.items.first()
    }

    /// Get the view data.
    pub fn view_data(&self) -> &serde_json::Value {
        self.view_data
    }

    /// Push a new view onto the stack.
    pub fn push_view(&self, spec: ViewSpec) {
        self.effects.push(Effect::PushView(spec));
    }

    /// Replace the current view.
    pub fn replace_view(&self, spec: ViewSpec) {
        self.effects.push(Effect::ReplaceView(spec));
    }

    /// Pop the current view.
    pub fn pop(&self) {
        self.effects.push(Effect::Pop);
    }

    /// Dismiss the launcher.
    pub fn dismiss(&self) {
        self.effects.push(Effect::Dismiss);
    }

    /// Report progress for a long-running operation.
    pub fn progress(&self, message: impl Into<String>) {
        self.effects.push(Effect::Progress(message.into()));
    }

    /// Mark the action as complete.
    pub fn complete(&self, message: impl Into<String>) {
        self.effects.push(Effect::Complete {
            message: message.into(),
        });
    }

    /// Mark the action as failed.
    pub fn fail(&self, error: impl Into<String>) {
        self.effects.push(Effect::Fail {
            error: error.into(),
        });
    }

    // Note: No set_groups - actions consume items, don't produce them
}

/// Context for view.on_select callbacks.
///
/// Can: select, deselect, clear_selection
/// Read-only: item, view_data, is_selected, get_selection
pub struct SelectContext<'a> {
    item: &'a Item,
    view_data: &'a serde_json::Value,
    current_selection: &'a HashSet<String>,
    effects: &'a EffectCollector,
}

impl<'a> SelectContext<'a> {
    /// Create a new select context.
    pub fn new(
        item: &'a Item,
        view_data: &'a serde_json::Value,
        current_selection: &'a HashSet<String>,
        effects: &'a EffectCollector,
    ) -> Self {
        Self {
            item,
            view_data,
            current_selection,
            effects,
        }
    }

    /// Get the item that was selected.
    pub fn item(&self) -> &Item {
        self.item
    }

    /// Get the view data.
    pub fn view_data(&self) -> &serde_json::Value {
        self.view_data
    }

    /// Check if an item is currently selected.
    pub fn is_selected(&self, id: &str) -> bool {
        self.current_selection.contains(id)
    }

    /// Get the current selection as a vector of IDs.
    pub fn get_selection(&self) -> Vec<String> {
        self.current_selection.iter().cloned().collect()
    }

    /// Select an item by ID.
    pub fn select(&self, id: impl Into<String>) {
        self.effects.push(Effect::Select(vec![id.into()]));
    }

    /// Deselect an item by ID.
    pub fn deselect(&self, id: impl Into<String>) {
        self.effects.push(Effect::Deselect(vec![id.into()]));
    }

    /// Clear all selection.
    pub fn clear_selection(&self) {
        self.effects.push(Effect::ClearSelection);
    }
}

/// Context for view.on_submit callbacks.
///
/// Can: push_view, replace_view, pop, dismiss
pub struct SubmitContext<'a> {
    query: &'a str,
    view_data: &'a serde_json::Value,
    effects: &'a EffectCollector,
}

impl<'a> SubmitContext<'a> {
    /// Create a new submit context.
    pub fn new(
        query: &'a str,
        view_data: &'a serde_json::Value,
        effects: &'a EffectCollector,
    ) -> Self {
        Self {
            query,
            view_data,
            effects,
        }
    }

    /// Get the current query.
    pub fn query(&self) -> &str {
        self.query
    }

    /// Get the view data.
    pub fn view_data(&self) -> &serde_json::Value {
        self.view_data
    }

    /// Push a new view onto the stack.
    pub fn push_view(&self, spec: ViewSpec) {
        self.effects.push(Effect::PushView(spec));
    }

    /// Replace the current view.
    pub fn replace_view(&self, spec: ViewSpec) {
        self.effects.push(Effect::ReplaceView(spec));
    }

    /// Pop the current view.
    pub fn pop(&self) {
        self.effects.push(Effect::Pop);
    }

    /// Dismiss the launcher.
    pub fn dismiss(&self) {
        self.effects.push(Effect::Dismiss);
    }
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
    fn test_trigger_context_collects_effects() {
        let collector = EffectCollector::new();
        let ctx = TriggerContext::new("query", "args", &collector);

        ctx.set_groups(vec![Group {
            title: None,
            items: vec![],
        }]);
        ctx.dismiss();

        let effects = collector.take();
        assert_eq!(effects.len(), 2);
        assert!(matches!(effects[0], Effect::SetGroups(_)));
        assert!(matches!(effects[1], Effect::Dismiss));
    }

    #[test]
    fn test_source_context_limited_methods() {
        let collector = EffectCollector::new();
        let view_data = serde_json::Value::Null;
        let ctx = SourceContext::new("query", &view_data, &collector);

        // Can set groups
        ctx.set_groups(vec![]);

        // Note: No push_view method exists - compile-time enforcement
        let effects = collector.take();
        assert_eq!(effects.len(), 1);
    }

    #[test]
    fn test_action_context_has_all_navigation() {
        let collector = EffectCollector::new();
        let view_data = serde_json::Value::Null;
        let items = vec![];
        let ctx = ActionContext::new(&items, &view_data, &collector);

        ctx.push_view(ViewSpec::new("test".to_string()));
        ctx.pop();
        ctx.dismiss();
        ctx.progress("working...");
        ctx.complete("done!");

        let effects = collector.take();
        assert_eq!(effects.len(), 5);
    }

    #[test]
    fn test_select_context_collects_effects() {
        let collector = EffectCollector::new();
        let item = Item {
            id: "item1".to_string(),
            title: "Test Item".to_string(),
            subtitle: None,
            icon: None,
            types: vec![],
            data: None,
        };
        let view_data = serde_json::Value::Null;
        let selection = HashSet::new();
        let ctx = SelectContext::new(&item, &view_data, &selection, &collector);

        ctx.select("item1");
        ctx.deselect("item2");
        ctx.clear_selection();

        let effects = collector.take();
        assert_eq!(effects.len(), 3);
        assert!(matches!(effects[0], Effect::Select(_)));
        assert!(matches!(effects[1], Effect::Deselect(_)));
        assert!(matches!(effects[2], Effect::ClearSelection));
    }

    #[test]
    fn test_submit_context_navigation() {
        let collector = EffectCollector::new();
        let view_data = serde_json::Value::Null;
        let ctx = SubmitContext::new("query", &view_data, &collector);

        ctx.push_view(ViewSpec::new("test".to_string()));
        ctx.pop();
        ctx.dismiss();

        let effects = collector.take();
        assert_eq!(effects.len(), 3);
    }
}
