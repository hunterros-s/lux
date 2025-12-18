//! Opaque, unforgeable handles for plugin components.
//!
//! Handles are the only way to reference registered components (triggers, sources, actions).
//! They cannot be forged because:
//! - Constructors are private (only registries can create them)
//! - No `From<u64>` or `Into<u64>` implementations
//! - Lua receives handles as opaque userdata
//!
//! Handles use generation-counted IDs to detect stale references.

use std::collections::HashMap;

// =============================================================================
// Opaque Handle Types
// =============================================================================

/// Opaque handle to a registered trigger.
///
/// Can only be created by [`TriggerRegistry::insert`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TriggerHandle(u64);

impl TriggerHandle {
    /// Private constructor - only TriggerRegistry can create handles.
    fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the internal ID (for registry lookup).
    fn id(&self) -> u64 {
        self.0
    }
}

/// Opaque handle to a registered source.
///
/// Can only be created by [`SourceRegistry::insert`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceHandle(u64);

impl SourceHandle {
    fn new(id: u64) -> Self {
        Self(id)
    }

    fn id(&self) -> u64 {
        self.0
    }
}

/// Opaque handle to a registered action.
///
/// Can only be created by [`ActionRegistry::insert`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActionHandle(u64);

impl ActionHandle {
    fn new(id: u64) -> Self {
        Self(id)
    }

    fn id(&self) -> u64 {
        self.0
    }
}

// =============================================================================
// Type-Specific Registries
// =============================================================================

/// Registry for triggers with generation-counted handles.
#[derive(Debug)]
pub struct TriggerRegistry<T> {
    items: HashMap<u64, (String, T)>, // (plugin_name, trigger)
    order: Vec<u64>,                  // Registration order (for priority)
    generation: u64,
}

impl<T> Default for TriggerRegistry<T> {
    fn default() -> Self {
        Self {
            items: HashMap::new(),
            order: Vec::new(),
            generation: 0,
        }
    }
}

impl<T> TriggerRegistry<T> {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a trigger and return its handle.
    pub fn insert(&mut self, plugin_name: &str, item: T) -> TriggerHandle {
        self.generation += 1;
        let id = self.generation;
        self.items.insert(id, (plugin_name.to_string(), item));
        self.order.push(id);
        TriggerHandle::new(id)
    }

    /// Get a trigger by handle.
    pub fn get(&self, handle: TriggerHandle) -> Option<&T> {
        self.items.get(&handle.id()).map(|(_, t)| t)
    }

    /// Get a trigger and its plugin name by handle.
    pub fn get_with_plugin(&self, handle: TriggerHandle) -> Option<(&str, &T)> {
        self.items
            .get(&handle.id())
            .map(|(name, t)| (name.as_str(), t))
    }

    /// Remove a trigger by handle.
    pub fn remove(&mut self, handle: TriggerHandle) -> Option<T> {
        self.order.retain(|&id| id != handle.id());
        self.items.remove(&handle.id()).map(|(_, t)| t)
    }

    /// Iterate over all triggers in registration order.
    pub fn iter(&self) -> impl Iterator<Item = (TriggerHandle, &str, &T)> {
        self.order.iter().filter_map(move |&id| {
            self.items
                .get(&id)
                .map(|(name, t)| (TriggerHandle::new(id), name.as_str(), t))
        })
    }

    /// Get all handles for a plugin.
    pub fn handles_for_plugin(&self, plugin_name: &str) -> Vec<TriggerHandle> {
        self.items
            .iter()
            .filter(|(_, (name, _))| name == plugin_name)
            .map(|(&id, _)| TriggerHandle::new(id))
            .collect()
    }

    /// Remove all triggers for a plugin.
    pub fn remove_plugin(&mut self, plugin_name: &str) -> Vec<T> {
        let ids: Vec<u64> = self
            .items
            .iter()
            .filter(|(_, (name, _))| name == plugin_name)
            .map(|(&id, _)| id)
            .collect();

        self.order.retain(|id| !ids.contains(id));

        ids.into_iter()
            .filter_map(|id| self.items.remove(&id).map(|(_, t)| t))
            .collect()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get the number of registered triggers.
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

/// Registry for sources with generation-counted handles.
#[derive(Debug)]
pub struct SourceRegistry<T> {
    items: HashMap<u64, (String, T)>,
    order: Vec<u64>,
    generation: u64,
}

impl<T> Default for SourceRegistry<T> {
    fn default() -> Self {
        Self {
            items: HashMap::new(),
            order: Vec::new(),
            generation: 0,
        }
    }
}

impl<T> SourceRegistry<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, plugin_name: &str, item: T) -> SourceHandle {
        self.generation += 1;
        let id = self.generation;
        self.items.insert(id, (plugin_name.to_string(), item));
        self.order.push(id);
        SourceHandle::new(id)
    }

    pub fn get(&self, handle: SourceHandle) -> Option<&T> {
        self.items.get(&handle.id()).map(|(_, t)| t)
    }

    pub fn get_with_plugin(&self, handle: SourceHandle) -> Option<(&str, &T)> {
        self.items
            .get(&handle.id())
            .map(|(name, t)| (name.as_str(), t))
    }

    pub fn remove(&mut self, handle: SourceHandle) -> Option<T> {
        self.order.retain(|&id| id != handle.id());
        self.items.remove(&handle.id()).map(|(_, t)| t)
    }

