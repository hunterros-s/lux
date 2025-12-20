//! Platform-specific implementations.
//!
//! This module provides platform-specific functionality like global hotkeys.

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "macos")]
pub use macos::*;
