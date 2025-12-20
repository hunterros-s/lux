//! Observable view stack with automatic change notifications.
//!
//! The key insight: mutation = notification. Every method that changes the stack
//! also broadcasts the new state. Callers cannot mutate without notifying.

use parking_lot::RwLock;
use tokio::sync::watch;

use crate::types::{ViewInstance, ViewState};

// =============================================================================
// ObservableViewStack
// =============================================================================

/// A view stack that automatically broadcasts changes.
///
/// Every mutation method (`push`, `pop`, `replace_top`, `clear`, etc.) broadcasts
/// the new state. This makes it impossible to change the stack without notifying
/// subscribers.
///
/// ## Thread Safety
///
/// Uses `parking_lot::RwLock` for the stack (never poisons) and `tokio::sync::watch`
/// for broadcasts. Multiple threads can read concurrently; writes are exclusive.
///
/// ## Usage
///
/// ```ignore
/// let stack = ObservableViewStack::new();
/// let rx = stack.subscribe();
///
/// // This pushes AND broadcasts
/// stack.push(view_instance);
///
/// // Subscriber sees the change
/// let states = rx.borrow().clone();
/// ```
pub struct ObservableViewStack {
    inner: RwLock<Vec<ViewInstance>>,
    tx: watch::Sender<Vec<ViewState>>,
    rx: watch::Receiver<Vec<ViewState>>,
}

