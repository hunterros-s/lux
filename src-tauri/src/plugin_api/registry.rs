//! Plugin Registry
//!
//! Stores registered plugins and provides lookup methods for triggers, sources, and actions.

use parking_lot::RwLock;
use std::collections::HashMap;

use mlua::{Lua, LuaSerdeExt};
use serde_json::Value;

use super::types::{Action, Plugin, Source, Trigger, View};

/// Result type for registry operations.
pub type RegistryResult<T> = Result<T, RegistryError>;

/// Errors that can occur during registry operations.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("Plugin '{0}' already registered")]
    PluginAlreadyRegistered(String),

    #[error("Plugin '{0}' not found")]
    PluginNotFound(String),

    #[error("Invalid plugin definition: {0}")]
    InvalidPlugin(String),

    #[error("Lua error: {0}")]
    LuaError(#[from] mlua::Error),
}

/// The plugin registry stores all registered plugins and their components.
///
/// Components (triggers, sources, actions) are stored in Vecs to maintain
/// registration order, which affects result merging and default action selection.
pub struct PluginRegistry {
    /// All registered plugins by name.
    plugins: RwLock<HashMap<String, PluginEntry>>,

    /// All triggers with their plugin name.
    /// Vec maintains registration order.
    triggers: RwLock<Vec<(String, TriggerEntry)>>,

    /// All sources with their plugin name.
    sources: RwLock<Vec<(String, SourceEntry)>>,

    /// All actions with their plugin name.
    actions: RwLock<Vec<(String, ActionEntry)>>,

    /// Custom root view, if set by user.
    root_view: RwLock<Option<View>>,
}

/// Entry for a registered plugin.
struct PluginEntry {
    plugin: Plugin,
    config: Option<Value>,
}

/// Entry for a registered trigger (index-based reference).
struct TriggerEntry {
    trigger_index: usize,
}

/// Entry for a registered source (index-based reference).
struct SourceEntry {
    source_index: usize,
}

/// Entry for a registered action (index-based reference).
struct ActionEntry {
    action_index: usize,
}

