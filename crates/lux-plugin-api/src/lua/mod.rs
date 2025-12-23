//! Lua bindings for the Plugin API.
//!
//! This module implements the `lux` global namespace with:
//! - `lux.views.add/get/list()` - View registry
//! - `lux.set_root(view)` - Set the root view
//! - `lux.hook(path, fn)` - Register hooks
//! - `lux.keymap.set/del/set_global/del_global()` - Keybindings
//! - `lux.shell/clipboard/fs/ui` - Utilities

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
    call_action_run, call_get_actions, call_hooked_search, call_source_search, call_trigger_run,
    call_view_on_select, call_view_on_submit, cleanup_view_registry_keys, ParsedAction,
};
pub use parse::*;

use crate::hooks::validate_hook_path;
use crate::views::ViewRegistryError;

/// Register the new `lux` API in a Lua state.
///
/// Create the Lua API for the plugin system.
///
/// This creates the spec-compliant API:
/// - `lux.views.add/get/list()` - View registry
/// - `lux.set_root(view)` - Set the root view
/// - `lux.hook(path, fn)` - Register hooks
/// - `lux.keymap.set/del/set_global/del_global()` - Keybindings
/// - `lux.shell/clipboard/fs/ui` - Utilities
pub fn register_lux_api(lua: &Lua, registry: Arc<PluginRegistry>) -> LuaResult<()> {
    let lux = lua.create_table()?;

    // lux.set_root_view(view) - legacy alias
    {
        let registry = Arc::clone(&registry);
        let set_root_view_fn = lua.create_function(move |lua, table: Table| {
            let view = parse_view(lua, table)?;

            registry.set_root_view(view);
            Ok(())
        })?;
        lux.set("set_root_view", set_root_view_fn)?;
    }

    // lux.set_root(view) - new API
    {
        let registry = Arc::clone(&registry);
        let set_root_fn = lua.create_function(move |lua, table: Table| {
            let view = parse_view(lua, table)?;

            registry.set_root_view(view);
            Ok(())
        })?;
        lux.set("set_root", set_root_fn)?;
    }

    // lux.views namespace
    let views_table = lua.create_table()?;

    // lux.views.add(def) - register a view
    {
        let registry = Arc::clone(&registry);
        let add_fn = lua.create_function(move |lua, table: Table| {
            let view_def = parse_view_definition(lua, table)?;
            let view_registry = registry.views();

            view_registry.add(view_def).map_err(|e| match e {
                ViewRegistryError::ViewAlreadyExists(id) => {
                    mlua::Error::RuntimeError(format!("View '{}' already exists", id))
                }
                _ => mlua::Error::RuntimeError(e.to_string()),
            })?;

            Ok(())
        })?;
        views_table.set("add", add_fn)?;
    }

    // lux.views.get(id) - get a view by ID (returns table with id, title, placeholder, selection)
    {
        let registry = Arc::clone(&registry);
        let get_fn = lua.create_function(move |lua, id: String| {
            let view_registry = registry.views();

            match view_registry.with_view(&id, |view| {
                let table = lua.create_table()?;
                table.set("id", view.id.as_str())?;
                if let Some(ref title) = view.title {
                    table.set("title", title.as_str())?;
                }
                if let Some(ref placeholder) = view.placeholder {
                    table.set("placeholder", placeholder.as_str())?;
                }
                let selection_str = match view.selection {
                    lux_core::SelectionMode::Single => "single",
                    lux_core::SelectionMode::Multi => "multi",
                    lux_core::SelectionMode::Custom => "custom",
                };
                table.set("selection", selection_str)?;
                Ok::<_, mlua::Error>(table)
            }) {
                Some(result) => result.map(Value::Table),
                None => Ok(Value::Nil),
            }
        })?;
        views_table.set("get", get_fn)?;
    }

    // lux.views.list() - list all registered view IDs
    {
        let registry = Arc::clone(&registry);
        let list_fn = lua.create_function(move |lua, ()| {
            let view_registry = registry.views();
            let ids = view_registry.list();

            let table = lua.create_table()?;
            for (i, id) in ids.iter().enumerate() {
                table.set(i + 1, id.as_str())?;
            }

            Ok(table)
        })?;
        views_table.set("list", list_fn)?;
    }

    lux.set("views", views_table)?;

    // lux.hook(path, fn) - register a hook, returns unhook function
    {
        let registry = Arc::clone(&registry);
        let hook_fn = lua.create_function(move |lua, (path, func): (String, Function)| {
            // Validate hook path
            validate_hook_path(&path).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;

            // Generate a unique key and store the function
            let key = format!("hook:{}:{}", path, generate_handler_id());
            let func_ref = LuaFunctionRef::from_function(lua, func, key.clone())?;

            // Add to hook registry
            let hook_registry = registry.hooks();
            let hook_id = hook_registry.add(&path, func_ref);

            // Create unhook function
            let registry_for_unhook = Arc::clone(&registry);
            let hook_id_for_unhook = hook_id.clone();
            let unhook_fn = lua.create_function(move |_lua, ()| {
                let removed = registry_for_unhook.hooks().remove(&hook_id_for_unhook);
                Ok(removed)
            })?;

            Ok(unhook_fn)
        })?;
        lux.set("hook", hook_fn)?;
    }

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
    // Examples:
    //   lux.keymap.del("ctrl+n")
    //   lux.keymap.del("ctrl+n", { view = "files" })
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
    // Remove a global hotkey.
    //
    // Examples:
    //   lux.keymap.del_global("cmd+space")
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

    // lux.shell - Shell command execution namespace
    //
    // Usage:
    //   lux.shell("open", path)         -- async fire-and-forget
    //   lux.shell.sync("ls", "-la")     -- blocking, returns output
    //   lux.shell.run({cmd, cwd, env})  -- advanced options
    {
        let shell_table = lua.create_table()?;

        // lux.shell.sync(command) - Blocking execution, returns output
        let sync_fn = lua.create_function(|lua, command: String| {
            use std::io::Read;
            use std::process::{Command, Stdio};
            use std::time::Duration;
            use wait_timeout::ChildExt;

            let timeout_ms = 30_000u64;

            let mut cmd = Command::new("sh");
            cmd.args(["-c", &command])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let mut child = cmd
                .spawn()
                .map_err(|e| mlua::Error::RuntimeError(format!("Command spawn failed: {}", e)))?;

            let timeout = Duration::from_millis(timeout_ms);

            let status = match child.wait_timeout(timeout) {
                Ok(Some(status)) => status,
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();

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
        shell_table.set("sync", sync_fn)?;

        // lux.shell.run({ cmd, cwd?, env?, timeout_ms? }) - Advanced options
        let run_fn = lua.create_function(|lua, opts: Table| {
            use std::io::Read;
            use std::process::{Command, Stdio};
            use std::time::Duration;
            use wait_timeout::ChildExt;

            let command: String = opts.get("cmd").map_err(|_| {
                mlua::Error::RuntimeError("shell.run requires 'cmd' field".to_string())
            })?;

            let timeout_ms = opts
                .get::<Option<u64>>("timeout_ms")
                .ok()
                .flatten()
                .unwrap_or(30_000);

            let cwd = opts.get::<Option<String>>("cwd").ok().flatten();

            let env: Option<Table> = opts.get("env").ok();

            let mut cmd = Command::new("sh");
            cmd.args(["-c", &command])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            if let Some(dir) = cwd {
                cmd.current_dir(dir);
            }

            if let Some(env_table) = env {
                for pair in env_table.pairs::<String, String>() {
                    if let Ok((key, value)) = pair {
                        cmd.env(key, value);
                    }
                }
            }

            let mut child = cmd
                .spawn()
                .map_err(|e| mlua::Error::RuntimeError(format!("Command spawn failed: {}", e)))?;

            let timeout = Duration::from_millis(timeout_ms);

            let status = match child.wait_timeout(timeout) {
                Ok(Some(status)) => status,
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();

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
        shell_table.set("run", run_fn)?;

        // Set __call metamethod for lux.shell("command", ...) - fire-and-forget
        let metatable = lua.create_table()?;
        let call_fn = lua.create_function(|_lua, args: MultiValue| {
            use std::process::{Command, Stdio};

            let mut args_iter = args.into_iter();
            args_iter.next(); // Skip 'self' (the shell table)

            // Collect all arguments as strings and join them
            let parts: Vec<String> = args_iter
                .filter_map(|v| match v {
                    Value::String(s) => Some(s.to_str().ok()?.to_string()),
                    Value::Number(n) => Some(n.to_string()),
                    Value::Integer(i) => Some(i.to_string()),
                    _ => None,
                })
                .collect();

            if parts.is_empty() {
                return Err(mlua::Error::RuntimeError(
                    "shell() requires at least one argument".to_string(),
                ));
            }

            let command = parts.join(" ");

            // Fire-and-forget: spawn detached process
            Command::new("sh")
                .args(["-c", &command])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| mlua::Error::RuntimeError(format!("Command spawn failed: {}", e)))?;

            Ok(())
        })?;
        metatable.set("__call", call_fn)?;
        shell_table.set_metatable(Some(metatable))?;

        lux.set("shell", shell_table)?;
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

    // lux.clipboard - Clipboard operations
    {
        let clipboard_table = lua.create_table()?;

        // lux.clipboard.read() - Read text from clipboard
        let read_fn = lua.create_function(|_lua, ()| {
            use std::process::Command;

            let output = Command::new("pbpaste")
                .output()
                .map_err(|e| mlua::Error::RuntimeError(format!("Clipboard read failed: {}", e)))?;

            if output.status.success() {
                Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()))
            } else {
                Ok(None)
            }
        })?;
        clipboard_table.set("read", read_fn)?;

        // lux.clipboard.write(text) - Write text to clipboard
        let write_fn = lua.create_function(|_lua, text: String| {
            use std::io::Write;
            use std::process::{Command, Stdio};

            let mut child = Command::new("pbcopy")
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|e| mlua::Error::RuntimeError(format!("Clipboard write failed: {}", e)))?;

            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes()).map_err(|e| {
                    mlua::Error::RuntimeError(format!("Clipboard write failed: {}", e))
                })?;
            }

            let status = child
                .wait()
                .map_err(|e| mlua::Error::RuntimeError(format!("Clipboard write failed: {}", e)))?;

            Ok(status.success())
        })?;
        clipboard_table.set("write", write_fn)?;

        lux.set("clipboard", clipboard_table)?;
    }

    // lux.fs - Filesystem operations
    {
        let fs_table = lua.create_table()?;

        // lux.fs.read(path) - Read file contents
        let read_fn =
            lua.create_function(|_lua, path: String| match std::fs::read_to_string(&path) {
                Ok(content) => Ok(Some(content)),
                Err(_) => Ok(None),
            })?;
        fs_table.set("read", read_fn)?;

        // lux.fs.write(path, content) - Write content to file
        let write_fn = lua.create_function(|_lua, (path, content): (String, String)| {
            std::fs::write(&path, content)
                .map_err(|e| mlua::Error::RuntimeError(format!("File write failed: {}", e)))?;
            Ok(true)
        })?;
        fs_table.set("write", write_fn)?;

        // lux.fs.exists(path) - Check if path exists
        let exists_fn =
            lua.create_function(|_lua, path: String| Ok(std::path::Path::new(&path).exists()))?;
        fs_table.set("exists", exists_fn)?;

        // lux.fs.is_dir(path) - Check if path is a directory
        let is_dir_fn =
            lua.create_function(|_lua, path: String| Ok(std::path::Path::new(&path).is_dir()))?;
        fs_table.set("is_dir", is_dir_fn)?;

        // lux.fs.list(dir) - List directory contents
        let list_fn = lua.create_function(|lua, dir: String| {
            let entries: Vec<String> = std::fs::read_dir(&dir)
                .map_err(|e| mlua::Error::RuntimeError(format!("Directory read failed: {}", e)))?
                .filter_map(|entry| {
                    entry.ok().and_then(|e| {
                        e.path()
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                    })
                })
                .collect();

            let table = lua.create_table()?;
            for (i, name) in entries.iter().enumerate() {
                table.set(i + 1, name.as_str())?;
            }
            Ok(table)
        })?;
        fs_table.set("list", list_fn)?;

        // lux.fs.glob(pattern) - Glob pattern matching
        let glob_fn = lua.create_function(|lua, pattern: String| {
            use std::process::Command;

            // Use shell glob expansion
            let output = Command::new("sh")
                .args(["-c", &format!("ls -1 {} 2>/dev/null || true", pattern)])
                .output()
                .map_err(|e| mlua::Error::RuntimeError(format!("Glob failed: {}", e)))?;

            let output_str = String::from_utf8_lossy(&output.stdout);
            let paths: Vec<&str> = output_str
                .trim()
                .split('\n')
                .filter(|s| !s.is_empty())
                .collect();

            let table = lua.create_table()?;
            for (i, path) in paths.iter().enumerate() {
                table.set(i + 1, *path)?;
            }
            Ok(table)
        })?;
        fs_table.set("glob", glob_fn)?;

        // lux.fs.home() - Get home directory
        let home_fn = lua.create_function(|_lua, ()| {
            Ok(dirs::home_dir().map(|p| p.to_string_lossy().to_string()))
        })?;
        fs_table.set("home", home_fn)?;

        // lux.fs.config() - Get config directory
        let config_fn = lua.create_function(|_lua, ()| {
            Ok(dirs::config_dir().map(|p| p.to_string_lossy().to_string()))
        })?;
        fs_table.set("config", config_fn)?;

        lux.set("fs", fs_table)?;
    }

    // lux.ui - UI control operations
    // Note: These create effects that need to be handled by the UI layer
    {
        let ui_table = lua.create_table()?;

        // lux.ui.show() - Show the launcher window
        let show_fn = lua.create_function(|_lua, ()| {
            // TODO: Connect to UI layer - for now just log
            tracing::debug!("lux.ui.show() called");
            Ok(())
        })?;
        ui_table.set("show", show_fn)?;

        // lux.ui.hide() - Hide the launcher window
        let hide_fn = lua.create_function(|_lua, ()| {
            tracing::debug!("lux.ui.hide() called");
            Ok(())
        })?;
        ui_table.set("hide", hide_fn)?;

        // lux.ui.toggle() - Toggle the launcher window
        let toggle_fn = lua.create_function(|_lua, ()| {
            tracing::debug!("lux.ui.toggle() called");
            Ok(())
        })?;
        ui_table.set("toggle", toggle_fn)?;

        // lux.ui.notify(message, opts?) - Show a notification
        let notify_fn =
            lua.create_function(|_lua, (message, _opts): (String, Option<Table>)| {
                // TODO: Connect to notification system
                tracing::info!("Notification: {}", message);
                Ok(())
            })?;
        ui_table.set("notify", notify_fn)?;

        lux.set("ui", ui_table)?;
    }

    // lux.item_id(item) - Get stable identity for an item
    {
        let item_id_fn = lua.create_function(|_lua, item: Table| {
            // Try to get 'id' field first, then fall back to 'title'
            if let Ok(id) = item.get::<Option<String>>("id") {
                if let Some(id) = id {
                    return Ok(id);
                }
            }
            if let Ok(title) = item.get::<Option<String>>("title") {
                if let Some(title) = title {
                    return Ok(title);
                }
            }
            Err(mlua::Error::RuntimeError(
                "item_id: item must have 'id' or 'title' field".to_string(),
            ))
        })?;
        lux.set("item_id", item_id_fn)?;
    }

    // lux.map_items(result, fn) - Transform items preserving group structure
    {
        let map_items_fn = lua.create_function(|lua, (result, mapper): (Table, Function)| {
            // Check if result has 'groups' field
            if let Ok(groups) = result.get::<Table>("groups") {
                // Has groups - map each group's items
                let new_groups = lua.create_table()?;
                for pair in groups.pairs::<i64, Table>() {
                    let (idx, group) = pair?;
                    let new_group = lua.create_table()?;

                    // Copy title if present
                    if let Ok(title) = group.get::<Option<String>>("title") {
                        if let Some(t) = title {
                            new_group.set("title", t)?;
                        }
                    }

                    // Map items
                    if let Ok(items) = group.get::<Table>("items") {
                        let new_items = lua.create_table()?;
                        for item_pair in items.pairs::<i64, Value>() {
                            let (i, item) = item_pair?;
                            let mapped: Value = mapper.call(item)?;
                            new_items.set(i, mapped)?;
                        }
                        new_group.set("items", new_items)?;
                    }

                    new_groups.set(idx, new_group)?;
                }

                let new_result = lua.create_table()?;
                new_result.set("groups", new_groups)?;
                Ok(new_result)
            } else {
                // No groups - treat as flat array of items
                let new_items = lua.create_table()?;
                for pair in result.pairs::<i64, Value>() {
                    let (idx, item) = pair?;
                    let mapped: Value = mapper.call(item)?;
                    new_items.set(idx, mapped)?;
                }
                Ok(new_items)
            }
        })?;
        lux.set("map_items", map_items_fn)?;
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
