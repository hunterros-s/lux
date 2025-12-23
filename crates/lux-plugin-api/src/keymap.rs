//! Keymap registry for Lua-scriptable keybindings.
//!
//! This module provides the `KeymapRegistry` which collects keybindings
//! during Lua config loading. Bindings are then registered with GPUI
//! in bulk before the UI shows.
//!
//! ## Architecture
//!
//! ```text
//! [Lua config]                    [GPUI startup]
//!      │                               │
//!      ▼                               ▼
//! lux.keymap.set()  ───►  KeymapRegistry  ───►  apply_keybindings()
//!      │                       │                      │
//!      └──► PendingBinding ───►│                      ▼
//!           LuaFunctionRef ───►│               cx.bind_keys()
//! ```

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::types::LuaFunctionRef;

// =============================================================================
// ID Generation
// =============================================================================

/// Generate unique IDs for Lua function bindings.
pub fn generate_handler_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("keyhandler:{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

// =============================================================================
// Key Handler
// =============================================================================

/// A keybinding handler - either an action name or a Lua function.
#[derive(Clone, Debug)]
pub enum KeyHandler {
    /// Built-in action name (e.g., "cursor_down").
    Action(String),

    /// Lua function - stores ID for lookup, function stored separately.
    Function { id: String },
}

// =============================================================================
// Global Hotkey Handler
// =============================================================================

/// Built-in global hotkey actions.
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum BuiltInHotkey {
    /// Toggle launcher visibility.
    ToggleLauncher,
}

impl BuiltInHotkey {
    /// Parse a built-in hotkey action from a string name.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "toggle_launcher" => Some(Self::ToggleLauncher),
            _ => None,
        }
    }

    /// Get the string name of this action.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ToggleLauncher => "toggle_launcher",
        }
    }
}

/// Handler for global system hotkeys.
#[derive(Clone, Debug)]
pub enum GlobalHandler {
    /// Built-in action (e.g., toggle_launcher).
    BuiltIn(BuiltInHotkey),

    /// Lua function to call when hotkey fires.
    Function { id: String },
}

/// A pending global hotkey registration.
#[derive(Clone, Debug)]
pub struct PendingHotkey {
    /// Keystroke string (e.g., "cmd+shift+space").
    pub key: String,

    /// Handler to invoke when hotkey fires.
    pub handler: GlobalHandler,
}

// =============================================================================
// Pending Binding
// =============================================================================

/// A registered keybinding (pending, before GPUI registration).
#[derive(Clone, Debug)]
pub struct PendingBinding {
    /// Keystroke string (e.g., "ctrl+n" or "cmd-shift-z").
    pub key: String,

    /// The handler to invoke.
    pub handler: KeyHandler,

    /// Optional GPUI context predicate (e.g., "Launcher", "SearchInput").
    /// Defaults to "Launcher" if None.
    ///
    /// Common contexts:
    /// - "Launcher" - Main launcher navigation
    /// - "SearchInput" - Text input field
    pub context: Option<String>,

    /// Optional Lua view ID for view-specific bindings (e.g., "file_browser").
    /// Combined with context to form: "{context} && view_id == {view}"
    pub view: Option<String>,
}

// =============================================================================
// Binding Key (for deduplication)
// =============================================================================

/// Composite key for deduplication: (keystroke, context, view).
type BindingKey = (String, Option<String>, Option<String>);

// =============================================================================
// Keymap Registry
// =============================================================================

/// Registry for keybindings during config loading.
///
/// The registry uses a HashMap keyed by (keystroke, view) to ensure
/// later bindings override earlier ones for the same key/view combination.
///
/// Also manages global system hotkeys separately from GPUI bindings.
#[derive(Default)]
pub struct KeymapRegistry {
    /// Pending GPUI bindings - HashMap ensures later bindings override earlier.
    bindings: RwLock<HashMap<BindingKey, PendingBinding>>,

    /// Pending global hotkeys - keyed by keystroke for deduplication.
    hotkeys: RwLock<HashMap<String, PendingHotkey>>,

    /// Lua function refs by ID (for RunLuaHandler dispatch).
    lua_handlers: RwLock<HashMap<String, LuaFunctionRef>>,
}

impl KeymapRegistry {
    /// Create a new empty keymap registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a binding. If same (key, context, view) exists, it's overwritten.
    pub fn set(&self, binding: PendingBinding) {
        let key = (
            binding.key.clone(),
            binding.context.clone(),
            binding.view.clone(),
        );
        self.bindings.write().insert(key, binding);
    }

    /// Remove a binding by key, context, and optional view.
    ///
    /// Returns `true` if a binding was removed.
    ///
    /// **Note:** This only works at startup time. Once bindings are registered
    /// with GPUI via `take_bindings()`, removal requires an app restart.
    pub fn del(&self, key: &str, context: Option<&str>, view: Option<&str>) -> bool {
        let binding_key = (
            key.to_string(),
            context.map(|s| s.to_string()),
            view.map(|s| s.to_string()),
        );
        self.bindings.write().remove(&binding_key).is_some()
    }

    /// Store a Lua function handler.
    pub fn store_lua_handler(&self, id: String, func_ref: LuaFunctionRef) {
        self.lua_handlers.write().insert(id, func_ref);
    }

    /// Get Lua handler by ID (for RunLuaHandler dispatch).
    pub fn get_lua_handler(&self, id: &str) -> Option<LuaFunctionRef> {
        self.lua_handlers.read().get(id).cloned()
    }

