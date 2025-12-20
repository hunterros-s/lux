//! Plugin API for the Lux launcher.
//!
//! This crate provides the Lua plugin system including:
//! - Plugin registration and lifecycle
//! - Query engine for trigger/source/action execution
//! - Effect-based Lua execution model
//! - View stack management
//! - Lua-scriptable keybinding system

pub mod context;
pub mod effect;
pub mod engine;
pub mod error;
pub mod handle;
pub mod keymap;
pub mod lua;
pub mod registry;
pub mod types;

// Re-export commonly used types
pub use effect::{Effect, EffectCollector, ViewSpec};
pub use engine::{ActionInfo, ApplyResult, QueryEngine};
pub use error::{PluginError, PluginResult};
pub use keymap::{generate_handler_id, KeyHandler, KeymapRegistry, PendingBinding};
pub use lua::register_lux_api;
pub use registry::{PluginRegistry, RegistryError, RegistryResult};
pub use types::{
    Action, LuaFunctionRef, Plugin, Source, Trigger, TriggerResult, View, ViewInstance, ViewState,
};

// Re-export lux_core types for convenience
pub use lux_core::{ActionResult, FollowUpAction, Group, Groups, Item, SelectionMode};