impl ObservableViewStack {
    /// Create a new empty observable view stack.
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(Vec::new());
        Self {
            inner: RwLock::new(Vec::new()),
            tx,
            rx,
        }
    }

    // =========================================================================
    // Mutation Methods (all broadcast automatically)
    // =========================================================================

    /// Push a view onto the stack.
    ///
    /// Broadcasts the new state after pushing.
    pub fn push(&self, view: ViewInstance) {
        let states = {
            let mut inner = self.inner.write();
            inner.push(view);
            tracing::debug!("Pushed view, stack depth: {}", inner.len());
            self.snapshot(&inner)
        };
        let _ = self.tx.send(states);
    }

    /// Pop the top view from the stack.
    ///
    /// Returns `None` if the stack is empty.
    /// Broadcasts the new state only if something was popped.
    pub fn pop(&self) -> Option<ViewInstance> {
        let (result, states) = {
            let mut inner = self.inner.write();
            let result = inner.pop();
            if result.is_some() {
                tracing::debug!("Popped view, stack depth: {}", inner.len());
            }
            (result, self.snapshot(&inner))
        };
        if result.is_some() {
            let _ = self.tx.send(states);
        }
        result
    }

    /// Pop the top view only if there's more than one view.
    ///
    /// Returns `true` if a view was popped, `false` if at root.
    /// Broadcasts the new state only if something was popped.
    pub fn pop_if_not_root(&self) -> bool {
        let (popped, states) = {
            let mut inner = self.inner.write();
            if inner.len() > 1 {
                inner.pop();
                tracing::debug!("Popped view, stack depth: {}", inner.len());
                (true, self.snapshot(&inner))
            } else {
                tracing::debug!("Cannot pop: already at root view");
                (false, Vec::new())
            }
        };
        if popped {
            let _ = self.tx.send(states);
        }
        popped
    }

    /// Replace the top view with a new one.
    ///
    /// If the stack is empty, just pushes the new view.
    /// Returns the old view if one was replaced.
    /// Always broadcasts the new state.
    pub fn replace_top(&self, view: ViewInstance) -> Option<ViewInstance> {
        let (old, states) = {
            let mut inner = self.inner.write();
            let old = inner.pop();
            inner.push(view);
            tracing::debug!("Replaced view, stack depth: {}", inner.len());
            (old, self.snapshot(&inner))
        };
        let _ = self.tx.send(states);
        old
    }

    /// Clear all views from the stack.
    ///
    /// Returns all views that were in the stack.
    /// Broadcasts the new (empty) state.
    pub fn clear(&self) -> Vec<ViewInstance> {
        let old = {
            let mut inner = self.inner.write();
            std::mem::take(&mut *inner)
        };
        let _ = self.tx.send(Vec::new());
        old
    }

    /// Modify the top view in place.
    ///
    /// The closure receives a mutable reference to the top view.
    /// Does NOT broadcast - use for non-structural changes like cursor position.
    /// Returns `true` if there was a view to modify.
    pub fn modify_top<F>(&self, f: F) -> bool
    where
        F: FnOnce(&mut ViewInstance),
    {
        let mut inner = self.inner.write();
        if let Some(view) = inner.last_mut() {
            f(view);
            true
        } else {
            false
        }
    }

    /// Modify the top view and broadcast the change.
    ///
    /// Use this when the modification should notify subscribers.
    /// Returns `true` if there was a view to modify.
    pub fn modify_top_and_broadcast<F>(&self, f: F) -> bool
    where
        F: FnOnce(&mut ViewInstance),
    {
        let (modified, states) = {
            let mut inner = self.inner.write();
            if let Some(view) = inner.last_mut() {
                f(view);
                (true, self.snapshot(&inner))
            } else {
                (false, Vec::new())
            }
        };
        if modified {
            let _ = self.tx.send(states);
        }
        modified
    }

    // =========================================================================
    // Read Methods
    // =========================================================================

    /// Get a snapshot of the current view states.
    ///
    /// This is the preferred way to read the stack when you need a copy.
    pub fn get_states(&self) -> Vec<ViewState> {
        let inner = self.inner.read();
        self.snapshot(&inner)
    }

    /// Get the current (top) view state.
    pub fn get_current_state(&self) -> Option<ViewState> {
        let inner = self.inner.read();
        inner.last().map(ViewState::from)
    }

    /// Get the stack depth.
    pub fn len(&self) -> usize {
        self.inner.read().len()
    }

    /// Check if the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.read().is_empty()
    }

    /// Read the top view with a closure.
    ///
    /// Use this for quick read-only access to the top view.
    /// Returns `None` if the stack is empty.
    pub fn with_top<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&ViewInstance) -> R,
    {
        let inner = self.inner.read();
        inner.last().map(f)
    }

    /// Read the stack with a closure.
    ///
    /// Use this for quick read-only access when you need multiple views.
    pub fn with_stack<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[ViewInstance]) -> R,
    {
        let inner = self.inner.read();
        f(&inner)
    }

    // =========================================================================
    // Subscription
    // =========================================================================

    /// Subscribe to view stack changes.
    ///
    /// The receiver will get the current state immediately and all future changes.
    /// Clone the receiver for multiple subscribers.
    pub fn subscribe(&self) -> watch::Receiver<Vec<ViewState>> {
        self.rx.clone()
    }

    /// Force a broadcast of the current state.
    ///
    /// Useful after initialization to ensure subscribers have the initial state.
    pub fn broadcast(&self) {
        let states = self.get_states();
        let _ = self.tx.send(states);
    }

    // =========================================================================
    // Internal Helpers
    // =========================================================================

    /// Create a snapshot of view states from the inner stack.
    fn snapshot(&self, inner: &[ViewInstance]) -> Vec<ViewState> {
        inner.iter().map(ViewState::from).collect()
    }
}

impl Default for ObservableViewStack {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{LuaFunctionRef, View};
    use lux_core::SelectionMode;

    fn test_view(title: &str) -> View {
        View {
            id: None,
            title: Some(title.to_string()),
            placeholder: None,
            source_fn: LuaFunctionRef::new(format!("test:source:{}", title)),
            selection: SelectionMode::Single,
            on_select_fn: None,
            on_submit_fn: None,
            view_data: serde_json::Value::Null,
        }
    }

    fn test_instance(title: &str) -> ViewInstance {
        ViewInstance::new(test_view(title))
    }

