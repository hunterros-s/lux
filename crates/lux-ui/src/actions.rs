//! Centralized actions for the Lux launcher.
//!
//! This module defines all GPUI actions used throughout the launcher.
//! Actions are dispatched by GPUI's key binding system.

use gpui::actions;

// =============================================================================
// Navigation Actions
// =============================================================================

actions!(
    lux,
    [
        CursorUp,
        CursorDown,
        CursorHome,
        CursorEnd,
        PageUp,
        PageDown,
    ]
);

// =============================================================================
// Selection Actions
// =============================================================================

actions!(lux, [ToggleSelection, SelectAll, ClearSelection,]);

// =============================================================================
// Execution Actions
// =============================================================================

actions!(lux, [Submit, OpenActionMenu, Dismiss, Pop,]);

// =============================================================================
// Text Editing Actions
// =============================================================================

actions!(
    lux,
    [
        Backspace,
        Delete,
        MoveLeft,
        MoveRight,
        SelectLeft,
        SelectRight,
        TextSelectAll,
        Home,
        End,
        Copy,
        Paste,
        Cut,
    ]
);

// =============================================================================
// Lua Handler Action
// =============================================================================

/// Action for Lua function bindings.
///
/// GPUI dispatches this action, and the handler looks up the Lua function by ID.
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(no_json, namespace = lux)]
pub struct RunLuaHandler {
    /// ID that maps to a LuaFunctionRef in KeymapRegistry.
    pub id: String,
}

// =============================================================================
// Action Lookup
// =============================================================================

/// Look up an action by name for GPUI registration.
///
/// Returns a boxed action that can be used with `cx.bind_keys()`.
pub fn action_from_name(name: &str) -> Option<Box<dyn gpui::Action>> {
    match name {
        // Navigation
        "cursor_up" => Some(Box::new(CursorUp)),
        "cursor_down" => Some(Box::new(CursorDown)),
        "cursor_home" => Some(Box::new(CursorHome)),
        "cursor_end" => Some(Box::new(CursorEnd)),
        "page_up" => Some(Box::new(PageUp)),
        "page_down" => Some(Box::new(PageDown)),

        // Selection
        "toggle_selection" => Some(Box::new(ToggleSelection)),
        "select_all" => Some(Box::new(SelectAll)),
        "clear_selection" => Some(Box::new(ClearSelection)),

        // Execution
        "submit" => Some(Box::new(Submit)),
        "open_action_menu" => Some(Box::new(OpenActionMenu)),
        "dismiss" => Some(Box::new(Dismiss)),
        "pop" => Some(Box::new(Pop)),

        // Text editing
        "backspace" => Some(Box::new(Backspace)),
        "delete" => Some(Box::new(Delete)),
        "move_left" => Some(Box::new(MoveLeft)),
        "move_right" => Some(Box::new(MoveRight)),
        "select_left" => Some(Box::new(SelectLeft)),
        "select_right" => Some(Box::new(SelectRight)),
        "text_select_all" => Some(Box::new(TextSelectAll)),
        "home" => Some(Box::new(Home)),
        "end" => Some(Box::new(End)),
        "copy" => Some(Box::new(Copy)),
        "paste" => Some(Box::new(Paste)),
        "cut" => Some(Box::new(Cut)),

        _ => None,
    }
}

/// Get all available action names.
pub fn available_actions() -> &'static [&'static str] {
    &[
        // Navigation
        "cursor_up",
        "cursor_down",
        "cursor_home",
        "cursor_end",
        "page_up",
        "page_down",
        // Selection
        "toggle_selection",
        "select_all",
        "clear_selection",
        // Execution
        "submit",
        "open_action_menu",
        "dismiss",
        "pop",
        // Text editing
        "backspace",
        "delete",
        "move_left",
        "move_right",
        "select_left",
        "select_right",
        "text_select_all",
        "home",
        "end",
        "copy",
        "paste",
        "cut",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_from_name() {
        assert!(action_from_name("cursor_up").is_some());
        assert!(action_from_name("submit").is_some());
        assert!(action_from_name("unknown_action").is_none());
    }

    #[test]
    fn test_available_actions() {
        let actions = available_actions();
        assert!(actions.contains(&"cursor_up"));
        assert!(actions.contains(&"submit"));
        assert!(actions.contains(&"dismiss"));
    }
}
