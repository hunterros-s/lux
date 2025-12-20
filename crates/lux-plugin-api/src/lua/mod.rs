//! Lua bindings for the Plugin API.
//!
//! This module implements the `lux` global namespace with:
//! - `lux.register(plugin)` - Register a plugin
//! - `lux.configure(name, config)` - Configure a registered plugin
//! - `lux.root_view` - Assignable root view

use std::sync::Arc;

use mlua::{Function, Lua, MultiValue, Result as LuaResult, Table, Value};

use crate::keymap::{
    generate_handler_id, BuiltInHotkey, GlobalHandler, KeyHandler, PendingBinding, PendingHotkey,
};
use crate::registry::PluginRegistry;
use crate::types::LuaFunctionRef;

pub mod bridge;
mod parse;

pub use bridge::{
    call_action_run, call_source_search, call_trigger_run, call_view_on_select,
    call_view_on_submit, cleanup_view_registry_keys,
};
pub use parse::*;

/// Register the new `lux` API in a Lua state.
///
/// This creates the minimal spec-compliant API:
/// - `lux.register(plugin)` - Register a plugin with triggers, sources, actions
/// - `lux.configure(name, config)` - Pass configuration to a plugin
/// - `lux.set_root_view` - Set a custom root view
pub fn register_lux_api(lua: &Lua, registry: Arc<PluginRegistry>) -> LuaResult<()> {
    let lux = lua.create_table()?;

    // lux.register(plugin)
    {
        let registry = Arc::clone(&registry);
        let register_fn = lua.create_function(move |lua, table: Table| {
            let plugin = parse_plugin(lua, table)?;

            registry
                .register(plugin)
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;

            Ok(())
        })?;
        lux.set("register", register_fn)?;
    }

    // lux.configure(name, config)
    {
        let registry = Arc::clone(&registry);
        let configure_fn = lua.create_function(move |lua, (name, config): (String, Value)| {
            let config_json = lua_value_to_json(lua, config)?;

            registry
                .configure(&name, config_json, lua)
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;

            Ok(())
        })?;
        lux.set("configure", configure_fn)?;
    }

    // lux.set_root_view(view)
    {
        let registry = Arc::clone(&registry);
        let set_root_view_fn = lua.create_function(move |lua, table: Table| {
            let view = parse_view(lua, table)?;

            registry.set_root_view(view);
            Ok(())
        })?;
        lux.set("set_root_view", set_root_view_fn)?;
    }

    // lux.builtin namespace (for helper functions)
    let builtin = lua.create_table()?;

    // lux.builtin.search_all(ctx) - Search all root sources
    {
        let _registry = Arc::clone(&registry);
        let search_all_fn = lua.create_function(move |lua, _ctx: Table| {
            // Placeholder - actual implementation will call all root sources
            // and merge results
            let result = lua.create_table()?;
            Ok(result)
        })?;
        builtin.set("search_all", search_all_fn)?;
    }

    lux.set("builtin", builtin)?;

    // lux.keymap namespace
    let keymap_table = lua.create_table()?;

    // lux.keymap.set(key, handler, opts?)
    //
    // Examples:
    //   lux.keymap.set("ctrl+n", "cursor_down")
    //   lux.keymap.set("ctrl+n", "cursor_down", { context = "Launcher" })
    //   lux.keymap.set("enter", "submit", { context = "SearchInput" })
    //   lux.keymap.set("ctrl+o", "open_finder", { context = "Launcher", view = "files" })
    //   lux.keymap.set("ctrl+d", function(ctx) ... end, { view = "files" })
    {
        let registry = Arc::clone(&registry);
        let set_fn = lua.create_function(move |lua, args: MultiValue| {
            let mut args_iter = args.into_iter();

            // First arg: key (required)
            let key: String = match args_iter.next() {
                Some(v) => lua
                    .unpack(v)
                    .map_err(|_| mlua::Error::RuntimeError("key must be a string".to_string()))?,
                None => {
                    return Err(mlua::Error::RuntimeError(
                        "keymap.set requires key argument".to_string(),
                    ))
                }
            };

            // Second arg: handler (required) - string or function
            let handler_val = match args_iter.next() {
                Some(v) => v,
                None => {
                    return Err(mlua::Error::RuntimeError(
                        "keymap.set requires handler argument".to_string(),
                    ))
                }
            };

            // Third arg: opts (optional)
            let opts: Option<Table> = args_iter.next().and_then(|v| lua.unpack(v).ok());
            let (context, view) = if let Some(ref t) = opts {
                (
                    t.get::<Option<String>>("context").ok().flatten(),
                    t.get::<Option<String>>("view").ok().flatten(),
                )
            } else {
                (None, None)
            };

            // Parse handler
            let handler = if let Ok(action_name) = lua.unpack::<String>(handler_val.clone()) {
                // Action name binding
                KeyHandler::Action(action_name)
            } else if let Ok(func) = lua.unpack::<Function>(handler_val) {
                // Lua function binding - store in registry
                let id = generate_handler_id();
                let func_ref = LuaFunctionRef::from_function(lua, func, id.clone())?;
                registry.keymap().store_lua_handler(id.clone(), func_ref);
                KeyHandler::Function { id }
            } else {
                return Err(mlua::Error::RuntimeError(
                    "handler must be string or function".to_string(),
                ));
            };

            registry.keymap().set(PendingBinding {
                key,
                handler,
                context,
                view,
            });
            Ok(())
        })?;
        keymap_table.set("set", set_fn)?;
    }

    // lux.keymap.del(key, opts?)
    //
    // Remove a keybinding by key, context, and optional view.
    // Note: Only works at startup time before bindings are registered with GPUI.
    //
    // Examples:
    //   lux.keymap.del("enter", { context = "SearchInput" })
    //   lux.keymap.del("up", { context = "Launcher" })
    //   lux.keymap.del("ctrl+d", { context = "Launcher", view = "files" })
    {
        let registry = Arc::clone(&registry);
        let del_fn = lua.create_function(move |lua, args: MultiValue| {
            let mut args_iter = args.into_iter();

            // First arg: key (required)
            let key: String = match args_iter.next() {
                Some(v) => lua
                    .unpack(v)
                    .map_err(|_| mlua::Error::RuntimeError("key must be a string".to_string()))?,
                None => {
                    return Err(mlua::Error::RuntimeError(
                        "keymap.del requires key argument".to_string(),
                    ))
                }
            };

            // Second arg: opts (optional)
            let opts: Option<Table> = args_iter.next().and_then(|v| lua.unpack(v).ok());
            let (context, view) = if let Some(ref t) = opts {
                (
                    t.get::<Option<String>>("context").ok().flatten(),
                    t.get::<Option<String>>("view").ok().flatten(),
                )
            } else {
                (None, None)
            };

            let removed = registry
                .keymap()
                .del(&key, context.as_deref(), view.as_deref());
            Ok(removed)
        })?;
        keymap_table.set("del", del_fn)?;
    }

    // lux.keymap.set_global(key, handler)
    //
    // Examples:
    //   lux.keymap.set_global("cmd+shift+space", "toggle_launcher")
    //   lux.keymap.set_global("cmd+shift+n", function() lux.shell("open -a Notes") end)
    {
        let registry = Arc::clone(&registry);
        let set_global_fn = lua.create_function(move |lua, args: MultiValue| {
            let mut args_iter = args.into_iter();

            // First arg: key (required)
            let key: String = match args_iter.next() {
                Some(v) => lua
                    .unpack(v)
                    .map_err(|_| mlua::Error::RuntimeError("key must be a string".to_string()))?,
                None => {
                    return Err(mlua::Error::RuntimeError(
                        "keymap.set_global requires key argument".to_string(),
                    ))
                }
            };

            // Second arg: handler (required) - string or function
            let handler_val = match args_iter.next() {
                Some(v) => v,
                None => {
                    return Err(mlua::Error::RuntimeError(
                        "keymap.set_global requires handler argument".to_string(),
                    ))
                }
            };

            // Parse handler
            let handler = if let Ok(action_name) = lua.unpack::<String>(handler_val.clone()) {
                // Built-in action
                if let Some(builtin) = BuiltInHotkey::from_name(&action_name) {
                    GlobalHandler::BuiltIn(builtin)
                } else {
                    return Err(mlua::Error::RuntimeError(format!(
                        "Unknown global action: '{}'. Available: toggle_launcher",
                        action_name
                    )));
                }
            } else if let Ok(func) = lua.unpack::<Function>(handler_val) {
                // Lua function binding - store in registry
                let id = generate_handler_id();
                let func_ref = LuaFunctionRef::from_function(lua, func, id.clone())?;
                registry.keymap().store_lua_handler(id.clone(), func_ref);
                GlobalHandler::Function { id }
            } else {
                return Err(mlua::Error::RuntimeError(
                    "handler must be string or function".to_string(),
                ));
            };

            registry.keymap().set_global(PendingHotkey { key, handler });
            Ok(())
        })?;
        keymap_table.set("set_global", set_global_fn)?;
    }

    // lux.keymap.del_global(key)
    //
    // Remove a global hotkey by key string.
    // Note: Only works at startup time before hotkeys are registered with the OS.
    {
        let registry = Arc::clone(&registry);
        let del_global_fn = lua.create_function(move |lua, key: Value| {
            let key: String = lua
                .unpack(key)
                .map_err(|_| mlua::Error::RuntimeError("key must be a string".to_string()))?;

            let removed = registry.keymap().del_global(&key);
            Ok(removed)
        })?;
        keymap_table.set("del_global", del_global_fn)?;
    }

    lux.set("keymap", keymap_table)?;

    // lux.shell(command, opts?) - Execute a shell command with timeout
    {
        let shell_fn = lua.create_function(|lua, (command, opts): (String, Option<Table>)| {
            use std::io::Read;
            use std::process::{Command, Stdio};
            use std::time::Duration;
            use wait_timeout::ChildExt;

            let timeout_ms = opts
                .as_ref()
                .and_then(|o| o.get::<Option<u64>>("timeout_ms").ok().flatten())
                .unwrap_or(30_000);

            let cwd = opts
                .as_ref()
                .and_then(|o| o.get::<Option<String>>("cwd").ok().flatten());

            let mut cmd = Command::new("sh");
            cmd.args(["-c", &command])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            if let Some(dir) = cwd {
                cmd.current_dir(dir);
            }

            let mut child = cmd
                .spawn()
                .map_err(|e| mlua::Error::RuntimeError(format!("Command spawn failed: {}", e)))?;

            let timeout = Duration::from_millis(timeout_ms);

            // Wait for process with timeout
            let status = match child.wait_timeout(timeout) {
                Ok(Some(status)) => status,
                Ok(None) => {
                    // Timeout expired - kill the process
                    let _ = child.kill();
                    let _ = child.wait(); // Reap the zombie process

                    let result = lua.create_table()?;
                    result.set("stdout", "")?;
                    result.set(
                        "stderr",
                        format!("Command timed out after {}ms", timeout_ms),
                    )?;
                    result.set("exit_code", -1)?;
                    result.set("success", false)?;
                    result.set("timed_out", true)?;
                    return Ok(result);
                }
                Err(e) => {
                    return Err(mlua::Error::RuntimeError(format!(
                        "Command wait failed: {}",
                        e
                    )));
                }
            };

            // Process completed - read stdout and stderr
            let mut stdout = String::new();
            let mut stderr = String::new();

            if let Some(mut stdout_handle) = child.stdout.take() {
                let _ = stdout_handle.read_to_string(&mut stdout);
            }
            if let Some(mut stderr_handle) = child.stderr.take() {
                let _ = stderr_handle.read_to_string(&mut stderr);
            }

            let result = lua.create_table()?;
            result.set("stdout", stdout)?;
            result.set("stderr", stderr)?;
            result.set("exit_code", status.code().unwrap_or(-1))?;
            result.set("success", status.success())?;
            result.set("timed_out", false)?;

            Ok(result)
        })?;
        lux.set("shell", shell_fn)?;
    }

    // lux.icon(app_path) - Get icon file path for macOS app (converts to PNG)
    {
        let icon_fn = lua.create_function(|_lua, app_path: String| {
            use std::process::Command;
            use std::path::Path;

            // Create cache directory
            let cache_dir = dirs::cache_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                .join("lux")
                .join("icons");
            std::fs::create_dir_all(&cache_dir).ok();

            // Generate cache filename from app path hash
            let hash = {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                app_path.hash(&mut hasher);
                hasher.finish()
            };
            let cached_png = cache_dir.join(format!("{:x}.png", hash));

            // Return cached version if exists
            if cached_png.exists() {
                return Ok(Some(cached_png.to_string_lossy().to_string()));
            }

            // Find and convert .icns to PNG
            let script = format!(
                r#"
                icon_name=$(/usr/bin/defaults read "{}/Contents/Info.plist" CFBundleIconFile 2>/dev/null || echo "AppIcon")
                icon_name="${{icon_name%.icns}}.icns"
                icon_path="{}/Contents/Resources/$icon_name"
                if [ ! -f "$icon_path" ]; then
                    icon_path="{}/Contents/Resources/AppIcon.icns"
                fi
                if [ -f "$icon_path" ]; then
                    /usr/bin/sips -s format png -z 64 64 "$icon_path" --out "{}" >/dev/null 2>&1 && echo "{}"
                fi
                "#,
                app_path, app_path, app_path,
                cached_png.display(), cached_png.display()
            );

            let output = Command::new("sh")
                .args(["-c", &script])
                .output()
                .map_err(|e| mlua::Error::RuntimeError(format!("Icon conversion failed: {}", e)))?;

            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() || !Path::new(&path).exists() {
                Ok(None)
            } else {
                Ok(Some(path))
            }
        })?;
        lux.set("icon", icon_fn)?;
    }

    // Set as global
    lua.globals().set("lux", lux)?;

    Ok(())
}

