//! Lua bindings for the Plugin API.
//!
//! This module implements the `lux` global namespace with:
//! - `lux.register(plugin)` - Register a plugin
//! - `lux.configure(name, config)` - Configure a registered plugin
//! - `lux.root_view` - Assignable root view

use std::sync::Arc;

use mlua::{Lua, Result as LuaResult, Table, Value};

use crate::plugin_api::registry::PluginRegistry;

mod parse;

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

    // lux.icon(app_path) - Extract app icon as base64 data URL (macOS)
    {
        let icon_fn = lua.create_function(|_lua, app_path: String| {
            use std::process::Command;

            // Use sips to convert .app icon to PNG, then base64 encode
            let script = format!(
                r#"
                icon_path=$(/usr/bin/defaults read "{}/Contents/Info.plist" CFBundleIconFile 2>/dev/null || echo "AppIcon")
                icon_path="${{icon_path%.icns}}.icns"
                icon_full="{}/Contents/Resources/$icon_path"
                if [ ! -f "$icon_full" ]; then
                    icon_full="{}/Contents/Resources/AppIcon.icns"
                fi
                if [ -f "$icon_full" ]; then
                    /usr/bin/sips -s format png -z 64 64 "$icon_full" --out /tmp/lux_icon_$$.png >/dev/null 2>&1
                    /usr/bin/base64 -i /tmp/lux_icon_$$.png
                    rm -f /tmp/lux_icon_$$.png
                fi
                "#,
                app_path, app_path, app_path
            );

            let output = Command::new("sh")
                .args(["-c", &script])
                .output()
                .map_err(|e| mlua::Error::RuntimeError(format!("Icon extraction failed: {}", e)))?;

            if output.status.success() {
                let base64 = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !base64.is_empty() {
                    return Ok(Some(format!("data:image/png;base64,{}", base64)));
                }
            }
            Ok(None)
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