impl PluginRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
            triggers: RwLock::new(Vec::new()),
            sources: RwLock::new(Vec::new()),
            actions: RwLock::new(Vec::new()),
            root_view: RwLock::new(None),
        }
    }

    /// Register a plugin.
    ///
    /// This extracts triggers, sources, and actions from the plugin and stores them
    /// for fast lookup during query execution.
    pub fn register(&self, plugin: Plugin) -> RegistryResult<()> {
        let name = plugin.name.clone();

        // Check for duplicate registration
        {
            let plugins = self.plugins.read();
            if plugins.contains_key(&name) {
                return Err(RegistryError::PluginAlreadyRegistered(name));
            }
        }

        // Store trigger references
        {
            let mut triggers = self.triggers.write();
            for (i, _trigger) in plugin.triggers.iter().enumerate() {
                triggers.push((name.clone(), TriggerEntry { trigger_index: i }));
            }
        }

        // Store source references
        {
            let mut sources = self.sources.write();
            for (i, _source) in plugin.sources.iter().enumerate() {
                sources.push((name.clone(), SourceEntry { source_index: i }));
            }
        }

        // Store action references
        {
            let mut actions = self.actions.write();
            for (i, _action) in plugin.actions.iter().enumerate() {
                actions.push((name.clone(), ActionEntry { action_index: i }));
            }
        }

        // Store the plugin itself
        {
            let mut plugins = self.plugins.write();
            plugins.insert(
                name.clone(),
                PluginEntry {
                    plugin,
                    config: None,
                },
            );
        }

        tracing::info!("Registered plugin: {}", name);
        Ok(())
    }

    /// Configure a plugin.
    ///
    /// Calls the plugin's setup function if it exists.
    pub fn configure(&self, name: &str, config: Value, lua: &Lua) -> RegistryResult<()> {
        let mut plugins = self.plugins.write();
        let entry = plugins
            .get_mut(name)
            .ok_or_else(|| RegistryError::PluginNotFound(name.to_string()))?;

        // Store config
        entry.config = Some(config.clone());

        // Call setup function if exists
        if let Some(ref setup_fn) = entry.plugin.setup_fn {
            let config_value = lua.to_value(&config).map_err(RegistryError::LuaError)?;
            setup_fn.call::<_, ()>(lua, config_value)?;
        }

        tracing::info!("Configured plugin: {}", name);
        Ok(())
    }

    /// Set a custom root view.
    pub fn set_root_view(&self, view: View) {
        let mut root = self.root_view.write();
        *root = Some(view);
    }

    /// Get the root view.
    pub fn get_root_view(&self) -> Option<View> {
        // Note: This clones the View, which isn't ideal since View contains
        // LuaFunctionRef. We may need to refactor this to use Arc or
        // return a reference with a guard.
        // For now, this is a placeholder - actual implementation will need
        // to handle the View lifecycle properly.
        None // TODO: Implement proper view cloning/sharing
    }

    /// Get all triggers that should be tested for a query.
    ///
    /// Returns an iterator over (plugin_name, trigger) pairs in registration order.
    pub fn get_triggers(&self) -> Vec<(String, &Trigger)> {
        // Note: This is a simplified version. The actual implementation needs
        // to handle the borrow checker properly since we're holding locks.
        // We may need to restructure to avoid returning references.
        Vec::new() // TODO: Implement with proper lifetime handling
    }

    /// Get sources that contribute to the root view.
    pub fn get_root_sources(&self) -> Vec<(String, usize)> {
        let plugins = self.plugins.read();
        let sources = self.sources.read();

        let mut result = Vec::new();
        for (plugin_name, entry) in sources.iter() {
            if let Some(plugin_entry) = plugins.get(plugin_name) {
                let source = &plugin_entry.plugin.sources[entry.source_index];
                if source.root {
                    result.push((plugin_name.clone(), entry.source_index));
                }
            }
        }
        result
    }

    /// Get actions that apply to an item.
    ///
    /// Returns actions in registration order. First applicable action is default.
    pub fn get_actions_for_item(&self) -> Vec<(String, usize)> {
        // Returns (plugin_name, action_index) pairs
        // The actual filtering by applies_fn happens at call time
        let actions = self.actions.read();
        actions
            .iter()
            .map(|(name, entry)| (name.clone(), entry.action_index))
            .collect()
    }

    /// Get a plugin by name.
    pub fn get_plugin(&self, name: &str) -> Option<String> {
        let plugins = self.plugins.read();
        plugins.get(name).map(|_| name.to_string())
    }

    /// List all registered plugin names.
    pub fn list_plugins(&self) -> Vec<String> {
        let plugins = self.plugins.read();
        plugins.keys().cloned().collect()
    }

    /// Get trigger count.
    pub fn trigger_count(&self) -> usize {
        self.triggers.read().len()
    }

    /// Get source count.
    pub fn source_count(&self) -> usize {
        self.sources.read().len()
    }

    /// Get action count.
    pub fn action_count(&self) -> usize {
        self.actions.read().len()
    }

    /// Check if a trigger with the given prefix exists.
    pub fn has_prefix_trigger(&self, prefix: &str) -> bool {
        let plugins = self.plugins.read();
        let triggers = self.triggers.read();

        for (plugin_name, entry) in triggers.iter() {
            if let Some(plugin_entry) = plugins.get(plugin_name) {
                let trigger = &plugin_entry.plugin.triggers[entry.trigger_index];
                if trigger.prefix.as_deref() == Some(prefix) {
                    return true;
                }
            }
        }
        false
    }

    /// Execute a function with access to a specific plugin's trigger.
    ///
    /// This is the safe way to access triggers without lifetime issues.
    pub fn with_trigger<F, R>(&self, plugin_name: &str, trigger_index: usize, f: F) -> Option<R>
    where
        F: FnOnce(&Trigger) -> R,
    {
        let plugins = self.plugins.read();
        plugins
            .get(plugin_name)
            .and_then(|entry| entry.plugin.triggers.get(trigger_index).map(f))
    }

    /// Execute a function with access to a specific plugin's source.
    pub fn with_source<F, R>(&self, plugin_name: &str, source_index: usize, f: F) -> Option<R>
    where
        F: FnOnce(&Source) -> R,
    {
        let plugins = self.plugins.read();
        plugins
            .get(plugin_name)
            .and_then(|entry| entry.plugin.sources.get(source_index).map(f))
    }

    /// Execute a function with access to a specific plugin's action.
    pub fn with_action<F, R>(&self, plugin_name: &str, action_index: usize, f: F) -> Option<R>
    where
        F: FnOnce(&Action) -> R,
    {
        let plugins = self.plugins.read();
        plugins
            .get(plugin_name)
            .and_then(|entry| entry.plugin.actions.get(action_index).map(f))
    }

    /// Iterate over all triggers in registration order.
    ///
    /// The callback receives (plugin_name, trigger_index, trigger).
    pub fn for_each_trigger<F>(&self, mut f: F)
    where
        F: FnMut(&str, usize, &Trigger),
    {
        let plugins = self.plugins.read();
        let triggers = self.triggers.read();

        for (plugin_name, entry) in triggers.iter() {
            if let Some(plugin_entry) = plugins.get(plugin_name) {
                if let Some(trigger) = plugin_entry.plugin.triggers.get(entry.trigger_index) {
                    f(plugin_name, entry.trigger_index, trigger);
                }
            }
        }
    }

    /// Iterate over all root sources.
    pub fn for_each_root_source<F>(&self, mut f: F)
    where
        F: FnMut(&str, usize, &Source),
    {
        let plugins = self.plugins.read();
        let sources = self.sources.read();

        for (plugin_name, entry) in sources.iter() {
            if let Some(plugin_entry) = plugins.get(plugin_name) {
                if let Some(source) = plugin_entry.plugin.sources.get(entry.source_index) {
                    if source.root {
                        f(plugin_name, entry.source_index, source);
                    }
                }
            }
        }
    }

    /// Iterate over all actions.
    pub fn for_each_action<F>(&self, mut f: F)
    where
        F: FnMut(&str, usize, &Action),
    {
        let plugins = self.plugins.read();
        let actions = self.actions.read();

        for (plugin_name, entry) in actions.iter() {
            if let Some(plugin_entry) = plugins.get(plugin_name) {
                if let Some(action) = plugin_entry.plugin.actions.get(entry.action_index) {
                    f(plugin_name, entry.action_index, action);
                }
            }
        }
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
        assert_eq!(registry.list_plugins().len(), 0);
        assert_eq!(registry.trigger_count(), 0);
        assert_eq!(registry.source_count(), 0);
        assert_eq!(registry.action_count(), 0);
    }

    // More tests would require a Lua context to create function refs
}
