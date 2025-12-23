//! Hook System for the Lux Lua API.
//!
//! This module provides:
//! - `HookRegistry` - Storage for registered hooks
//! - `HookEntry` - Individual hook registration
//! - Hook chain execution with pcall wrapping
//!
//! ## Hook Paths
//!
//! - `search` - Global search hook
//! - `get_actions` - Global actions hook
//! - `views.{id}.search` - View-specific search hook
//! - `views.{id}.get_actions` - View-specific actions hook
//!
//! ## Execution Order
//!
//! 1. View-specific hooks (registration order)
//! 2. Global hooks (registration order)
//! 3. Original function
//!
//! Chain is built as: original → view hooks → global hooks
//! Global hooks are outermost, view hooks see raw results.
//!
//! ## Error Isolation
//!
//! Hooks are pcall wrapped. If a hook throws, the error is logged
//! and the chain continues with the previous result.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::types::LuaFunctionRef;

/// Global counter for generating unique hook IDs.
static HOOK_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique hook ID.
fn generate_hook_id() -> String {
    let id = HOOK_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("hook:{}", id)
}

/// A registered hook entry.
#[derive(Debug)]
pub struct HookEntry {
    /// Unique identifier for unhooking.
    pub id: String,

    /// Reference to the Lua function.
    pub function: LuaFunctionRef,
}

/// Registry for storing hooks.
///
/// Hooks are registered via `lux.hook(path, fn)` and executed
/// in a chain when search or get_actions is called.
pub struct HookRegistry {
    /// Global hooks by hook name (search, get_actions).
    global_hooks: RwLock<HashMap<String, Vec<HookEntry>>>,

    /// View-specific hooks: view_id -> hook_name -> hooks.
    view_hooks: RwLock<HashMap<String, HashMap<String, Vec<HookEntry>>>>,
}

impl HookRegistry {
    /// Create a new empty hook registry.
    pub fn new() -> Self {
        Self {
            global_hooks: RwLock::new(HashMap::new()),
            view_hooks: RwLock::new(HashMap::new()),
        }
    }

    /// Add a hook at the specified path.
    ///
    /// Returns the hook ID for later removal.
    ///
    /// # Hook Paths
    ///
    /// - `search` - Global search hook
    /// - `get_actions` - Global actions hook
    /// - `views.{id}.search` - View-specific search hook
    /// - `views.{id}.get_actions` - View-specific actions hook
    pub fn add(&self, path: &str, func: LuaFunctionRef) -> String {
        let id = generate_hook_id();
        let entry = HookEntry {
            id: id.clone(),
            function: func,
        };

        if let Some((view_id, hook_name)) = parse_view_hook_path(path) {
            // View-specific hook: views.{id}.{hook}
            let mut view_hooks = self.view_hooks.write();
            let view_map = view_hooks.entry(view_id.to_string()).or_default();
            let hooks = view_map.entry(hook_name.to_string()).or_default();
            hooks.push(entry);
            tracing::debug!(
                "Added view hook '{}' for view '{}' (id: {})",
                hook_name,
                view_id,
                id
            );
        } else {
            // Global hook: search, get_actions
            let mut global = self.global_hooks.write();
            let hooks = global.entry(path.to_string()).or_default();
            hooks.push(entry);
            tracing::debug!("Added global hook '{}' (id: {})", path, id);
        }

        id
    }

