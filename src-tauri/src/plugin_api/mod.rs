//! Plugin API Module
//!
//! This module implements the Lux Plugin API Specification v0.1.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                        Plugin API                                    │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │  types.rs      - Core types (Item, Group, Plugin, View, etc.)       │
//! │  registry.rs   - Plugin storage and lookup                          │
//! │  context.rs    - Context builders for Lua hooks                     │
//! │  engine.rs     - Query execution and state management               │
//! │  lua/          - Lua bindings (lux.register, lux.configure, etc.)   │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```lua
//! -- In init.lua
//! lux.register({
//!   name = "my-plugin",
//!   triggers = { ... },
//!   sources = { ... },
//!   actions = { ... },
//! })
//! ```

pub mod context;
pub mod effect;
pub mod engine;
pub mod error;
pub mod handle;
pub mod lua;
pub mod registry;
pub mod types;

// Re-export commonly used types
pub use context::{
    // Table-based context builders (for simple hooks)
    build_action_applies_context,
    build_trigger_match_context,
    // Typestate contexts (for effect-based execution)
    ActionContext,
    SelectContext,
    SourceContext,
    SubmitContext,
    TriggerContext,
};
pub use effect::{Effect, EffectCollector, ViewSpec};
pub use engine::{ActionInfo, ApplyResult, QueryEngine};
pub use error::{PluginError, PluginResult};
pub use handle::{
    ActionHandle, ActionRegistry, SourceHandle, SourceRegistry, TriggerHandle, TriggerRegistry,
};
pub use lua::{json_to_lua_value, lua_value_to_json, register_lux_api};
pub use registry::PluginRegistry;
pub use types::{
    Action, ActionResult, Direction, Group, Groups, Item, KeyBinding, KeypressResult,
    LuaFunctionRef, Plugin, SelectionMode, Source, Trigger, TriggerResult, View, ViewInstance,
    ViewState,
};
