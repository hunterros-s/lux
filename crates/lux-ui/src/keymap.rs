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

#[cfg(test)]
use gpui::Keystroke;
use gpui::{App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate};

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
#[cfg(test)]
fn parse_keystroke(s: &str) -> Result<Keystroke, String> {
    let normalized = normalize_keystroke(s);
    Keystroke::parse(&normalized).map_err(|e| format!("Invalid keystroke '{}': {:?}", s, e))
}

// =============================================================================
// Context Building
// =============================================================================

/// Build GPUI context predicate from context and optional view ID.
///
/// - No context, no view: context = "Launcher" (default)
/// - Context only: context = "{context}"
/// - Context + view: context = "{context} && view_id == {view}"
/// - No context + view: context = "Launcher && view_id == {view}"
fn build_context_predicate(
    context: Option<&str>,
    view: Option<&str>,
) -> Option<Rc<KeyBindingContextPredicate>> {
    let base = context.unwrap_or("Launcher");
    let context_str = match view {
        Some(v) => format!("{} && view_id == {}", base, v),
        None => base.to_string(),
    };

    KeyBindingContextPredicate::parse(&context_str)
        .ok()
        .map(Rc::new)
}

// =============================================================================
// Apply Keybindings
// =============================================================================

/// Apply all pending bindings to GPUI.
///
/// This should be called after Lua config is loaded but before the UI shows.
/// It takes all pending bindings from the registry and registers them with GPUI.
///
/// Default bindings are registered in `main.rs` before user config loads.
/// User config can override them via `lux.keymap.del()` + `lux.keymap.set()`.
pub fn apply_keybindings(keymap: &KeymapRegistry, cx: &mut App) {
    let bindings = keymap.take_bindings();

    for pending in bindings {
        apply_binding(pending, cx);
    }
}

/// Apply a single binding to GPUI.
fn apply_binding(pending: PendingBinding, cx: &mut App) {
    let context_predicate =
        build_context_predicate(pending.context.as_deref(), pending.view.as_deref());
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
                            "Registered action binding: {} -> {} (context: {:?}, view: {:?})",
                            pending.key,
                            name,
                            pending.context,
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
                        "Registered Lua handler binding: {} -> {} (context: {:?}, view: {:?})",
                        pending.key,
                        id,
                        pending.context,
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
