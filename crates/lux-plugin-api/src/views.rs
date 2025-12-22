//! View Registry for the new Lua API.
//!
//! This module provides:
//! - `ViewDefinition` - A registered view with search and get_actions functions
//! - `ViewRegistry` - Storage for registered views

use parking_lot::RwLock;
use std::collections::HashMap;

use lux_core::SelectionMode;

use crate::types::LuaFunctionRef;

/// A registered view definition.
///
/// Views are the primary unit of organization in the new API.
/// Each view has a search function and a get_actions function.
#[derive(Debug)]
pub struct ViewDefinition {
    /// Unique identifier for the view.
    pub id: String,

    /// Optional title displayed in the view header.
    pub title: Option<String>,

    /// Optional placeholder text for the search input.
    pub placeholder: Option<String>,

    /// Selection mode: single, multi, or custom.
    pub selection: SelectionMode,

    /// Search function: `search(query, ctx) -> { groups = [...] }`
    pub search_fn: LuaFunctionRef,

    /// Get actions function: `get_actions(item, ctx) -> { action, ... }`
    pub get_actions_fn: LuaFunctionRef,
}

/// Registry for storing view definitions.
///
/// Views are registered via `lux.views.add()` and can be looked up
/// by ID for navigation or action delegation.
pub struct ViewRegistry {
    /// Registered views by ID.
    views: RwLock<HashMap<String, ViewDefinition>>,
}

impl ViewRegistry {
    /// Create a new empty view registry.
    pub fn new() -> Self {
        Self {
            views: RwLock::new(HashMap::new()),
        }
    }

    /// Register a view definition.
    ///
    /// Returns an error if a view with the same ID already exists.
    pub fn add(&self, view: ViewDefinition) -> Result<(), ViewRegistryError> {
        let mut views = self.views.write();
        if views.contains_key(&view.id) {
            return Err(ViewRegistryError::ViewAlreadyExists(view.id));
        }
        let id = view.id.clone();
        views.insert(id.clone(), view);
        tracing::info!("Registered view: {}", id);
        Ok(())
    }

    /// Get a view definition by ID.
    ///
    /// Returns None if the view is not registered.
    pub fn get(&self, id: &str) -> Option<ViewDefinitionRef> {
        let views = self.views.read();
        if views.contains_key(id) {
            Some(ViewDefinitionRef { id: id.to_string() })
        } else {
            None
        }
    }

    /// List all registered view IDs.
    pub fn list(&self) -> Vec<String> {
        let views = self.views.read();
        views.keys().cloned().collect()
    }

    /// Execute a function with access to a view definition.
    ///
    /// This is the safe way to access view data without lifetime issues.
    pub fn with_view<F, R>(&self, id: &str, f: F) -> Option<R>
    where
        F: FnOnce(&ViewDefinition) -> R,
    {
        let views = self.views.read();
        views.get(id).map(f)
    }

    /// Check if a view with the given ID exists.
    pub fn exists(&self, id: &str) -> bool {
        let views = self.views.read();
        views.contains_key(id)
    }

    /// Get the count of registered views.
    pub fn count(&self) -> usize {
        let views = self.views.read();
        views.len()
    }
}

impl Default for ViewRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// A reference to a registered view.
///
/// This is returned from `ViewRegistry::get()` as a lightweight
/// reference that doesn't hold any locks.
#[derive(Debug, Clone)]
pub struct ViewDefinitionRef {
    /// The view ID.
    pub id: String,
}

/// Errors that can occur during view registry operations.
#[derive(Debug, thiserror::Error)]
pub enum ViewRegistryError {
    #[error("View '{0}' already exists")]
    ViewAlreadyExists(String),

    #[error("View '{0}' not found")]
    ViewNotFound(String),

    #[error("Invalid view definition: {0}")]
    InvalidView(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_fn_ref(key: &str) -> LuaFunctionRef {
        LuaFunctionRef::new(key.to_string())
    }

    #[test]
    fn test_view_registry_add_and_get() {
        let registry = ViewRegistry::new();

        let view = ViewDefinition {
            id: "files".to_string(),
            title: Some("Files".to_string()),
            placeholder: Some("Search files...".to_string()),
            selection: SelectionMode::Single,
            search_fn: make_test_fn_ref("files:search"),
            get_actions_fn: make_test_fn_ref("files:get_actions"),
        };

        registry.add(view).unwrap();

        assert!(registry.exists("files"));
        assert!(!registry.exists("other"));

        let view_ref = registry.get("files").unwrap();
        assert_eq!(view_ref.id, "files");
    }

    #[test]
    fn test_view_registry_duplicate_error() {
        let registry = ViewRegistry::new();

        let view1 = ViewDefinition {
            id: "files".to_string(),
            title: None,
            placeholder: None,
            selection: SelectionMode::Single,
            search_fn: make_test_fn_ref("files:search"),
            get_actions_fn: make_test_fn_ref("files:get_actions"),
        };

        let view2 = ViewDefinition {
            id: "files".to_string(),
            title: Some("Different".to_string()),
            placeholder: None,
            selection: SelectionMode::Multi,
            search_fn: make_test_fn_ref("files:search2"),
            get_actions_fn: make_test_fn_ref("files:get_actions2"),
        };

        registry.add(view1).unwrap();
        let result = registry.add(view2);

        assert!(matches!(
            result,
            Err(ViewRegistryError::ViewAlreadyExists(_))
        ));
    }

    #[test]
    fn test_view_registry_list() {
        let registry = ViewRegistry::new();

        let view1 = ViewDefinition {
            id: "files".to_string(),
            title: None,
            placeholder: None,
            selection: SelectionMode::Single,
            search_fn: make_test_fn_ref("files:search"),
            get_actions_fn: make_test_fn_ref("files:get_actions"),
        };

        let view2 = ViewDefinition {
            id: "clipboard".to_string(),
            title: None,
            placeholder: None,
            selection: SelectionMode::Single,
            search_fn: make_test_fn_ref("clipboard:search"),
            get_actions_fn: make_test_fn_ref("clipboard:get_actions"),
        };

        registry.add(view1).unwrap();
        registry.add(view2).unwrap();

        let ids = registry.list();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"files".to_string()));
        assert!(ids.contains(&"clipboard".to_string()));
    }

    #[test]
    fn test_view_registry_with_view() {
        let registry = ViewRegistry::new();

        let view = ViewDefinition {
            id: "files".to_string(),
            title: Some("Files".to_string()),
            placeholder: None,
            selection: SelectionMode::Multi,
            search_fn: make_test_fn_ref("files:search"),
            get_actions_fn: make_test_fn_ref("files:get_actions"),
        };

        registry.add(view).unwrap();

        let title = registry.with_view("files", |v| v.title.clone());
        assert_eq!(title, Some(Some("Files".to_string())));

        let selection = registry.with_view("files", |v| v.selection);
        assert_eq!(selection, Some(SelectionMode::Multi));

        let missing = registry.with_view("other", |v| v.title.clone());
        assert!(missing.is_none());
    }
}
