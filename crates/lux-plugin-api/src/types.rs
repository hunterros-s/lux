//! Core types for the Lux Plugin API.
//!
//! This module defines the data structures that match the Plugin API Specification v0.1.
//! Common types (Item, Group, SelectionMode, ActionResult) are re-exported from lux_core.

use mlua::{Function, Lua, Result as LuaResult};
use serde::{Deserialize, Serialize};

// Re-export common types from lux-core
pub use lux_core::{ActionResult, FollowUpAction, Group, Groups, Item, SelectionMode};

// =============================================================================
// Lua Function Reference
// =============================================================================

/// A reference to a Lua function stored in the registry.
///
/// Lua functions cannot be stored directly as they reference the Lua state.
/// Instead, we store a string key that can be used to retrieve the function
/// from Lua's named registry.
#[derive(Debug, Clone)]
pub struct LuaFunctionRef {
    /// The key used to look up this function in Lua's named registry.
    pub key: String,
}

impl LuaFunctionRef {
    /// Create a new function reference with a key.
    pub fn new(key: String) -> Self {
        Self { key }
    }

    /// Store a function in Lua's registry and create a reference to it.
    pub fn from_function(lua: &Lua, func: Function, key: String) -> LuaResult<Self> {
        let registry_key = lua.create_registry_value(func)?;
        lua.set_named_registry_value(&key, registry_key)?;
        Ok(Self { key })
    }

    /// Retrieve the function from the registry and call it.
    pub fn call<A, R>(&self, lua: &Lua, args: A) -> LuaResult<R>
    where
        A: mlua::IntoLuaMulti,
        R: mlua::FromLuaMulti,
    {
        let registry_key = lua.named_registry_value::<mlua::RegistryKey>(&self.key)?;
        let func: Function = lua.registry_value(&registry_key)?;
        func.call(args)
    }

    /// Remove the function from the registry.
    /// Call this when the plugin is unregistered to prevent memory leaks.
    pub fn cleanup(&self, lua: &Lua) -> LuaResult<()> {
        if let Ok(key) = lua.named_registry_value::<mlua::RegistryKey>(&self.key) {
            lua.remove_registry_value(key)?;
        }
        Ok(())
    }
}

// =============================================================================
// Plugin Components
// =============================================================================

/// A trigger intercepts queries before they reach the current view's source.
///
/// Triggers enable prefix-based commands (`:git`), calculators (`= 1+1`),
/// and other query transformations.
pub struct Trigger {
    /// Match function: `match(ctx) -> bool`
    /// If provided, called to determine if trigger should activate.
    pub match_fn: Option<LuaFunctionRef>,

    /// Prefix shorthand. If provided, trigger activates when query starts with prefix.
    /// E.g., prefix = ":" activates for queries like ":git status"
    pub prefix: Option<String>,

    /// Run function: `run(ctx)` - handles the matched query.
    pub run_fn: LuaFunctionRef,
}

impl std::fmt::Debug for Trigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Trigger")
            .field("prefix", &self.prefix)
            .field("has_match_fn", &self.match_fn.is_some())
            .finish()
    }
}

/// A source is a search provider that produces items.
pub struct Source {
    /// Optional identifier for debugging/logging.
    pub name: Option<String>,

    /// If true, contributes to root view.
    pub root: bool,

    /// Group title when contributing to root.
    pub group: Option<String>,

    /// Search function: `search(ctx) -> Groups`
    pub search_fn: LuaFunctionRef,

    /// Milliseconds to wait after typing stops before calling search.
    pub debounce_ms: u32,

    /// Minimum query length before calling search.
    pub min_query_length: u32,
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Source")
            .field("name", &self.name)
            .field("root", &self.root)
            .field("group", &self.group)
            .field("debounce_ms", &self.debounce_ms)
            .field("min_query_length", &self.min_query_length)
            .finish()
    }
}

/// An action operates on one or more items.
pub struct Action {
    /// Unique identifier.
    pub id: String,

    /// Display text in action list.
    pub title: String,

    /// Icon identifier.
    pub icon: Option<String>,

    /// If true, appears for multi-select.
    pub bulk: bool,

    /// Applies function: `applies(ctx) -> bool`
    pub applies_fn: LuaFunctionRef,

    /// Run function: `run(ctx)`
    pub run_fn: LuaFunctionRef,
}

