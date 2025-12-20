//! Persistent Lua runtime with thread-safe async access.
//!
//! mlua::Lua is !Send, so we run it on a dedicated OS thread
//! and communicate via channels.

use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use lux_core::BackendError;
use mlua::Lua;
use tokio::sync::oneshot;

/// Type alias for Lua closure functions.
type LuaFn = Box<dyn FnOnce(&Lua) -> Result<serde_json::Value, String> + Send>;

/// Request types for the Lua runtime thread.
pub enum LuaRequest {
    /// Execute arbitrary code on the Lua thread.
    WithLua {
        func: LuaFn,
        resp: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    Shutdown,
}

/// Persistent Lua runtime that runs on a dedicated OS thread.
///
/// Since mlua::Lua is !Send, we cannot use it across async tasks.
/// Instead, we spawn a dedicated thread that owns the Lua state
/// and communicate with it via channels.
pub struct LuaRuntime {
    tx: mpsc::Sender<LuaRequest>,
    _handle: JoinHandle<()>,
}

impl LuaRuntime {
    /// Create a new Lua runtime. MUST use std::thread::spawn, NOT tokio::spawn.
    pub fn new(lua: Lua) -> Self {
        let (tx, rx) = mpsc::channel();

        // Dedicated OS thread - Lua stays here forever
        let handle = thread::spawn(move || {
            tracing::info!("Lua runtime thread started");

            while let Ok(request) = rx.recv() {
                match request {
                    LuaRequest::WithLua { func, resp } => {
                        let result = func(&lua);
                        let _ = resp.send(result);
                    }
                    LuaRequest::Shutdown => {
                        tracing::info!("Lua runtime thread shutting down");
                        break;
                    }
                }
            }
        });

        Self {
            tx,
            _handle: handle,
        }
    }

    /// Execute arbitrary code on the Lua thread.
    ///
    /// The closure receives a reference to the Lua state and can perform any operations.
    /// The result is serialized to JSON and returned.
    pub async fn with_lua<F, T>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce(&Lua) -> Result<T, String> + Send + 'static,
        T: serde::de::DeserializeOwned + serde::Serialize + 'static,
    {
        let (resp_tx, resp_rx) = oneshot::channel();

        // Wrap the closure to return JSON
        let boxed_fn: LuaFn = Box::new(move |lua| {
            let result = f(lua)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        });

        self.tx
            .send(LuaRequest::WithLua {
                func: boxed_fn,
                resp: resp_tx,
            })
            .map_err(|e| e.to_string())?;

        let json_result = resp_rx.await.map_err(|e| e.to_string())??;
        serde_json::from_value(json_result).map_err(|e| e.to_string())
    }

    /// Execute with timeout wrapper.
    ///
    /// Wraps the Lua execution with a timeout. If the timeout expires,
    /// returns a BackendError::Timeout.
    pub async fn with_lua_timeout<F, T>(&self, timeout: Duration, f: F) -> Result<T, BackendError>
    where
        F: FnOnce(&Lua) -> Result<T, String> + Send + 'static,
        T: serde::de::DeserializeOwned + serde::Serialize + 'static,
    {
        match tokio::time::timeout(timeout, self.with_lua(f)).await {
            Ok(result) => result.map_err(|e| BackendError::Lua(e)),
            Err(_) => Err(BackendError::Timeout { duration: timeout }),
        }
    }

    /// Shutdown the Lua runtime thread.
    pub fn shutdown(&self) {
        let _ = self.tx.send(LuaRequest::Shutdown);
    }
}

impl Drop for LuaRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_with_lua_basic() {
        let lua = Lua::new();
        let runtime = LuaRuntime::new(lua);

        let result: i32 = runtime
            .with_lua(|lua| {
                let value: i32 = lua.load("return 1 + 2").eval().map_err(|e| e.to_string())?;
                Ok(value)
            })
            .await
            .unwrap();

        assert_eq!(result, 3);
    }

    #[tokio::test]
    async fn test_with_lua_error() {
        let lua = Lua::new();
        let runtime = LuaRuntime::new(lua);

        let result: Result<i32, String> = runtime
            .with_lua(|lua| {
                let _: i32 = lua
                    .load("return invalid_syntax(")
                    .eval()
                    .map_err(|e| e.to_string())?;
                Ok(0)
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_with_lua_timeout() {
        let lua = Lua::new();
        let runtime = LuaRuntime::new(lua);

        // This should succeed within timeout
        let result: Result<i32, BackendError> = runtime
            .with_lua_timeout(Duration::from_secs(1), |lua| {
                let value: i32 = lua.load("return 42").eval().map_err(|e| e.to_string())?;
                Ok(value)
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }
}
