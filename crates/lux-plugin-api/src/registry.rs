//! Plugin Registry
//!
//! Stores the root view and provides registries for views, hooks, and keybindings.

use parking_lot::RwLock;
use std::sync::Arc;

use crate::hooks::HookRegistry;
use crate::keymap::KeymapRegistry;
use crate::types::View;
use crate::views::ViewRegistry;

/// The plugin registry stores the root view and sub-registries.
pub struct PluginRegistry {
    /// Custom root view, if set by user.
    root_view: RwLock<Option<View>>,

    /// Keymap registry for Lua-scriptable keybindings.
    keymap: Arc<KeymapRegistry>,

    /// View registry for the new API (lux.views.add/get/list).
    view_registry: Arc<ViewRegistry>,

    /// Hook registry for the new API (lux.hook).
    hook_registry: Arc<HookRegistry>,
}

impl PluginRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            root_view: RwLock::new(None),
            keymap: Arc::new(KeymapRegistry::new()),
            view_registry: Arc::new(ViewRegistry::new()),
            hook_registry: Arc::new(HookRegistry::new()),
        }
    }

    /// Get the keymap registry (shared Arc).
    pub fn keymap(&self) -> Arc<KeymapRegistry> {
        self.keymap.clone()
    }

    /// Get the view registry (shared Arc).
    pub fn views(&self) -> Arc<ViewRegistry> {
        self.view_registry.clone()
    }

    /// Get the hook registry (shared Arc).
    pub fn hooks(&self) -> Arc<HookRegistry> {
        self.hook_registry.clone()
    }

    /// Set a custom root view.
    pub fn set_root_view(&self, view: View) {
        let mut root = self.root_view.write();
        *root = Some(view);
    }

    /// Take the root view (consumes it from the registry).
    pub fn take_root_view(&self) -> Option<View> {
        let mut root = self.root_view.write();
        root.take()
    }

    /// Check if a custom root view was set.
    pub fn has_root_view(&self) -> bool {
        self.root_view.read().is_some()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new() {
        let registry = PluginRegistry::new();
        // Registry should have empty sub-registries
        assert_eq!(registry.keymap().binding_count(), 0);
        assert_eq!(registry.views().count(), 0);
    }
}
