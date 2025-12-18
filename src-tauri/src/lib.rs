//! Lux Launcher - An extensible Spotlight-like launcher for macOS.
//!
//! This is the composition root that wires all components together.
//!
//! Startup order:
//! 1. Initialize tracing
//! 2. Create PluginRegistry
//! 3. Load init.lua from ~/.config/lux/init.lua
//! 4. Create QueryEngine
//! 5. Initialize QueryEngine with root view

use std::sync::Arc;

use tauri::{ActivationPolicy, Emitter, Listener};

// Module declarations
pub mod commands;
pub mod config;
pub mod error;
pub mod events;
pub mod lua_runtime;
pub mod platform;
pub mod plugin_api;

// Re-export error types for convenience
pub use error::{AppError, AppResult};

use events::{EventBus, TauriEvent};
use lua_runtime::LuaRuntime;
use plugin_api::{PluginRegistry, QueryEngine};

/// Initialize the tracing subscriber for structured logging.
///
/// Log levels can be controlled via the `RUST_LOG` environment variable:
/// - `RUST_LOG=debug` - Enable debug logs for all modules
/// - `RUST_LOG=info,lux=debug` - Info for most, debug for lux modules
fn init_tracing() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("info")
            .add_directive("lux=debug".parse().expect("valid directive"))
            .add_directive("tauri=info".parse().expect("valid directive"))
    });

    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_target(true)
                .with_file(true)
                .with_line_number(true),
        )
        .with(filter)
        .init();
}

/// Run the Lux application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing first
    init_tracing();

    // Create the event bus and subscribe before sharing
    let event_bus = Arc::new(EventBus::new());
    let event_rx = event_bus.subscribe();

    // Create the plugin registry
    let plugin_registry = Arc::new(PluginRegistry::new());

    // Ensure config directory exists
    if let Err(e) = config::ensure_config_dir() {
        tracing::error!("Failed to create config directory: {}", e);
    }

    // Load init.lua with the Plugin API
    tracing::info!("Loading init.lua...");
    let lua_runtime: Option<Arc<LuaRuntime>> =
        match config::load_init_lua(Arc::clone(&plugin_registry)) {
            Ok(Some(lua)) => {
                tracing::info!("Loaded init.lua with Plugin API");
                Some(Arc::new(LuaRuntime::new(lua)))
            }
            Ok(None) => {
                tracing::info!("No init.lua found, using defaults");
                None
            }
            Err(e) => {
                tracing::error!("Failed to load init.lua: {}", e);
                None
            }
        };

    // Create the QueryEngine with the plugin registry
    let query_engine = Arc::new(QueryEngine::new(Arc::clone(&plugin_registry)));

    // Initialize QueryEngine with root view (needs Lua context)
    if let Some(ref rt) = lua_runtime {
        let engine = Arc::clone(&query_engine);
        let rt_clone = Arc::clone(rt);
        tauri::async_runtime::block_on(async move {
            let _ = rt_clone
                .with_lua(move |lua| {
                    engine.initialize(lua);
                    Ok(serde_json::Value::Null)
                })
                .await;
        });
    }

    // Build the Tauri application
    tauri::Builder::default()
        // Plugin: NSPanel support for macOS
        .plugin(tauri_nspanel::init())
        // Plugin: Global shortcuts (registered on frontend-ready)
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // Plugin: File system access
        .plugin(tauri_plugin_fs::init())
        // State: QueryEngine
        .manage(Arc::clone(&query_engine))
        // State: Event bus
        .manage(Arc::clone(&event_bus))
        // State: Lua runtime
        .manage(lua_runtime.clone())
        // Register Tauri commands
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::get_actions,
            commands::execute_action,
            commands::execute_default_action,
            commands::pop_view,
            commands::pop_to_view,
            commands::get_view_state,
            commands::get_view_stack,
        ])
        // Setup hook
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            {
                // Hide from dock (run as accessory app)
                app.set_activation_policy(ActivationPolicy::Accessory);

                // Set up the spotlight panel
                platform::setup_panel(app)?;

                // Set up hide request listener
                platform::setup_hide_listener(app);
            }

            // Bridge EventBus to Tauri events
            let app_handle = app.handle().clone();
            let mut rx = event_rx;
            tauri::async_runtime::spawn(async move {
                while let Ok(event) = rx.recv().await {
                    if let Some(tauri_event) = TauriEvent::from_lux_event(&event) {
                        let event_name = tauri_event.event_name();
                        let _ = app_handle.emit(event_name, tauri_event);
                    }
                }
            });

            // Listen for frontend ready signal to register shortcut
            let app_handle_ready = app.handle().clone();
            let event_bus_shortcut = Arc::clone(&event_bus);
            app.listen("lux:frontend-ready", move |_| {
                tracing::info!("Frontend ready, registering shortcut");

                let handle = app_handle_ready.clone();
                let event_bus = event_bus_shortcut.clone();

                if let Err(e) = platform::register_shortcut_with_handler(&handle, event_bus) {
                    tracing::error!("Failed to register shortcut: {}", e);
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