/// Convert a Lua value to a JSON value.
pub fn lua_value_to_json(_lua: &Lua, value: Value) -> LuaResult<serde_json::Value> {
    match value {
        Value::Nil => Ok(serde_json::Value::Null),
        Value::Boolean(b) => Ok(serde_json::Value::Bool(b)),
        Value::Integer(i) => Ok(serde_json::Value::Number(i.into())),
        Value::Number(n) => {
            if let Some(num) = serde_json::Number::from_f64(n) {
                Ok(serde_json::Value::Number(num))
            } else {
                Ok(serde_json::Value::Null)
            }
        }
        Value::String(s) => Ok(serde_json::Value::String(s.to_str()?.to_string())),
        Value::Table(t) => {
            // Check if it's an array or object
            let is_array = t.clone().pairs::<i64, Value>().all(|r| r.is_ok());

            if is_array && t.raw_len() > 0 {
                let mut arr = Vec::new();
                for pair in t.pairs::<i64, Value>() {
                    let (_, v) = pair?;
                    arr.push(lua_value_to_json(_lua, v)?);
                }
                Ok(serde_json::Value::Array(arr))
            } else {
                let mut obj = serde_json::Map::new();
                for pair in t.pairs::<String, Value>() {
                    let (k, v) = pair?;
                    obj.insert(k, lua_value_to_json(_lua, v)?);
                }
                Ok(serde_json::Value::Object(obj))
            }
        }
        _ => Ok(serde_json::Value::Null),
    }
}

/// Convert a JSON value to a Lua value.
pub fn json_to_lua_value(lua: &Lua, value: &serde_json::Value) -> LuaResult<Value> {
    match value {
        serde_json::Value::Null => Ok(Value::Nil),
        serde_json::Value::Bool(b) => Ok(Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(f))
            } else {
                Ok(Value::Nil)
            }
        }
        serde_json::Value::String(s) => Ok(Value::String(lua.create_string(s)?)),
        serde_json::Value::Array(arr) => {
            let table = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                table.set(i + 1, json_to_lua_value(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
        serde_json::Value::Object(obj) => {
            let table = lua.create_table()?;
            for (k, v) in obj {
                table.set(k.as_str(), json_to_lua_value(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}
