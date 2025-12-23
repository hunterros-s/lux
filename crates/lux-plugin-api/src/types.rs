//! Core types for the Lux Plugin API.
//!
//! This module defines the data structures for views, function references,
//! and other core API types.
//!
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

    /// Get actions function: `get_actions(item, ctx) -> Actions`
    pub get_actions_fn: Option<LuaFunctionRef>,

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
            .field("has_get_actions", &self.get_actions_fn.is_some())
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
