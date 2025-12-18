//! Core types for the Lux Plugin API.
//!
//! This module defines the data structures that match the Plugin API Specification v0.1.
//! All types here are designed for:
//! - Serialization between Rust and Lua
//! - IPC transport between backend and frontend
//! - Clean separation of concerns

use mlua::{Function, Lua, Result as LuaResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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
// Item & Group
// =============================================================================

/// An item is the atomic unit of data in Lux.
///
/// Everything users search, select, and act upon is an item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    /// Unique identifier within the current result set.
    pub id: String,

    /// Primary display text.
    pub title: String,

    /// Secondary display text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,

    /// Icon identifier (path, emoji, or named icon).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// Array of type tags for action filtering.
    /// E.g., ["file", "typescript", "react"]
    #[serde(default)]
    pub types: Vec<String>,

    /// Arbitrary data for actions to consume.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Item {
    /// Check if this item has a specific type tag.
    pub fn has_type(&self, type_name: &str) -> bool {
        self.types.iter().any(|t| t == type_name)
    }
}

/// A group of items with an optional title.
///
/// Sources return groups to enable sectioned results like
/// "Recent", "Suggested", "All Files", etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    /// Optional section title. If None, items are ungrouped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Items in this group.
    pub items: Vec<Item>,
}

impl Group {
    /// Create a new group with a title.
    pub fn new(title: impl Into<String>, items: Vec<Item>) -> Self {
        Self {
            title: Some(title.into()),
            items,
        }
    }

    /// Create an ungrouped group (no title).
    pub fn ungrouped(items: Vec<Item>) -> Self {
        Self { title: None, items }
    }
}

/// A collection of groups returned by sources.
pub type Groups = Vec<Group>;

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

/// Selection mode for a view.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SelectionMode {
    /// Selecting an item clears previous selection.
    #[default]
    Single,
    /// Selecting toggles. Multiple items can be selected.
    Multi,
    /// `on_select` hook controls all selection logic.
    Custom,
}

/// A key binding in a view.
#[derive(Debug, Clone)]
pub enum KeyBinding {
    /// Lua function to call.
    Function(LuaFunctionRef),
    /// Action ID to execute.
    ActionId(String),
}

/// A view is a search context with source, selection, and submission handling.
pub struct View {
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

    /// Custom keybindings for this view.
    pub keys: HashMap<String, KeyBinding>,
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View")
            .field("title", &self.title)
            .field("placeholder", &self.placeholder)
            .field("selection", &self.selection)
            .field("has_on_select", &self.on_select_fn.is_some())
            .field("has_on_submit", &self.on_submit_fn.is_some())
            .field("keys_count", &self.keys.len())
            .finish()
    }
}

/// A view instance in the view stack with runtime state.
#[derive(Debug)]
pub struct ViewInstance {
    /// The view definition.
    pub view: View,

    /// Currently focused item (arrow keys move this).
    pub cursor_id: Option<String>,

    /// Selected items (actions operate on these).
    pub selected_ids: HashSet<String>,

    /// Preserved query when pushed.
    pub query: String,

    /// Preserved scroll position.
    pub scroll_position: Option<u32>,
}

impl ViewInstance {
    /// Create a new view instance.
    pub fn new(view: View, initial_query: Option<String>) -> Self {
        Self {
            view,
            cursor_id: None,
            selected_ids: HashSet::new(),
            query: initial_query.unwrap_or_default(),
            scroll_position: None,
        }
    }
}

// =============================================================================
// Action Results
// =============================================================================

/// Result returned by action execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ActionResult {
    /// Close Lux entirely.
    Dismiss,

    /// Push a new view onto the stack.
    PushView {
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        query: Option<String>,
    },

    /// Replace current view.
    ReplaceView {
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },

    /// Pop current view, return to previous.
    Pop,

    /// Keep launcher open, continue.
    Continue,

    /// Show progress message.
    Progress { message: String },

    /// Action completed successfully.
    Complete {
        message: String,
        #[serde(default)]
        actions: Vec<FollowUpAction>,
    },

    /// Action failed.
    Fail { error: String },
}

/// A follow-up action shown after completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowUpAction {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
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

/// View state sent to frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewState {
    /// View title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Search placeholder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,

    /// Selection mode.
    pub selection: SelectionMode,

    /// Currently focused item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_id: Option<String>,

    /// Selected item IDs.
    pub selected_ids: Vec<String>,

    /// Current query.
    pub query: String,
}

impl From<&ViewInstance> for ViewState {
    fn from(instance: &ViewInstance) -> Self {
        Self {
            title: instance.view.title.clone(),
            placeholder: instance.view.placeholder.clone(),
            selection: instance.view.selection,
            cursor_id: instance.cursor_id.clone(),
            selected_ids: instance.selected_ids.iter().cloned().collect(),
            query: instance.query.clone(),
        }
    }
}

// =============================================================================
// Direction (for cursor movement)
// =============================================================================

/// Direction for cursor movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

// =============================================================================
// Keypress Result
// =============================================================================

/// Result of handling a keypress.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeypressResult {
    /// Key was handled by a view binding.
    Handled,
    /// Key was not handled, frontend should process.
    NotHandled,
}
