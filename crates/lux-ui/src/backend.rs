//! Backend integration for the Lux launcher.
//!
//! This module provides the bridge between the UI and the plugin engine.
//! The `Backend` trait is GPUI-independent and mockable for testing.
//!
//! ## Reactive State
//!
//! The engine broadcasts view stack changes automatically via `tokio::sync::watch`.
//! The UI subscribes to these changes and reacts to configuration updates.
//! View stack mutations (push/pop/replace) in the engine auto-notify subscribers.

use futures::future::BoxFuture;
use lux_core::{ActionResult, BackendError, Groups, Item};
use lux_lua_runtime::LuaRuntime;
use lux_plugin_api::{ActionInfo, PluginRegistry, QueryEngine, ViewState};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

// =============================================================================
// Backend State (Type Alias)
// =============================================================================

/// View stack state broadcast from the engine.
///
/// Each ViewState contains configuration (title, placeholder, selection_mode).
/// Ephemeral state (cursor, selection, query) is owned by the UI.
pub type BackendState = Vec<ViewState>;

// =============================================================================
// Backend Trait
// =============================================================================

/// Trait for backend operations.
///
/// This trait is GPUI-independent and returns futures, allowing the caller
/// to spawn them however they want. This enables testing with mock backends.
///
/// ## View Stack Operations
///
/// Views are pushed/popped through effects:
/// - `execute_action()` may return `ActionResult::PushView` or `ActionResult::Pop`
/// - State changes are broadcast via `subscribe()` for reactive UI updates
/// - `pop_view()` is for UI-initiated navigation (e.g., Escape key)
pub trait Backend: Send + Sync {
    /// Subscribe to state changes. Clone the receiver for each subscriber.
    fn subscribe(&self) -> watch::Receiver<BackendState>;

    /// Search with the current query. Returns groups of results.
    fn search(&self, query: String) -> BoxFuture<'static, Result<Groups, BackendError>>;

    /// Get available actions for the given items.
    fn get_actions(
        &self,
        items: Vec<Item>,
    ) -> BoxFuture<'static, Result<Vec<ActionInfo>, BackendError>>;

    /// Execute an action. Returns the action result.
    ///
    /// The result indicates what happened:
    /// - `ActionResult::Dismiss` - close the launcher
    /// - `ActionResult::Pop` - go back to previous view
    /// - `ActionResult::PushView` - a new view was pushed
    /// - `ActionResult::Continue` - stay on current view
    /// - `ActionResult::Complete` - show success feedback
    /// - `ActionResult::Progress` - show progress feedback
    /// - `ActionResult::Fail` - show error feedback
    ///
    /// View stack changes are also broadcast via subscription.
    fn execute_action(
        &self,
        plugin: String,
        action_index: usize,
        items: Vec<Item>,
    ) -> BoxFuture<'static, Result<ActionResult, BackendError>>;

    /// Pop the current view (UI-initiated, e.g., Escape key).
    /// Returns true if a view was popped, false if already at root.
    /// State changes are broadcast via subscription.
    fn pop_view(&self) -> BoxFuture<'static, Result<bool, BackendError>>;

    /// Initialize the engine with the root view.
    /// State changes are broadcast via subscription.
    fn initialize(&self) -> BoxFuture<'static, Result<(), BackendError>>;

    /// Run a Lua key handler by ID.
    ///
    /// This is used for keybindings that map to Lua functions.
    fn run_key_handler(
        &self,
        handler_id: &str,
        items: Vec<Item>,
    ) -> BoxFuture<'static, Result<ActionResult, BackendError>>;
}

// =============================================================================
// Runtime Backend
// =============================================================================

/// Real backend implementation using QueryEngine and LuaRuntime.
///
/// View stack changes are broadcast automatically by the engine.
/// RuntimeBackend forwards the engine's subscription channel.
pub struct RuntimeBackend {
    engine: Arc<QueryEngine>,
    runtime: Arc<LuaRuntime>,
    registry: Arc<PluginRegistry>,
    timeout: Duration,
}

impl RuntimeBackend {
    /// Create a new runtime backend.
    pub fn new(
        engine: Arc<QueryEngine>,
        runtime: Arc<LuaRuntime>,
        registry: Arc<PluginRegistry>,
    ) -> Self {
        Self {
            engine,
            runtime,
            registry,
            timeout: Duration::from_secs(5),
        }
    }