    /// Remove a hook by ID.
    ///
    /// Returns true if the hook was found and removed.
    pub fn remove(&self, id: &str) -> bool {
        // Try global hooks first
        {
            let mut global = self.global_hooks.write();
            for hooks in global.values_mut() {
                if let Some(pos) = hooks.iter().position(|h| h.id == id) {
                    hooks.remove(pos);
                    tracing::debug!("Removed global hook (id: {})", id);
                    return true;
                }
            }
        }

        // Try view hooks
        {
            let mut view_hooks = self.view_hooks.write();
            for view_map in view_hooks.values_mut() {
                for hooks in view_map.values_mut() {
                    if let Some(pos) = hooks.iter().position(|h| h.id == id) {
                        hooks.remove(pos);
                        tracing::debug!("Removed view hook (id: {})", id);
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Get the hook chain for a given hook name and optional view ID.
    ///
    /// Returns function references in execution order:
    /// - View-specific hooks first (registration order)
    /// - Global hooks second (registration order)
    ///
    /// When building the actual call chain:
    /// - Chain is: original → view hooks → global hooks
    /// - Global hooks wrap view hooks, which wrap the original
    /// - Result: view hooks see raw results, global hooks see modified results
    pub fn get_chain(&self, hook_name: &str, view_id: Option<&str>) -> Vec<LuaFunctionRef> {
        let mut chain = Vec::new();

        // View-specific hooks first (inner)
        if let Some(vid) = view_id {
            let view_hooks = self.view_hooks.read();
            if let Some(view_map) = view_hooks.get(vid) {
                if let Some(hooks) = view_map.get(hook_name) {
                    chain.extend(hooks.iter().map(|h| h.function.clone()));
                }
            }
        }

        // Global hooks second (outer)
        let global = self.global_hooks.read();
        if let Some(hooks) = global.get(hook_name) {
            chain.extend(hooks.iter().map(|h| h.function.clone()));
        }

        chain
    }

    /// Check if any hooks are registered for the given path.
    pub fn has_hooks(&self, hook_name: &str, view_id: Option<&str>) -> bool {
        // Check view-specific hooks
        if let Some(vid) = view_id {
            let view_hooks = self.view_hooks.read();
            if let Some(view_map) = view_hooks.get(vid) {
                if let Some(hooks) = view_map.get(hook_name) {
                    if !hooks.is_empty() {
                        return true;
                    }
                }
            }
        }

        // Check global hooks
        let global = self.global_hooks.read();
        if let Some(hooks) = global.get(hook_name) {
            if !hooks.is_empty() {
                return true;
            }
        }

        false
    }

    /// Get the count of hooks for a given path.
    pub fn count(&self, hook_name: &str, view_id: Option<&str>) -> usize {
        let mut count = 0;

        // Count view-specific hooks
        if let Some(vid) = view_id {
            let view_hooks = self.view_hooks.read();
            if let Some(view_map) = view_hooks.get(vid) {
                if let Some(hooks) = view_map.get(hook_name) {
                    count += hooks.len();
                }
            }
        }

        // Count global hooks
        let global = self.global_hooks.read();
        if let Some(hooks) = global.get(hook_name) {
            count += hooks.len();
        }

        count
    }

    /// Clear all hooks (useful for testing).
    #[cfg(test)]
    pub fn clear(&self) {
        self.global_hooks.write().clear();
        self.view_hooks.write().clear();
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a view-specific hook path like "views.files.search" into (view_id, hook_name).
///
/// Returns None for global hooks like "search" or "get_actions".
fn parse_view_hook_path(path: &str) -> Option<(&str, &str)> {
    if let Some(rest) = path.strip_prefix("views.") {
        if let Some(dot_pos) = rest.find('.') {
            let view_id = &rest[..dot_pos];
            let hook_name = &rest[dot_pos + 1..];
            if !view_id.is_empty() && !hook_name.is_empty() {
                return Some((view_id, hook_name));
            }
        }
    }
    None
}

/// Validate a hook path.
///
/// Valid paths:
/// - `search`
/// - `get_actions`
/// - `views.{id}.search`
/// - `views.{id}.get_actions`
pub fn validate_hook_path(path: &str) -> Result<(), HookError> {
    match path {
        "search" | "get_actions" => Ok(()),
        _ if path.starts_with("views.") => {
            if let Some((view_id, hook_name)) = parse_view_hook_path(path) {
                if view_id.is_empty() {
                    return Err(HookError::InvalidPath(format!(
                        "View ID cannot be empty in '{}'",
                        path
                    )));
                }
                if hook_name != "search" && hook_name != "get_actions" {
                    return Err(HookError::InvalidPath(format!(
                        "Invalid hook name '{}' in '{}'. Expected 'search' or 'get_actions'",
                        hook_name, path
                    )));
                }
                Ok(())
            } else {
                Err(HookError::InvalidPath(format!(
                    "Invalid view hook path '{}'. Expected 'views.{{id}}.search' or 'views.{{id}}.get_actions'",
                    path
                )))
            }
        }
        _ => Err(HookError::InvalidPath(format!(
            "Invalid hook path '{}'. Expected 'search', 'get_actions', or 'views.{{id}}.{{hook}}'",
            path
        ))),
    }
}

/// Errors that can occur during hook operations.
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    #[error("Invalid hook path: {0}")]
    InvalidPath(String),

    #[error("Hook not found: {0}")]
    HookNotFound(String),

    #[error("Hook execution error: {0}")]
    ExecutionError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_fn_ref(key: &str) -> LuaFunctionRef {
        LuaFunctionRef::new(key.to_string())
    }

    #[test]
    fn test_parse_view_hook_path() {
        assert_eq!(
            parse_view_hook_path("views.files.search"),
            Some(("files", "search"))
        );
        assert_eq!(
            parse_view_hook_path("views.clipboard.get_actions"),
            Some(("clipboard", "get_actions"))
        );
        assert_eq!(parse_view_hook_path("search"), None);
        assert_eq!(parse_view_hook_path("get_actions"), None);
        assert_eq!(parse_view_hook_path("views."), None);
        assert_eq!(parse_view_hook_path("views..search"), None);
    }

    #[test]
    fn test_validate_hook_path() {
        assert!(validate_hook_path("search").is_ok());
        assert!(validate_hook_path("get_actions").is_ok());
        assert!(validate_hook_path("views.files.search").is_ok());
        assert!(validate_hook_path("views.files.get_actions").is_ok());

        assert!(validate_hook_path("invalid").is_err());
        assert!(validate_hook_path("views.files.invalid").is_err());
        assert!(validate_hook_path("views..search").is_err());
    }

    #[test]
    fn test_add_global_hook() {
        let registry = HookRegistry::new();

        let id = registry.add("search", make_test_fn_ref("hook1:search"));
        assert!(id.starts_with("hook:"));
        assert!(registry.has_hooks("search", None));
        assert_eq!(registry.count("search", None), 1);
    }

    #[test]
    fn test_add_view_hook() {
        let registry = HookRegistry::new();

        let id = registry.add("views.files.search", make_test_fn_ref("files:hook:search"));
        assert!(id.starts_with("hook:"));
        assert!(registry.has_hooks("search", Some("files")));
        assert_eq!(registry.count("search", Some("files")), 1);
        assert!(!registry.has_hooks("search", Some("other")));
    }

    #[test]
    fn test_remove_hook() {
        let registry = HookRegistry::new();

        let id1 = registry.add("search", make_test_fn_ref("hook1"));
        let id2 = registry.add("search", make_test_fn_ref("hook2"));

        assert_eq!(registry.count("search", None), 2);

        assert!(registry.remove(&id1));
        assert_eq!(registry.count("search", None), 1);

        assert!(registry.remove(&id2));
        assert_eq!(registry.count("search", None), 0);

        // Removing again should return false
        assert!(!registry.remove(&id1));
    }

    #[test]
    fn test_get_chain_order() {
        let registry = HookRegistry::new();

        // Add view-specific hooks
        registry.add("views.files.search", make_test_fn_ref("view1"));
        registry.add("views.files.search", make_test_fn_ref("view2"));

        // Add global hooks
        registry.add("search", make_test_fn_ref("global1"));
        registry.add("search", make_test_fn_ref("global2"));

        let chain = registry.get_chain("search", Some("files"));

        // View hooks come first (inner), then global hooks (outer)
        assert_eq!(chain.len(), 4);
        assert_eq!(chain[0].key, "view1");
        assert_eq!(chain[1].key, "view2");
        assert_eq!(chain[2].key, "global1");
        assert_eq!(chain[3].key, "global2");
    }

    #[test]
    fn test_get_chain_no_view() {
        let registry = HookRegistry::new();

        registry.add("search", make_test_fn_ref("global1"));
        registry.add("views.files.search", make_test_fn_ref("view1"));

        // Without view_id, only global hooks are returned
        let chain = registry.get_chain("search", None);
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].key, "global1");
    }
}