impl std::fmt::Debug for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Action")
            .field("id", &self.id)
            .field("title", &self.title)
            .field("icon", &self.icon)
            .field("bulk", &self.bulk)
            .finish()
    }
}

// =============================================================================
// Plugin
// =============================================================================

/// A plugin is a Lua module that returns a table with metadata and registrations.
pub struct Plugin {
    /// Unique identifier for the plugin.
    pub name: String,

    /// Query hooks that intercept input.
    pub triggers: Vec<Trigger>,

    /// Search providers that produce items.
    pub sources: Vec<Source>,

    /// Operations that act on items.
    pub actions: Vec<Action>,

    /// Called when plugin loads, receives user config.
    pub setup_fn: Option<LuaFunctionRef>,
}

impl std::fmt::Debug for Plugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plugin")
            .field("name", &self.name)
            .field("triggers_count", &self.triggers.len())
            .field("sources_count", &self.sources.len())
            .field("actions_count", &self.actions.len())
            .field("has_setup", &self.setup_fn.is_some())
            .finish()
    }
}

// =============================================================================
// View
// =============================================================================

/// A view is a search context with source, selection, and submission handling.
pub struct View {
    /// Stable view identifier.
    ///
    /// Used for:
    /// - View-specific keybindings: `lux.keymap.set("ctrl+d", "delete", { view = "file_browser" })`
    /// - Logging and debugging
    /// - Future features like `lux.goto_view("file_browser")` or state persistence
    pub id: Option<String>,

    /// Displayed in view header.
    pub title: Option<String>,

    /// Hint text in search input.
    pub placeholder: Option<String>,

    /// Source function: `source(ctx) -> Groups`
    pub source_fn: LuaFunctionRef,

    /// Selection mode.
    pub selection: SelectionMode,

    /// Custom selection hook: `on_select(ctx)`
    pub on_select_fn: Option<LuaFunctionRef>,

    /// Submission hook: `on_submit(ctx)`
    pub on_submit_fn: Option<LuaFunctionRef>,

    /// Data available to source and actions.
    pub view_data: serde_json::Value,
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View")
            .field("id", &self.id)
            .field("title", &self.title)
            .field("placeholder", &self.placeholder)
            .field("selection", &self.selection)
            .field("has_on_select", &self.on_select_fn.is_some())
            .field("has_on_submit", &self.on_submit_fn.is_some())
            .finish()
    }
}

/// A view instance in the view stack.
///
/// Contains the view definition and Lua registry keys for cleanup.
/// Ephemeral state (cursor, selection, query) is owned by the UI.
#[derive(Debug)]
pub struct ViewInstance {
    /// The view definition.
    pub view: View,

    /// Lua registry keys to clean up when this view is popped.
    /// Used for inline source functions and callbacks.
    pub registry_keys: Vec<String>,
}

impl ViewInstance {
    /// Create a new view instance.
    pub fn new(view: View) -> Self {
        Self {
            view,
            registry_keys: Vec::new(),
        }
    }

    /// Create a new view instance with registry keys for cleanup.
    pub fn with_registry_keys(view: View, registry_keys: Vec<String>) -> Self {
        Self {
            view,
            registry_keys,
        }
    }
}

// =============================================================================
// Trigger Results
// =============================================================================

/// Result from running a trigger.
#[derive(Debug, Default)]
pub struct TriggerResult {
    /// Results added via ctx.add_results().
    pub added: Groups,

    /// View pushed via ctx.push(), if any.
    pub pushed_view: Option<View>,

    /// Whether ctx.dismiss() was called.
    pub dismissed: bool,
}

impl TriggerResult {
    /// Create an empty result.
    pub fn empty() -> Self {
        Self::default()
    }
}

// =============================================================================
// View State (for frontend)
// =============================================================================

/// View configuration state sent to frontend.
///
/// Contains only structural configuration (id, title, placeholder, selection mode).
/// Ephemeral state (cursor, selection, query) is owned by the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ViewState {
    /// View identifier (for keybindings, logging, etc).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// View title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Search placeholder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,

    /// Selection mode.
    pub selection: SelectionMode,
}

impl From<&ViewInstance> for ViewState {
    fn from(instance: &ViewInstance) -> Self {
        Self {
            id: instance.view.id.clone(),
            title: instance.view.title.clone(),
            placeholder: instance.view.placeholder.clone(),
            selection: instance.view.selection,
        }
    }
}