    /// Create with a custom timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Get a reference to the engine.
    pub fn engine(&self) -> &Arc<QueryEngine> {
        &self.engine
    }

    /// Get a reference to the runtime.
    pub fn runtime(&self) -> &Arc<LuaRuntime> {
        &self.runtime
    }
}

impl Backend for RuntimeBackend {
    fn subscribe(&self) -> watch::Receiver<BackendState> {
        // Forward engine's subscription directly
        // View stack changes are broadcast automatically by the engine
        self.engine.subscribe()
    }

    fn search(&self, query: String) -> BoxFuture<'static, Result<Groups, BackendError>> {
        let engine = self.engine.clone();
        let runtime = self.runtime.clone();
        let timeout = self.timeout;

        Box::pin(async move {
            runtime
                .with_lua_timeout(timeout, move |lua| {
                    engine.search(lua, &query).map_err(|e| e.to_string())
                })
                .await
        })
    }

    fn get_actions(
        &self,
        items: Vec<Item>,
    ) -> BoxFuture<'static, Result<Vec<ActionInfo>, BackendError>> {
        let engine = self.engine.clone();
        let runtime = self.runtime.clone();
        let timeout = self.timeout;

        Box::pin(async move {
            runtime
                .with_lua_timeout(timeout, move |lua| {
                    engine
                        .get_applicable_actions(lua, &items)
                        .map_err(|e| e.to_string())
                })
                .await
        })
    }

    fn execute_action(
        &self,
        plugin: String,
        action_index: usize,
        items: Vec<Item>,
    ) -> BoxFuture<'static, Result<ActionResult, BackendError>> {
        let engine = self.engine.clone();
        let runtime = self.runtime.clone();
        let timeout = self.timeout;

        Box::pin(async move {
            // View stack changes are auto-broadcast by the engine
            runtime
                .with_lua_timeout(timeout, move |lua| {
                    engine
                        .execute_action(lua, &plugin, action_index, &items)
                        .map_err(|e| e.to_string())
                })
                .await
        })
    }

    fn pop_view(&self) -> BoxFuture<'static, Result<bool, BackendError>> {
        let engine = self.engine.clone();

        Box::pin(async move {
            // pop_view auto-broadcasts via ObservableViewStack
            Ok(engine.pop_view())
        })
    }

    fn initialize(&self) -> BoxFuture<'static, Result<(), BackendError>> {
        let engine = self.engine.clone();
        let runtime = self.runtime.clone();
        let timeout = self.timeout;

        Box::pin(async move {
            // initialize auto-broadcasts via ObservableViewStack
            runtime
                .with_lua_timeout(timeout, move |lua| {
                    engine.initialize(lua);
                    Ok(())
                })
                .await
        })
    }

    fn run_key_handler(
        &self,
        handler_id: &str,
        items: Vec<Item>,
    ) -> BoxFuture<'static, Result<ActionResult, BackendError>> {
        let engine = self.engine.clone();
        let runtime = self.runtime.clone();
        let registry = self.registry.clone();
        let timeout = self.timeout;
        let handler_id = handler_id.to_string();

        Box::pin(async move {
            // Look up the Lua function from keymap registry
            let func_ref = registry
                .keymap()
                .get_lua_handler(&handler_id)
                .ok_or_else(|| {
                    BackendError::Lua(format!("Key handler not found: {}", handler_id))
                })?;

            // Execute via the engine
            runtime
                .with_lua_timeout(timeout, move |lua| {
                    engine
                        .execute_lua_callback(lua, &func_ref, &items)
                        .map_err(|e| e.to_string())
                })
                .await
        })
    }
}

// Keep BackendHandle as an alias for backwards compatibility
pub type BackendHandle = RuntimeBackend;

// =============================================================================
// Mock Backend for Testing
// =============================================================================

#[cfg(test)]
pub mod mock {
    use super::*;
    use lux_core::SelectionMode;
    use parking_lot::Mutex;

    /// Mock backend for testing.
    pub struct MockBackend {
        pub search_results: Arc<Mutex<Groups>>,
        pub search_delay: Duration,
        pub actions: Arc<Mutex<Vec<ActionInfo>>>,
        pub can_pop: Arc<Mutex<bool>>,
        /// Kept alive to keep watch channel active.
        _state_tx: watch::Sender<BackendState>,
        state_rx: watch::Receiver<BackendState>,
    }

