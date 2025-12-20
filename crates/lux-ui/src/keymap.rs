//! GPUI keybinding registration.
//!
//! This module provides `apply_keybindings()` which registers all pending
//! keybindings from the KeymapRegistry with GPUI at startup.
//!
//! ## Binding Order
//!
//! GPUI uses last-wins semantics: later bindings override earlier ones at the
//! same context depth. We register defaults first, then user bindings, so user
//! bindings take precedence.

use std::rc::Rc;

use gpui::{App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate, Keystroke};

use lux_plugin_api::{KeyHandler, KeymapRegistry, PendingBinding};

use crate::actions::{action_from_name, RunLuaHandler};

// =============================================================================
// Keystroke Parsing
// =============================================================================

/// Convert user-friendly keystroke to GPUI format.
///
/// Users write: "ctrl+n" or "cmd+shift+z"
/// GPUI expects: "ctrl-n" or "cmd-shift-z"
fn normalize_keystroke(s: &str) -> String {
    s.replace('+', "-")
}

/// Parse keystroke string to GPUI Keystroke.
///
/// Accepts both "ctrl+n" and "ctrl-n" formats.
fn parse_keystroke(s: &str) -> Result<Keystroke, String> {
    let normalized = normalize_keystroke(s);
    Keystroke::parse(&normalized).map_err(|e| format!("Invalid keystroke '{}': {:?}", s, e))
}

// =============================================================================
// Context Building
// =============================================================================

/// Build GPUI context predicate for view-specific binding.
///
/// - Global bindings: context = "Launcher"
/// - View-specific: context = "Launcher && view_id == file_browser"
fn build_context_predicate(view: Option<&str>) -> Option<Rc<KeyBindingContextPredicate>> {
    let context_str = match view {
        None => "Launcher".to_string(),
        Some(v) => format!("Launcher && view_id == {}", v),
    };

    KeyBindingContextPredicate::parse(&context_str)
        .ok()
        .map(|p| Rc::new(p))
}

// =============================================================================
// Apply Keybindings
// =============================================================================

/// Apply all pending bindings to GPUI.
///
/// This should be called after Lua config is loaded but before the UI shows.
/// It takes all pending bindings from the registry and registers them with GPUI.
///
/// Default bindings should be registered first via `register_default_bindings()`,
/// then user bindings via this function. GPUI uses last-wins semantics, so user
/// bindings will override defaults.
pub fn apply_keybindings(keymap: &KeymapRegistry, cx: &mut App) {
    let bindings = keymap.take_bindings();

    for pending in bindings {
        apply_binding(pending, cx);
    }
}

/// Apply a single binding to GPUI.
fn apply_binding(pending: PendingBinding, cx: &mut App) {
    let context_predicate = build_context_predicate(pending.view.as_deref());
    let keystroke = normalize_keystroke(&pending.key);

    match pending.handler {
        KeyHandler::Action(name) => {
            // Look up built-in action and register using KeyBinding::load
            if let Some(action) = action_from_name(&name) {
                match KeyBinding::load(
                    &keystroke,
                    action,
                    context_predicate,
                    false, // use_key_equivalents
                    None,  // action_input
                    &DummyKeyboardMapper,
                ) {
                    Ok(binding) => {
                        cx.bind_keys([binding]);
                        tracing::debug!(
                            "Registered action binding: {} -> {} (view: {:?})",
                            pending.key,
                            name,
                            pending.view
                        );
                    }
                    Err(e) => {
                        tracing::warn!("Failed to create binding for '{}': {:?}", pending.key, e);
                    }
                }
            } else {
                tracing::warn!("Unknown action: {}", name);
            }
        }
        KeyHandler::Function { id } => {
            // Register RunLuaHandler - GPUI dispatches, we look up function
            let action = RunLuaHandler { id: id.clone() };
            match KeyBinding::load(
                &keystroke,
                Box::new(action),
                context_predicate,
                false, // use_key_equivalents
                None,  // action_input
                &DummyKeyboardMapper,
            ) {
                Ok(binding) => {
                    cx.bind_keys([binding]);
                    tracing::debug!(
                        "Registered Lua handler binding: {} -> {} (view: {:?})",
                        pending.key,
                        id,
                        pending.view
                    );
                }
                Err(e) => {
                    tracing::warn!("Failed to create binding for '{}': {:?}", pending.key, e);
                }
            }
        }
    }
}

// =============================================================================
// Default Bindings
// =============================================================================

/// Register default keybindings.
///
/// These are the base bindings that users can override via `lux.keymap.set()`.
/// Call this BEFORE `apply_keybindings()` so user bindings take precedence.
pub fn register_default_bindings(cx: &mut App) {
    use crate::actions::*;

    // Navigation - Launcher context
    cx.bind_keys([
        KeyBinding::new("up", CursorUp, Some("Launcher")),
        KeyBinding::new("down", CursorDown, Some("Launcher")),
        KeyBinding::new("tab", OpenActionMenu, Some("Launcher")),
        KeyBinding::new("cmd-enter", ToggleSelection, Some("Launcher")),
        KeyBinding::new("escape", Dismiss, Some("Launcher")),
    ]);

    // Text editing - SearchInput context
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("SearchInput")),
        KeyBinding::new("delete", Delete, Some("SearchInput")),
        KeyBinding::new("left", MoveLeft, Some("SearchInput")),
        KeyBinding::new("right", MoveRight, Some("SearchInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("SearchInput")),
        KeyBinding::new("shift-right", SelectRight, Some("SearchInput")),
        KeyBinding::new("cmd-a", TextSelectAll, Some("SearchInput")),
        KeyBinding::new("home", Home, Some("SearchInput")),
        KeyBinding::new("end", End, Some("SearchInput")),
        KeyBinding::new("cmd-c", Copy, Some("SearchInput")),
        KeyBinding::new("cmd-v", Paste, Some("SearchInput")),
        KeyBinding::new("cmd-x", Cut, Some("SearchInput")),
        KeyBinding::new("enter", Submit, Some("SearchInput")),
        // Note: escape is handled by Launcher context, not here
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_keystroke() {
        assert_eq!(normalize_keystroke("ctrl+n"), "ctrl-n");
        assert_eq!(normalize_keystroke("cmd+shift+z"), "cmd-shift-z");
        assert_eq!(normalize_keystroke("ctrl-n"), "ctrl-n"); // Already normalized
    }

    #[test]
    fn test_parse_keystroke() {
        assert!(parse_keystroke("ctrl+n").is_ok());
        assert!(parse_keystroke("cmd-shift-z").is_ok());
    }
}
