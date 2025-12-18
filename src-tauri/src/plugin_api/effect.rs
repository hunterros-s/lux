//! Effect types for the Plugin API.
//!
//! Effects are returned by Lua callbacks and applied by the engine.
//! This pattern eliminates shared mutable state - Lua describes *intent*,
//! the engine validates and executes.

use std::cell::RefCell;

/// An effect returned by a Lua callback.
///
/// Callbacks accumulate effects via [`EffectCollector`], then the engine
/// applies them in [`Engine::apply_effects`].
#[derive(Debug)]
pub enum Effect {
    /// Set the results for the current view.
    SetGroups(Vec<super::types::Group>),

    /// Push a new view onto the stack.
    PushView(ViewSpec),

    /// Replace current view (pop + push).
    ReplaceView(ViewSpec),

    /// Pop current view (return to previous).
    Pop,

    /// Dismiss the launcher.
    Dismiss,

    /// Show progress indicator (for long-running actions).
    Progress(String),

    /// Mark action as complete.
    Complete { message: String },

    /// Mark action as failed.
    Fail { error: String },

    // =========================================================================
    // Selection Effects (for on_select hook)
    // =========================================================================
    /// Select item IDs.
    Select(Vec<String>),

    /// Deselect item IDs.
    Deselect(Vec<String>),

    /// Clear all selection.
    ClearSelection,
}

/// Specification for a view to push.
///
/// Uses inline source functions stored in Lua registry.
/// These can't go stale since they're created at push time.
#[derive(Debug)]
pub struct ViewSpec {
    pub(crate) title: Option<String>,
    pub(crate) placeholder: Option<String>,
    pub(crate) source_fn_key: String,
    pub(crate) on_select_fn_key: Option<String>,
    pub(crate) on_submit_fn_key: Option<String>,
    pub(crate) selection_mode: SelectionMode,
    pub(crate) view_data: serde_json::Value,
    /// Registry keys that need cleanup when the view is popped.
    pub(crate) registry_keys: Vec<String>,
}

impl ViewSpec {
    /// Create a new ViewSpec with the given source function key.
    pub fn new(source_fn_key: String) -> Self {
        let registry_keys = vec![source_fn_key.clone()];
        Self {
            title: None,
            placeholder: None,
            source_fn_key,
            on_select_fn_key: None,
            on_submit_fn_key: None,
            selection_mode: SelectionMode::Single,
            view_data: serde_json::Value::Null,
            registry_keys,
        }
    }

    /// Set the view title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the placeholder text.
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Set the selection mode.
    pub fn with_selection_mode(mut self, mode: SelectionMode) -> Self {
        self.selection_mode = mode;
        self
    }

    /// Set the on_select callback key.
    pub fn with_on_select(mut self, key: String) -> Self {
        self.registry_keys.push(key.clone());
        self.on_select_fn_key = Some(key);
        self
    }

    /// Set the on_submit callback key.
    pub fn with_on_submit(mut self, key: String) -> Self {
        self.registry_keys.push(key.clone());
        self.on_submit_fn_key = Some(key);
        self
    }

    /// Set view data.
    pub fn with_view_data(mut self, data: serde_json::Value) -> Self {
        self.view_data = data;
        self
    }

    /// Get the registry keys for cleanup when the view is popped.
    pub fn registry_keys(&self) -> &[String] {
        &self.registry_keys
    }
}

/// Selection mode for a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionMode {
    /// Only one item can be selected at a time.
    #[default]
    Single,
    /// Multiple items can be selected.
    Multi,
    /// Custom selection logic via on_select hook.
    Custom,
}

/// Accumulator for effects during Lua callback execution.
///
/// Uses `RefCell` for interior mutability within a single Lua call.
/// After the call completes, use [`take()`](Self::take) to consume
/// the collected effects.
///
/// # Example
///
/// ```ignore
/// let collector = EffectCollector::new();
///
/// // Pass to Lua context...
/// collector.push(Effect::SetItems(items));
/// collector.push(Effect::Dismiss);
///
/// // After Lua call
/// let effects = collector.take();  // Move, not clone
/// engine.apply_effects(effects);
/// ```
#[derive(Debug, Default)]
pub struct EffectCollector {
    effects: RefCell<Vec<Effect>>,
}

impl EffectCollector {
    /// Create a new empty collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Push an effect onto the collection.
    pub fn push(&self, effect: Effect) {
        self.effects.borrow_mut().push(effect);
    }

    /// Consume the collector and return all collected effects.
    ///
    /// This takes ownership, ensuring no clone is needed.
    pub fn take(self) -> Vec<Effect> {
        self.effects.into_inner()
    }

    /// Check if any effects have been collected.
    pub fn is_empty(&self) -> bool {
        self.effects.borrow().is_empty()
    }

    /// Get the number of collected effects.
    pub fn len(&self) -> usize {
        self.effects.borrow().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effect_collector_basic() {
        let collector = EffectCollector::new();
        assert!(collector.is_empty());

        collector.push(Effect::Dismiss);
        assert_eq!(collector.len(), 1);

        collector.push(Effect::Pop);
        assert_eq!(collector.len(), 2);

        let effects = collector.take();
        assert_eq!(effects.len(), 2);
        assert!(matches!(effects[0], Effect::Dismiss));
        assert!(matches!(effects[1], Effect::Pop));
    }

    #[test]
    fn test_view_spec_builder() {
        let spec = ViewSpec::new("test:source".to_string())
            .with_title("Test View")
            .with_placeholder("Search...")
            .with_selection_mode(SelectionMode::Multi);

        assert_eq!(spec.title, Some("Test View".to_string()));
        assert_eq!(spec.placeholder, Some("Search...".to_string()));
        assert_eq!(spec.selection_mode, SelectionMode::Multi);
        assert_eq!(spec.source_fn_key, "test:source");
    }
}
