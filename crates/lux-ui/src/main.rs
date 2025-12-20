//! Lux Launcher - main entry point.
//!
//! Initializes the plugin system, creates the RuntimeBackend,
//! and starts the GPUI application.

use std::sync::Arc;

use lux_lua_runtime::LuaRuntime;
use lux_plugin_api::{
    lua::register_lux_api, BuiltInHotkey, GlobalHandler, KeyHandler, KeymapRegistry,
    PendingBinding, PendingHotkey, PluginRegistry, QueryEngine,
};
use lux_ui::backend::{Backend, RuntimeBackend};
use lux_ui::platform::Hotkey;
use lux_ui::window::run_launcher;
use mlua::Lua;

// =============================================================================
// Configuration
// =============================================================================

/// Get the path to the user's init.lua configuration file.
///
/// Tries paths in order:
/// 1. XDG-style: ~/.config/lux/init.lua (common for CLI tools)
/// 2. Platform config: ~/Library/Application Support/lux/init.lua (macOS)
fn get_config_path() -> Option<std::path::PathBuf> {
    // Try XDG-style first (common for CLI tools)
    if let Some(home) = dirs::home_dir() {
        let xdg_path = home.join(".config").join("lux").join("init.lua");
        if xdg_path.exists() {
            return Some(xdg_path);
        }
    }

    // Fall back to platform-specific config dir
    let config_dir = dirs::config_dir()?;
    let path = config_dir.join("lux").join("init.lua");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

// =============================================================================
// Default Keybindings
// =============================================================================

/// Register default GPUI keybindings via the KeymapRegistry.
///
/// These are registered before user config loads so users can override them
/// with `lux.keymap.del()` + `lux.keymap.set()`.
fn register_default_bindings(keymap: &KeymapRegistry) {
    // Navigation - Launcher context
    keymap.set(PendingBinding {
        key: "up".to_string(),
        handler: KeyHandler::Action("cursor_up".to_string()),
        context: Some("Launcher".to_string()),
        view: None,
    });
    keymap.set(PendingBinding {
        key: "down".to_string(),
        handler: KeyHandler::Action("cursor_down".to_string()),
        context: Some("Launcher".to_string()),
        view: None,
    });
    keymap.set(PendingBinding {
        key: "tab".to_string(),
        handler: KeyHandler::Action("open_action_menu".to_string()),
        context: Some("Launcher".to_string()),
        view: None,
    });
    keymap.set(PendingBinding {
        key: "cmd+enter".to_string(),
        handler: KeyHandler::Action("toggle_selection".to_string()),
        context: Some("Launcher".to_string()),
        view: None,
    });
    keymap.set(PendingBinding {
        key: "escape".to_string(),
        handler: KeyHandler::Action("dismiss".to_string()),
        context: Some("Launcher".to_string()),
        view: None,
    });

    // Text editing - SearchInput context
    keymap.set(PendingBinding {
        key: "backspace".to_string(),
        handler: KeyHandler::Action("backspace".to_string()),
        context: Some("SearchInput".to_string()),
        view: None,
    });
    keymap.set(PendingBinding {
        key: "delete".to_string(),
        handler: KeyHandler::Action("delete".to_string()),
        context: Some("SearchInput".to_string()),
        view: None,
    });
    keymap.set(PendingBinding {
        key: "left".to_string(),
        handler: KeyHandler::Action("move_left".to_string()),
        context: Some("SearchInput".to_string()),
        view: None,
    });
    keymap.set(PendingBinding {
        key: "right".to_string(),
        handler: KeyHandler::Action("move_right".to_string()),
        context: Some("SearchInput".to_string()),
        view: None,
    });
    keymap.set(PendingBinding {
        key: "shift+left".to_string(),
        handler: KeyHandler::Action("select_left".to_string()),
        context: Some("SearchInput".to_string()),
        view: None,
    });
    keymap.set(PendingBinding {
        key: "shift+right".to_string(),
        handler: KeyHandler::Action("select_right".to_string()),
        context: Some("SearchInput".to_string()),
        view: None,
    });
    keymap.set(PendingBinding {
        key: "cmd+a".to_string(),
        handler: KeyHandler::Action("text_select_all".to_string()),
        context: Some("SearchInput".to_string()),
        view: None,
    });
    keymap.set(PendingBinding {
        key: "home".to_string(),
        handler: KeyHandler::Action("home".to_string()),
        context: Some("SearchInput".to_string()),
        view: None,
    });
    keymap.set(PendingBinding {
        key: "end".to_string(),
        handler: KeyHandler::Action("end".to_string()),
        context: Some("SearchInput".to_string()),
        view: None,
    });
    keymap.set(PendingBinding {
        key: "cmd+c".to_string(),
        handler: KeyHandler::Action("copy".to_string()),
        context: Some("SearchInput".to_string()),
        view: None,
    });
    keymap.set(PendingBinding {
        key: "cmd+v".to_string(),
        handler: KeyHandler::Action("paste".to_string()),
        context: Some("SearchInput".to_string()),
        view: None,
    });
    keymap.set(PendingBinding {
        key: "cmd+x".to_string(),
        handler: KeyHandler::Action("cut".to_string()),
        context: Some("SearchInput".to_string()),
        view: None,
    });
    keymap.set(PendingBinding {
        key: "enter".to_string(),
        handler: KeyHandler::Action("submit".to_string()),
        context: Some("SearchInput".to_string()),
        view: None,
    });

    tracing::debug!(
        "Registered {} default GPUI bindings",
        keymap.binding_count()
    );
}

// =============================================================================
// Initialization
// =============================================================================

/// Initialize the plugin system and create the backend.
///
/// This sets up:
/// 1. PluginRegistry - holds all registered plugins and keymap
/// 2. Lua state with lux API registered
/// 3. Load and execute init.lua (graceful degradation on error)
/// 4. LuaRuntime - moves Lua to dedicated thread
/// 5. QueryEngine - orchestrates plugin execution
/// 6. RuntimeBackend - async interface for UI
///
/// Returns both the backend and keymap registry for GPUI binding registration.
fn create_backend() -> Result<(Arc<RuntimeBackend>, Arc<KeymapRegistry>), String> {
    // Step 1: Create plugin registry
    let registry = Arc::new(PluginRegistry::new());
    tracing::info!("Plugin registry created");

    // Step 2: Create Lua state and register the lux API
    let lua = Lua::new();
    register_lux_api(&lua, registry.clone())
        .map_err(|e| format!("Failed to register Lua API: {}", e))?;
    tracing::info!("Lua API registered");

    // Step 2.5: Register default global hotkey (before user config loads)
    // User can override this in init.lua with lux.keymap.del_global() + set_global()
    registry.keymap().set_global(PendingHotkey {
        key: "cmd+shift+space".to_string(),
        handler: GlobalHandler::BuiltIn(BuiltInHotkey::ToggleLauncher),
    });
    tracing::debug!("Registered default toggle hotkey: cmd+shift+space");

    // Step 2.6: Register default GPUI bindings (before user config loads)
    // User can override these in init.lua with lux.keymap.del() + lux.keymap.set()
    register_default_bindings(registry.keymap().as_ref());

    // Step 3: Load init.lua if it exists (graceful degradation on error)
    if let Some(config_path) = get_config_path() {
        tracing::info!("Loading config from: {}", config_path.display());

        match std::fs::read_to_string(&config_path) {
            Ok(init_lua) => {
                if let Err(e) = lua
                    .load(&init_lua)
                    .set_name(config_path.to_string_lossy())
                    .exec()
                {
                    tracing::error!("init.lua error: {} - continuing with no plugins", e);
                } else {
                    tracing::info!("Config loaded successfully");
                }
            }
            Err(e) => {
                tracing::error!(
                    "Failed to read init.lua: {} - continuing with no plugins",
                    e
                );
            }
        }
    } else {
        tracing::warn!("No init.lua found - using default configuration");
        tracing::info!("Create ~/.config/lux/init.lua to customize");
    }

    // Get keymap from registry (holds Lua function handlers + pending bindings + hotkeys)
    let keymap = registry.keymap();
    tracing::info!(
        "Keymap: {} GPUI bindings, {} global hotkeys, {} Lua handlers",
        keymap.binding_count(),
        keymap.hotkey_count(),
        keymap.handler_count()
    );

    // Step 4: Create query engine (references registry)
    let engine = Arc::new(QueryEngine::new(registry.clone()));
    tracing::info!("Query engine created");

    // Step 5: Move Lua to dedicated runtime thread
    // IMPORTANT: Lua must be moved AFTER loading init.lua
    let runtime = Arc::new(LuaRuntime::new(lua));
    tracing::info!("Lua runtime started");

    // Step 6: Create the backend (connects engine, runtime, and registry)
    let backend = Arc::new(RuntimeBackend::new(engine, runtime, registry));
    tracing::info!("Backend created");

    Ok((backend, keymap))
}

/// Initialize the backend by calling the async initialize method.
///
/// This sets up the root view in the query engine.
/// Uses the existing tokio runtime context from main().
fn initialize_backend(backend: &Arc<RuntimeBackend>) -> Result<(), String> {
    tokio::runtime::Handle::current()
        .block_on(backend.initialize())
        .map_err(|e| format!("Backend initialization failed: {}", e))?;

    tracing::info!("Backend initialized with root view");
    Ok(())
}

// =============================================================================
// Entry Point
// =============================================================================

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("Lux Launcher starting...");

    // Create a tokio runtime and enter its context.
    // This keeps tokio available for the entire lifetime of the app,
    // which is needed for tokio channels used in RuntimeBackend and LuaRuntime.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("Failed to create tokio runtime");
    let _guard = rt.enter();

    // Create and initialize the backend
    let (backend, keymap) = match create_backend() {
        Ok(result) => result,
        Err(e) => {
            tracing::error!("Failed to create backend: {}", e);
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = initialize_backend(&backend) {
        tracing::error!("Failed to initialize backend: {}", e);
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    // Configure hotkey (Cmd+Shift+Space by default)
    // TODO: Load from config file
    let hotkey = Hotkey::default();
    tracing::info!("Hotkey: Cmd+Shift+Space");

    // Run the GPUI application with keymap for binding registration
    tracing::info!("Starting GPUI application...");
    run_launcher(hotkey, backend, keymap);
}
