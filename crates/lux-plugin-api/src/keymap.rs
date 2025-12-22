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
// Pending Binding
// =============================================================================

/// A registered keybinding (pending, before GPUI registration).
#[derive(Clone, Debug)]
pub struct PendingBinding {
    /// Keystroke string (e.g., "ctrl+n" or "cmd-shift-z").
    pub key: String,

    /// The handler to invoke.
    pub handler: KeyHandler,

    /// Optional view ID for view-specific bindings (e.g., "file_browser").
    /// None means global binding.
    pub view: Option<String>,
}

// =============================================================================
// Binding Key (for deduplication)
// =============================================================================

/// Composite key for deduplication: (keystroke, view).
type BindingKey = (String, Option<String>);

// =============================================================================
// Keymap Registry
// =============================================================================

/// Registry for keybindings during config loading.
///
/// The registry uses a HashMap keyed by (keystroke, view) to ensure
/// later bindings override earlier ones for the same key/view combination.
#[derive(Default)]
pub struct KeymapRegistry {
    /// Pending bindings - HashMap ensures later bindings override earlier.
    bindings: RwLock<HashMap<BindingKey, PendingBinding>>,

    /// Lua function refs by ID (for RunLuaHandler dispatch).
    lua_handlers: RwLock<HashMap<String, LuaFunctionRef>>,

    /// Global hotkeys (system-wide, not GPUI).
    global_hotkeys: RwLock<HashMap<String, GlobalHotkey>>,
}

/// A global hotkey (system-wide, outside the app window).
#[derive(Clone, Debug)]
pub struct GlobalHotkey {
    /// Keystroke string (e.g., "cmd+space").
    pub key: String,

    /// The handler to invoke.
    pub handler: KeyHandler,
}

impl KeymapRegistry {
    /// Create a new empty keymap registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a binding. If same (key, view) exists, it's overwritten.
    pub fn set(&self, binding: PendingBinding) {
        let key = (binding.key.clone(), binding.view.clone());
        self.bindings.write().insert(key, binding);
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
    // Deletion
    // =========================================================================

    /// Delete a binding by key and optional view.
    ///
    /// Returns `true` if a binding was removed.
    pub fn del(&self, key: &str, view: Option<&str>) -> bool {
        let binding_key = (key.to_string(), view.map(|s| s.to_string()));
        self.bindings.write().remove(&binding_key).is_some()
    }

    // =========================================================================
    // Global Hotkeys
    // =========================================================================

    /// Register a global hotkey (system-wide, outside the app window).
    ///
    /// If the same key exists, it's overwritten.
    pub fn set_global(&self, hotkey: GlobalHotkey) {
        self.global_hotkeys
            .write()
            .insert(hotkey.key.clone(), hotkey);
    }

    /// Delete a global hotkey.
    ///
    /// Returns `true` if a hotkey was removed.
    pub fn del_global(&self, key: &str) -> bool {
        self.global_hotkeys.write().remove(key).is_some()
    }

    /// Take all global hotkeys for platform registration.
    ///
    /// This clears the hotkeys from the registry.
    pub fn take_global_hotkeys(&self) -> Vec<GlobalHotkey> {
        std::mem::take(&mut *self.global_hotkeys.write())
            .into_values()
            .collect()
    }

    /// Get the number of global hotkeys.
    pub fn global_hotkey_count(&self) -> usize {
        self.global_hotkeys.read().len()
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
            view: None,
        });

        assert_eq!(registry.binding_count(), 1);

        // Override same key
        registry.set(PendingBinding {
            key: "ctrl+n".to_string(),
            handler: KeyHandler::Action("cursor_up".to_string()),
            view: None,
        });

        assert_eq!(registry.binding_count(), 1);

        // Different view is a different binding
        registry.set(PendingBinding {
            key: "ctrl+n".to_string(),
            handler: KeyHandler::Action("submit".to_string()),
            view: Some("file_browser".to_string()),
        });

        assert_eq!(registry.binding_count(), 2);
    }

    #[test]
    fn test_keymap_registry_take_bindings() {
        let registry = KeymapRegistry::new();

        registry.set(PendingBinding {
            key: "ctrl+n".to_string(),
            handler: KeyHandler::Action("cursor_down".to_string()),
            view: None,
        });

        registry.set(PendingBinding {
            key: "ctrl+p".to_string(),
            handler: KeyHandler::Action("cursor_up".to_string()),
            view: None,
        });

        let bindings = registry.take_bindings();
        assert_eq!(bindings.len(), 2);
        assert_eq!(registry.binding_count(), 0);
    }

    #[test]
    fn test_keymap_registry_del() {
        let registry = KeymapRegistry::new();

        registry.set(PendingBinding {
            key: "ctrl+n".to_string(),
            handler: KeyHandler::Action("cursor_down".to_string()),
            view: None,
        });

        registry.set(PendingBinding {
            key: "ctrl+n".to_string(),
            handler: KeyHandler::Action("submit".to_string()),
            view: Some("file_browser".to_string()),
        });

        assert_eq!(registry.binding_count(), 2);

        // Delete global binding
        assert!(registry.del("ctrl+n", None));
        assert_eq!(registry.binding_count(), 1);

        // Delete non-existent binding
        assert!(!registry.del("ctrl+n", None));

        // Delete view-specific binding
        assert!(registry.del("ctrl+n", Some("file_browser")));
        assert_eq!(registry.binding_count(), 0);
    }

    #[test]
    fn test_global_hotkeys() {
        let registry = KeymapRegistry::new();

        registry.set_global(GlobalHotkey {
            key: "cmd+space".to_string(),
            handler: KeyHandler::Action("toggle".to_string()),
        });

        assert_eq!(registry.global_hotkey_count(), 1);

        // Override same key
        registry.set_global(GlobalHotkey {
            key: "cmd+space".to_string(),
            handler: KeyHandler::Action("show".to_string()),
        });

        assert_eq!(registry.global_hotkey_count(), 1);

        // Add another hotkey
        registry.set_global(GlobalHotkey {
            key: "cmd+shift+space".to_string(),
            handler: KeyHandler::Action("clipboard".to_string()),
        });

        assert_eq!(registry.global_hotkey_count(), 2);

        // Delete hotkey
        assert!(registry.del_global("cmd+space"));
        assert_eq!(registry.global_hotkey_count(), 1);

        // Delete non-existent
        assert!(!registry.del_global("cmd+space"));

        // Take all
        let hotkeys = registry.take_global_hotkeys();
        assert_eq!(hotkeys.len(), 1);
        assert_eq!(hotkeys[0].key, "cmd+shift+space");
        assert_eq!(registry.global_hotkey_count(), 0);
    }
}
