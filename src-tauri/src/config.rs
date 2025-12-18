//! Global configuration loading from init.lua.
//!
//! init.lua at `~/.config/lux/init.lua` is THE entry point for all user
//! configuration. Users `require()` everything explicitly - no magic loading.
//!
//! Directory structure:
//! ```text
//! ~/.config/lux/
//! ├── init.lua          # Entry point (created automatically if missing)
//! └── *.lua             # User modules (require("foo") finds foo.lua here)
//! ```
//!
//! Users can organize however they want - files at the root or in subdirectories.

use std::path::PathBuf;
use std::sync::Arc;

use mlua::{Lua, Table};

use crate::plugin_api::{register_lux_api, PluginRegistry};

/// Get the path to the init.lua configuration file.
pub fn config_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".config").join("lux").join("init.lua"))
        .unwrap_or_else(|| PathBuf::from("~/.config/lux/init.lua"))
}

/// Get the config directory path.
pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".config").join("lux"))
        .unwrap_or_else(|| PathBuf::from("~/.config/lux"))
}

/// Load and execute init.lua with the Plugin API.
///
/// This uses PluginRegistry and registers the `lux` API:
/// - `lux.register(plugin)` - Register plugins with triggers, sources, actions
/// - `lux.configure(name, config)` - Configure a registered plugin
/// - `lux.set_root_view(view)` - Set a custom root view
///
/// Returns Ok(Some(lua)) if init.lua loaded successfully,
/// Ok(None) if init.lua doesn't exist,
/// Err if init.lua exists but failed to execute.
pub fn load_init_lua(registry: Arc<PluginRegistry>) -> Result<Option<Lua>, ConfigError> {
    let path = config_path();

    // If init.lua doesn't exist, that's OK - run with defaults
    if !path.exists() {
        tracing::info!("No init.lua found at {:?}", path);
        return Ok(None);
    }

    tracing::info!("Loading init.lua from {:?}", path);

    // Read the init.lua file
    let code = std::fs::read_to_string(&path).map_err(|e| ConfigError::IoError {
        path: path.clone(),
        error: e.to_string(),
    })?;

    // Create Lua state
    let lua = Lua::new();

    // Register Plugin API
    register_lux_api(&lua, Arc::clone(&registry)).map_err(|e| ConfigError::LuaError {
        message: format!("Failed to register lux API: {}", e),
    })?;

    // Add ~/.config/lux/ to package.path so require() works
    // Users can organize files however they want (like Neovim)
    let config_path = config_dir();
    if let Err(e) = setup_package_path(&lua, &config_path) {
        tracing::warn!("Failed to set up package.path: {}", e);
    }

    // Execute init.lua
    lua.load(&code).exec().map_err(|e| ConfigError::LuaError {
        message: format!("Error in init.lua: {}", e),
    })?;

    tracing::info!("Successfully loaded init.lua");
    tracing::info!(
        "Registered {} plugins, {} triggers, {} sources, {} actions",
        registry.list_plugins().len(),
        registry.trigger_count(),
        registry.source_count(),
        registry.action_count()
    );

    Ok(Some(lua))
}

/// Add a directory to Lua's package.path for require() to find modules.
fn setup_package_path(lua: &Lua, lua_dir: &PathBuf) -> Result<(), mlua::Error> {
    let package: Table = lua.globals().get("package")?;
    let current_path: String = package.get("path")?;

    // Add both ?.lua and ?/init.lua patterns
    let lua_dir_str = lua_dir.to_string_lossy();
    let new_path = format!(
        "{}/?.lua;{}/?/init.lua;{}",
        lua_dir_str, lua_dir_str, current_path
    );
    package.set("path", new_path)?;

    tracing::debug!("Added {} to package.path", lua_dir_str);
    Ok(())
}

/// Default init.lua content for new installations.
const DEFAULT_INIT_LUA: &str = r#"-- Lux configuration
-- See https://github.com/example/lux for documentation

-- Example: register a simple plugin
-- lux.register({
--     name = "my-plugin",
--     sources = {
--         {
--             name = "example",
--             search = function(ctx)
--                 return {}
--             end,
--         },
--     },
-- })
"#;

/// Ensure the config directory and default init.lua exist.
pub fn ensure_config_dir() -> Result<(), std::io::Error> {
    let dir = config_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }

    // Create default init.lua if it doesn't exist
    let init_path = config_path();
    if !init_path.exists() {
        std::fs::write(&init_path, DEFAULT_INIT_LUA)?;
        tracing::info!("Created default init.lua at {:?}", init_path);
    }

    Ok(())
}

/// Configuration loading errors.
#[derive(Debug)]
pub enum ConfigError {
    IoError { path: PathBuf, error: String },
    LuaError { message: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::IoError { path, error } => {
                write!(f, "Failed to read {:?}: {}", path, error)
            }
            ConfigError::LuaError { message } => write!(f, "{}", message),
        }
    }
}

impl std::error::Error for ConfigError {}
