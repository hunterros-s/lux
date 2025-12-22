//! Plugin API for the Lux launcher.
//!
//! This crate provides the Lua plugin system including:
//! - View-based navigation with lux.views.add/get/list
//! - Hook system for intercepting search/actions
//! - Effect-based Lua execution model
//! - View stack management
//! - Lua-scriptable keybinding system

pub mod context;
pub mod effect;
pub mod engine;
pub mod error;
pub mod handle;
pub mod hooks;
pub mod keymap;
pub mod lua;
pub mod registry;
pub mod types;
pub mod views;

// Re-export commonly used types
pub use effect::{Effect, EffectCollector, ViewSpec};
pub use engine::{ActionInfo, ApplyResult, QueryEngine};
pub use error::{PluginError, PluginResult};
pub use hooks::{HookEntry, HookError, HookRegistry};
pub use keymap::{generate_handler_id, KeyHandler, KeymapRegistry, PendingBinding};
pub use lua::register_lux_api;
pub use registry::PluginRegistry;
pub use types::{LuaFunctionRef, View, ViewInstance, ViewState};
pub use views::{ViewDefinition, ViewDefinitionRef, ViewRegistry, ViewRegistryError};

// Re-export lux_core types for convenience
pub use lux_core::{ActionResult, FollowUpAction, Group, Groups, Item, SelectionMode};