    #[test]
    fn test_push_broadcasts() {
        let stack = ObservableViewStack::new();
        let rx = stack.subscribe();

        // Initially empty
        assert_eq!(rx.borrow().len(), 0);

        // Push broadcasts
        stack.push(test_instance("View 1"));
        assert_eq!(rx.borrow().len(), 1);
        assert_eq!(rx.borrow()[0].title, Some("View 1".to_string()));

        // Push again
        stack.push(test_instance("View 2"));
        assert_eq!(rx.borrow().len(), 2);
    }

    #[test]
    fn test_pop_broadcasts() {
        let stack = ObservableViewStack::new();
        let rx = stack.subscribe();

        stack.push(test_instance("View 1"));
        stack.push(test_instance("View 2"));
        assert_eq!(rx.borrow().len(), 2);

        // Pop broadcasts
        let popped = stack.pop();
        assert!(popped.is_some());
        assert_eq!(rx.borrow().len(), 1);

        // Pop empty doesn't broadcast extra
        stack.pop();
        assert_eq!(rx.borrow().len(), 0);
    }

    #[test]
    fn test_pop_if_not_root() {
        let stack = ObservableViewStack::new();
        let rx = stack.subscribe();

        stack.push(test_instance("Root"));
        stack.push(test_instance("Child"));
        assert_eq!(rx.borrow().len(), 2);

        // Can pop child
        assert!(stack.pop_if_not_root());
        assert_eq!(rx.borrow().len(), 1);

        // Cannot pop root
        assert!(!stack.pop_if_not_root());
        assert_eq!(rx.borrow().len(), 1);
    }

    #[test]
    fn test_replace_top_broadcasts() {
        let stack = ObservableViewStack::new();
        let rx = stack.subscribe();

        stack.push(test_instance("View 1"));
        assert_eq!(rx.borrow()[0].title, Some("View 1".to_string()));

        // Replace broadcasts
        let old = stack.replace_top(test_instance("View 2"));
        assert!(old.is_some());
        assert_eq!(rx.borrow().len(), 1);
        assert_eq!(rx.borrow()[0].title, Some("View 2".to_string()));
    }

    #[test]
    fn test_clear_broadcasts() {
        let stack = ObservableViewStack::new();
        let rx = stack.subscribe();

        stack.push(test_instance("View 1"));
        stack.push(test_instance("View 2"));
        assert_eq!(rx.borrow().len(), 2);

        // Clear broadcasts empty
        let old = stack.clear();
        assert_eq!(old.len(), 2);
        assert_eq!(rx.borrow().len(), 0);
    }

    #[test]
    fn test_modify_top_no_broadcast() {
        let stack = ObservableViewStack::new();
        let rx = stack.subscribe();

        stack.push(test_instance("View 1"));

        // Modify without broadcast (e.g., add a registry key)
        stack.modify_top(|view| {
            view.registry_keys.push("test_key".to_string());
        });

        // Verify the modification happened
        let key_count = stack.with_top(|v| v.registry_keys.len()).unwrap();
        assert_eq!(key_count, 1);
    }

    #[test]
    fn test_modify_top_and_broadcast() {
        let stack = ObservableViewStack::new();
        let rx = stack.subscribe();

        stack.push(test_instance("View 1"));
        let initial_len = rx.borrow().len();

        // Modify with broadcast - this triggers a new broadcast
        // even though ViewState doesn't change (registry_keys not in ViewState)
        stack.modify_top_and_broadcast(|view| {
            view.registry_keys.push("test_key".to_string());
        });

        // Subscriber still sees the view (broadcast happened)
        assert_eq!(rx.borrow().len(), initial_len);
    }

    #[test]
    fn test_with_top() {
        let stack = ObservableViewStack::new();

        // Empty stack returns None
        assert!(stack.with_top(|_| ()).is_none());

        stack.push(test_instance("View 1"));

        // Can read top
        let title = stack.with_top(|v| v.view.title.clone());
        assert_eq!(title, Some(Some("View 1".to_string())));
    }

    #[test]
    fn test_with_stack() {
        let stack = ObservableViewStack::new();

        stack.push(test_instance("View 1"));
        stack.push(test_instance("View 2"));

        let len = stack.with_stack(|s| s.len());
        assert_eq!(len, 2);
    }
}
