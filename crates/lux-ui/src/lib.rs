//! GPUI frontend for the Lux launcher.
//!
//! This crate provides the native GPUI user interface including:
//! - LauncherWindow for window management
//! - LauncherPanel for UI composition
//! - Views and components
//! - Backend integration
//! - Lua-scriptable keybinding system

pub mod actions;
pub mod backend;
pub mod keymap;
pub mod model;
pub mod platform;
pub mod theme;
pub mod views;
pub mod window;

// Re-export commonly used types
pub use backend::{Backend, BackendHandle, BackendState, RuntimeBackend};
pub use lux_core::SelectionMode;
pub use model::{
    ActionMenuItem, ActionMenuState, ActiveState, ExecutionFeedback, LauncherPhase, ListEntry,
    ViewFrame, ViewId, ViewStack,
};
pub use theme::{Appearance, Theme, ThemeExt, ThemeSettings};
pub use views::{
    scroll_to_cursor, LauncherPanel, LauncherPanelEvent, SearchInput, SearchInputEvent,
};
pub use window::{run_launcher, LauncherWindow};