    impl MockBackend {
        /// Create a new mock backend.
        pub fn new() -> Self {
            let initial_state: BackendState = vec![ViewState {
                id: None,
                title: None,
                placeholder: Some("Search...".to_string()),
                selection: SelectionMode::Single,
            }];
            let (state_tx, state_rx) = watch::channel(initial_state);

            Self {
                search_results: Arc::new(Mutex::new(vec![])),
                search_delay: Duration::ZERO,
                actions: Arc::new(Mutex::new(vec![])),
                can_pop: Arc::new(Mutex::new(true)),
                _state_tx: state_tx,
                state_rx,
            }
        }

        /// Set the search results.
        pub fn with_results(self, results: Groups) -> Self {
            *self.search_results.lock() = results;
            self
        }

        /// Set the search delay.
        pub fn with_delay(mut self, delay: Duration) -> Self {
            self.search_delay = delay;
            self
        }

        /// Set whether pop_view returns true or false.
        pub fn with_can_pop(self, can_pop: bool) -> Self {
            *self.can_pop.lock() = can_pop;
            self
        }
    }

    impl Default for MockBackend {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Backend for MockBackend {
        fn subscribe(&self) -> watch::Receiver<BackendState> {
            self.state_rx.clone()
        }

        fn search(&self, _query: String) -> BoxFuture<'static, Result<Groups, BackendError>> {
            let results = self.search_results.clone();
            let delay = self.search_delay;

            Box::pin(async move {
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                Ok(results.lock().clone())
            })
        }

        fn get_actions(
            &self,
            _items: Vec<Item>,
        ) -> BoxFuture<'static, Result<Vec<ActionInfo>, BackendError>> {
            let actions = self.actions.clone();
            Box::pin(async move { Ok(actions.lock().clone()) })
        }

        fn execute_action(
            &self,
            _plugin: String,
            _action_index: usize,
            _items: Vec<Item>,
        ) -> BoxFuture<'static, Result<ActionResult, BackendError>> {
            Box::pin(async move { Ok(ActionResult::Dismiss) })
        }

        fn pop_view(&self) -> BoxFuture<'static, Result<bool, BackendError>> {
            let can_pop = self.can_pop.clone();
            Box::pin(async move { Ok(*can_pop.lock()) })
        }

        fn initialize(&self) -> BoxFuture<'static, Result<(), BackendError>> {
            Box::pin(async move { Ok(()) })
        }

        fn run_key_handler(
            &self,
            _handler_id: &str,
            _items: Vec<Item>,
        ) -> BoxFuture<'static, Result<ActionResult, BackendError>> {
            // Mock: key handlers are a no-op
            Box::pin(async move { Ok(ActionResult::Continue) })
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::mock::*;
    use super::*;
    use lux_core::Group;

    fn test_items() -> Vec<Item> {
        vec![Item::new("1", "Test Item")]
    }

    fn test_groups() -> Groups {
        vec![Group::new("Test", test_items())]
    }

    #[tokio::test]
    async fn test_mock_backend_search() {
        let backend = MockBackend::new().with_results(test_groups());

        let results = backend.search("test".to_string()).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].items.len(), 1);
    }

    #[tokio::test]
    async fn test_mock_backend_with_delay() {
        let backend = MockBackend::new()
            .with_results(test_groups())
            .with_delay(Duration::from_millis(10));

        let start = std::time::Instant::now();
        let _results = backend.search("test".to_string()).await.unwrap();
        assert!(start.elapsed() >= Duration::from_millis(10));
    }

    #[tokio::test]
    async fn test_mock_backend_execute_action() {
        let backend = MockBackend::new();

        let result = backend
            .execute_action("test".to_string(), 0, test_items())
            .await
            .unwrap();

        assert!(matches!(result, ActionResult::Dismiss));
    }

    #[tokio::test]
    async fn test_mock_backend_pop_view() {
        let backend = MockBackend::new();
        assert!(backend.pop_view().await.unwrap());

        let backend = MockBackend::new().with_can_pop(false);
        assert!(!backend.pop_view().await.unwrap());
    }

    #[tokio::test]
    async fn test_mock_backend_initialize() {
        let backend = MockBackend::new();
        assert!(backend.initialize().await.is_ok());
    }

    #[tokio::test]
    async fn test_mock_backend_subscribe() {
        let backend = MockBackend::new();
        let rx = backend.subscribe();
        let state = rx.borrow();
        assert_eq!(state.len(), 1);
        assert!(state.last().is_some());
    }
}