    pub fn iter(&self) -> impl Iterator<Item = (SourceHandle, &str, &T)> {
        self.order.iter().filter_map(move |&id| {
            self.items
                .get(&id)
                .map(|(name, t)| (SourceHandle::new(id), name.as_str(), t))
        })
    }

    pub fn handles_for_plugin(&self, plugin_name: &str) -> Vec<SourceHandle> {
        self.items
            .iter()
            .filter(|(_, (name, _))| name == plugin_name)
            .map(|(&id, _)| SourceHandle::new(id))
            .collect()
    }

    pub fn remove_plugin(&mut self, plugin_name: &str) -> Vec<T> {
        let ids: Vec<u64> = self
            .items
            .iter()
            .filter(|(_, (name, _))| name == plugin_name)
            .map(|(&id, _)| id)
            .collect();

        self.order.retain(|id| !ids.contains(id));

        ids.into_iter()
            .filter_map(|id| self.items.remove(&id).map(|(_, t)| t))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }
}

/// Registry for actions with generation-counted handles.
#[derive(Debug)]
pub struct ActionRegistry<T> {
    items: HashMap<u64, (String, T)>,
    order: Vec<u64>,
    generation: u64,
}

impl<T> Default for ActionRegistry<T> {
    fn default() -> Self {
        Self {
            items: HashMap::new(),
            order: Vec::new(),
            generation: 0,
        }
    }
}

impl<T> ActionRegistry<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, plugin_name: &str, item: T) -> ActionHandle {
        self.generation += 1;
        let id = self.generation;
        self.items.insert(id, (plugin_name.to_string(), item));
        self.order.push(id);
        ActionHandle::new(id)
    }

    pub fn get(&self, handle: ActionHandle) -> Option<&T> {
        self.items.get(&handle.id()).map(|(_, t)| t)
    }

    pub fn get_with_plugin(&self, handle: ActionHandle) -> Option<(&str, &T)> {
        self.items
            .get(&handle.id())
            .map(|(name, t)| (name.as_str(), t))
    }

    pub fn remove(&mut self, handle: ActionHandle) -> Option<T> {
        self.order.retain(|&id| id != handle.id());
        self.items.remove(&handle.id()).map(|(_, t)| t)
    }

    pub fn iter(&self) -> impl Iterator<Item = (ActionHandle, &str, &T)> {
        self.order.iter().filter_map(move |&id| {
            self.items
                .get(&id)
                .map(|(name, t)| (ActionHandle::new(id), name.as_str(), t))
        })
    }

    pub fn handles_for_plugin(&self, plugin_name: &str) -> Vec<ActionHandle> {
        self.items
            .iter()
            .filter(|(_, (name, _))| name == plugin_name)
            .map(|(&id, _)| ActionHandle::new(id))
            .collect()
    }

    pub fn remove_plugin(&mut self, plugin_name: &str) -> Vec<T> {
        let ids: Vec<u64> = self
            .items
            .iter()
            .filter(|(_, (name, _))| name == plugin_name)
            .map(|(&id, _)| id)
            .collect();

        self.order.retain(|id| !ids.contains(id));

        ids.into_iter()
            .filter_map(|id| self.items.remove(&id).map(|(_, t)| t))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_registry_basic() {
        let mut registry: TriggerRegistry<String> = TriggerRegistry::new();

        let h1 = registry.insert("plugin-a", "trigger-1".to_string());
        let h2 = registry.insert("plugin-a", "trigger-2".to_string());
        let h3 = registry.insert("plugin-b", "trigger-3".to_string());

        assert_eq!(registry.len(), 3);
        assert_eq!(registry.get(h1), Some(&"trigger-1".to_string()));
        assert_eq!(registry.get(h2), Some(&"trigger-2".to_string()));
        assert_eq!(registry.get(h3), Some(&"trigger-3".to_string()));

        // Order preserved
        let order: Vec<_> = registry.iter().map(|(_, _, t)| t.as_str()).collect();
        assert_eq!(order, vec!["trigger-1", "trigger-2", "trigger-3"]);
    }

    #[test]
    fn test_trigger_registry_remove() {
        let mut registry: TriggerRegistry<String> = TriggerRegistry::new();

        let h1 = registry.insert("plugin-a", "trigger-1".to_string());
        let _h2 = registry.insert("plugin-a", "trigger-2".to_string());

        assert_eq!(registry.remove(h1), Some("trigger-1".to_string()));
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get(h1), None); // Stale handle returns None
    }

    #[test]
    fn test_trigger_registry_remove_plugin() {
        let mut registry: TriggerRegistry<String> = TriggerRegistry::new();

        registry.insert("plugin-a", "trigger-1".to_string());
        registry.insert("plugin-a", "trigger-2".to_string());
        registry.insert("plugin-b", "trigger-3".to_string());

        let removed = registry.remove_plugin("plugin-a");
        assert_eq!(removed.len(), 2);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_handles_are_unique() {
        let mut registry: TriggerRegistry<String> = TriggerRegistry::new();

        let h1 = registry.insert("plugin", "a".to_string());
        let h2 = registry.insert("plugin", "b".to_string());

        assert_ne!(h1, h2);
    }

    #[test]
    fn test_stale_handle_returns_none() {
        let mut registry: TriggerRegistry<String> = TriggerRegistry::new();

        let h1 = registry.insert("plugin", "a".to_string());
        registry.remove(h1);

        // Stale handle returns None, doesn't crash
        assert_eq!(registry.get(h1), None);
    }
}
