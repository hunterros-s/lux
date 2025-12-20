//! Core types for the Lux launcher.
//!
//! This crate contains shared data structures that are used across all Lux crates:
//! - Item and Group types for search results
//! - Selection modes
//! - Action results
//! - Configuration types
//! - Error types

mod action;
mod config;
mod error;
mod item;
mod selection;

pub use action::{ActionInfo, ActionResult, FollowUpAction};
pub use config::{
    config_dir, ensure_config_dir, init_lua_path, AppConfig, AppearanceConfig, HotkeyConfig,
    ThemeMode,
};
pub use error::{BackendError, ConfigError};
pub use item::{Group, Groups, Item, ItemId};
pub use selection::SelectionMode;