    /// Take all pending bindings for GPUI registration.
    ///
    /// This clears the bindings from the registry.
    pub fn take_bindings(&self) -> Vec<PendingBinding> {
        std::mem::take(&mut *self.bindings.write())
            .into_values()
            .collect()
    }

    /// Get the number of pending bindings.
    pub fn binding_count(&self) -> usize {
        self.bindings.read().len()
    }

    /// Get the number of stored Lua handlers.
    pub fn handler_count(&self) -> usize {
        self.lua_handlers.read().len()
    }

    // =========================================================================
    // Global Hotkey Methods
    // =========================================================================

    /// Add a global hotkey. If same key exists, it's overwritten.
    ///
    /// Global hotkeys work when the app is hidden (unlike GPUI bindings).
    pub fn set_global(&self, hotkey: PendingHotkey) {
        let key = hotkey.key.clone();
        self.hotkeys.write().insert(key, hotkey);
    }

    /// Remove a global hotkey by key string.
    ///
    /// Returns `true` if a hotkey was removed.
    ///
    /// **Note:** This only works at startup time. Once hotkeys are registered
    /// with the OS via `take_hotkeys()`, removal requires an app restart.
    pub fn del_global(&self, key: &str) -> bool {
        self.hotkeys.write().remove(key).is_some()
    }

    /// Take all pending hotkeys for OS registration.
    ///
    /// This clears the hotkeys from the registry.
    pub fn take_hotkeys(&self) -> Vec<PendingHotkey> {
        std::mem::take(&mut *self.hotkeys.write())
            .into_values()
            .collect()
    }

    /// Get the number of pending hotkeys.
    pub fn hotkey_count(&self) -> usize {
        self.hotkeys.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_handler_id() {
        let id1 = generate_handler_id();
        let id2 = generate_handler_id();
        assert!(id1.starts_with("keyhandler:"));
        assert!(id2.starts_with("keyhandler:"));
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_keymap_registry_set() {
        let registry = KeymapRegistry::new();

        registry.set(PendingBinding {
            key: "ctrl+n".to_string(),
            handler: KeyHandler::Action("cursor_down".to_string()),
            context: Some("Launcher".to_string()),
            view: None,
        });

        assert_eq!(registry.binding_count(), 1);

        // Override same key + context + view
        registry.set(PendingBinding {
            key: "ctrl+n".to_string(),
            handler: KeyHandler::Action("cursor_up".to_string()),
            context: Some("Launcher".to_string()),
            view: None,
        });

        assert_eq!(registry.binding_count(), 1);

        // Different context is a different binding
        registry.set(PendingBinding {
            key: "ctrl+n".to_string(),
            handler: KeyHandler::Action("submit".to_string()),
            context: Some("SearchInput".to_string()),
            view: None,
        });

        assert_eq!(registry.binding_count(), 2);

        // Different view is also a different binding
        registry.set(PendingBinding {
            key: "ctrl+n".to_string(),
            handler: KeyHandler::Action("delete".to_string()),
            context: Some("Launcher".to_string()),
            view: Some("file_browser".to_string()),
        });

        assert_eq!(registry.binding_count(), 3);
    }

    #[test]
    fn test_keymap_registry_del() {
        let registry = KeymapRegistry::new();

        registry.set(PendingBinding {
            key: "ctrl+n".to_string(),
            handler: KeyHandler::Action("cursor_down".to_string()),
            context: Some("Launcher".to_string()),
            view: None,
        });

        assert_eq!(registry.binding_count(), 1);
        assert!(registry.del("ctrl+n", Some("Launcher"), None));
        assert_eq!(registry.binding_count(), 0);
        assert!(!registry.del("ctrl+n", Some("Launcher"), None)); // Already deleted
    }

    #[test]
    fn test_keymap_registry_take_bindings() {
        let registry = KeymapRegistry::new();

        registry.set(PendingBinding {
            key: "ctrl+n".to_string(),
            handler: KeyHandler::Action("cursor_down".to_string()),
            context: Some("Launcher".to_string()),
            view: None,
        });

        registry.set(PendingBinding {
            key: "ctrl+p".to_string(),
            handler: KeyHandler::Action("cursor_up".to_string()),
            context: Some("Launcher".to_string()),
            view: None,
        });

        let bindings = registry.take_bindings();
        assert_eq!(bindings.len(), 2);
        assert_eq!(registry.binding_count(), 0);
    }

    #[test]
    fn test_global_hotkeys() {
        let registry = KeymapRegistry::new();

        registry.set_global(PendingHotkey {
            key: "cmd+space".to_string(),
            handler: GlobalHandler::BuiltIn(BuiltInHotkey::ToggleLauncher),
        });

        assert_eq!(registry.hotkey_count(), 1);

        // Override same key
        registry.set_global(PendingHotkey {
            key: "cmd+space".to_string(),
            handler: GlobalHandler::Function {
                id: "test".to_string(),
            },
        });

        assert_eq!(registry.hotkey_count(), 1);

        // Add another hotkey
        registry.set_global(PendingHotkey {
            key: "cmd+shift+space".to_string(),
            handler: GlobalHandler::BuiltIn(BuiltInHotkey::ToggleLauncher),
        });

        assert_eq!(registry.hotkey_count(), 2);

        // Delete hotkey
        assert!(registry.del_global("cmd+space"));
        assert_eq!(registry.hotkey_count(), 1);

        // Delete non-existent
        assert!(!registry.del_global("cmd+space"));

        // Take all
        let hotkeys = registry.take_hotkeys();
        assert_eq!(hotkeys.len(), 1);
        assert_eq!(hotkeys[0].key, "cmd+shift+space");
        assert_eq!(registry.hotkey_count(), 0);
    }
}
