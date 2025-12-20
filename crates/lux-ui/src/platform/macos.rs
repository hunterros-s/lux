//! macOS platform support.
//!
//! This module provides macOS-specific functionality including global hotkey management.

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSEvent, NSEventMask, NSEventModifierFlags};
use objc2_foundation::MainThreadMarker;
use std::ptr::NonNull;
use std::sync::Arc;

// =============================================================================
// Activation Policy (Dock Visibility)
// =============================================================================

/// Set the app to run as an accessory (like Spotlight).
///
/// This hides the app from the dock and removes the menu bar.
/// Call this early in app initialization (must be on main thread).
///
/// # Safety
/// This must be called from the main thread (e.g., inside GPUI's run callback).
pub fn set_activation_policy_accessory() {
    // SAFETY: This is called from the GPUI run callback, which runs on the main thread.
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
}

// =============================================================================
// Hotkey Configuration
// =============================================================================

/// A hotkey combination (modifier keys + key code).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    /// Modifier flags (Cmd, Ctrl, Alt, Shift).
    pub modifiers: NSEventModifierFlags,
    /// Virtual key code.
    pub keycode: u16,
}

impl Hotkey {
    /// Create a new hotkey.
    pub fn new(modifiers: NSEventModifierFlags, keycode: u16) -> Self {
        Self { modifiers, keycode }
    }

    /// Check if an event matches this hotkey.
    pub fn matches_ptr(&self, event: NonNull<NSEvent>) -> bool {
        // SAFETY: The event pointer is valid during the callback
        let event = unsafe { event.as_ref() };
        let event_modifiers = unsafe { event.modifierFlags() };
        let event_keycode = unsafe { event.keyCode() };

        // Mask to only check the modifier keys we care about
        let modifier_mask = NSEventModifierFlags::NSEventModifierFlagCommand
            | NSEventModifierFlags::NSEventModifierFlagControl
            | NSEventModifierFlags::NSEventModifierFlagOption
            | NSEventModifierFlags::NSEventModifierFlagShift;

        let our_mods = self.modifiers & modifier_mask;
        let event_mods = event_modifiers & modifier_mask;

        our_mods == event_mods && event_keycode == self.keycode
    }
}

impl Default for Hotkey {
    fn default() -> Self {
        // Cmd+Shift+Space (avoids conflict with Spotlight's Cmd+Space)
        Self {
            modifiers: NSEventModifierFlags::NSEventModifierFlagCommand
                | NSEventModifierFlags::NSEventModifierFlagShift,
            keycode: keycodes::SPACE,
        }
    }
}

// =============================================================================
// Accessibility Permissions
// =============================================================================

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const std::ffi::c_void) -> bool;
}

/// Check if the app has accessibility permissions.
///
/// Global hotkey monitoring requires accessibility permissions on macOS.
/// If this returns false, you should prompt the user to enable permissions
/// in System Preferences > Security & Privacy > Privacy > Accessibility.
pub fn has_accessibility_permission() -> bool {
    unsafe { AXIsProcessTrustedWithOptions(std::ptr::null()) }
}

/// Prompt the user to grant accessibility permissions.
///
/// This opens the accessibility preferences pane with the app selected.
/// Returns true if the app is already trusted.
pub fn prompt_accessibility_permission() -> bool {
    use objc2::runtime::ProtocolObject;
    use objc2_foundation::{NSDictionary, NSNumber, NSString};

    // Create options dictionary with kAXTrustedCheckOptionPrompt = true
    let key = NSString::from_str("AXTrustedCheckOptionPrompt");
    let value = NSNumber::new_bool(true);

    // NSString implements NSCopying, so we can convert it to a ProtocolObject
    let key_protocol = ProtocolObject::from_ref(&*key);

    let options: Retained<NSDictionary<NSString, NSNumber>> =
        unsafe { NSDictionary::dictionaryWithObject_forKey(&value, key_protocol) };

    unsafe { AXIsProcessTrustedWithOptions(Retained::as_ptr(&options) as *const _) }
}

// =============================================================================
// Hotkey Manager
// =============================================================================

/// Global hotkey manager using NSEvent monitoring.
///
/// IMPORTANT: The monitors must be kept alive for the callbacks to work.
/// Dropping this struct will unregister the hotkey.
///
/// ## Thread Safety
///
/// The callback is invoked on the main thread. If you need to interact with
/// GPUI state, use a channel to send events to the GPUI context.
///
/// ## Accessibility Permissions
///
/// Global hotkey monitoring requires accessibility permissions. Call
/// `has_accessibility_permission()` before creating the manager, and
/// `prompt_accessibility_permission()` if needed.
pub struct HotkeyManager {
    /// Global event monitor - fires when app is NOT focused.
    _global_monitor: Retained<AnyObject>,
    /// Local event monitor - fires when app IS focused.
    _local_monitor: Retained<AnyObject>,
    /// The blocks must be kept alive alongside the monitors.
    _global_block: RcBlock<dyn Fn(NonNull<NSEvent>)>,
    _local_block: RcBlock<dyn Fn(NonNull<NSEvent>) -> *mut NSEvent>,
    /// Current hotkey configuration.
    hotkey: Hotkey,
}

impl HotkeyManager {
    /// Create a new hotkey manager with the given hotkey and callback.
    ///
    /// The callback will be invoked on the main thread when the hotkey is pressed,
    /// regardless of whether the app is focused.
    ///
    /// Returns `None` if the monitors couldn't be created (e.g., missing
    /// accessibility permissions for the global monitor).
    pub fn new<F>(hotkey: Hotkey, callback: F) -> Option<Self>
    where
        F: Fn() + Send + Sync + 'static,
    {
        let callback = Arc::new(callback);

        // Create global monitor block (fires when app is NOT focused)
        let global_block = {
            let hotkey_clone = hotkey;
            let callback_clone = callback.clone();

            RcBlock::new(move |event: NonNull<NSEvent>| {
                if hotkey_clone.matches_ptr(event) {
                    callback_clone();
                }
            })
        };

        // Create local monitor block (fires when app IS focused)
        let local_block = {
            let hotkey_clone = hotkey;
            let callback_clone = callback.clone();

            RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
                if hotkey_clone.matches_ptr(event) {
                    callback_clone();
                    // Return null to consume the event
                    std::ptr::null_mut()
                } else {
                    // Pass through unmatched events
                    event.as_ptr()
                }
            })
        };

        // Register global monitor
        let global_monitor = unsafe {
            NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
                NSEventMask::KeyDown,
                &global_block,
            )
        }?;

        // Register local monitor
        let local_monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::KeyDown,
                &local_block,
            )
        }?;

        Some(Self {
            _global_monitor: global_monitor,
            _local_monitor: local_monitor,
            _global_block: global_block,
            _local_block: local_block,
            hotkey,
        })
    }

    /// Get the current hotkey configuration.
    pub fn hotkey(&self) -> Hotkey {
        self.hotkey
    }
}

// =============================================================================
// Key Code Constants
// =============================================================================

/// Common macOS virtual key codes.
pub mod keycodes {
    pub const A: u16 = 0;
    pub const S: u16 = 1;
    pub const D: u16 = 2;
    pub const F: u16 = 3;
    pub const H: u16 = 4;
    pub const G: u16 = 5;
    pub const Z: u16 = 6;
    pub const X: u16 = 7;
    pub const C: u16 = 8;
    pub const V: u16 = 9;
    pub const B: u16 = 11;
    pub const Q: u16 = 12;
    pub const W: u16 = 13;
    pub const E: u16 = 14;
    pub const R: u16 = 15;
    pub const Y: u16 = 16;
    pub const T: u16 = 17;
    pub const O: u16 = 31;
    pub const U: u16 = 32;
    pub const I: u16 = 34;
    pub const P: u16 = 35;
    pub const L: u16 = 37;
    pub const J: u16 = 38;
    pub const K: u16 = 40;
    pub const N: u16 = 45;
    pub const M: u16 = 46;
    pub const SPACE: u16 = 49;
    pub const RETURN: u16 = 36;
    pub const TAB: u16 = 48;
    pub const ESCAPE: u16 = 53;
}

// =============================================================================
// Hotkey Parsing
// =============================================================================

/// Parse a hotkey string like "cmd+space" or "ctrl+shift+p".
pub fn parse_hotkey(s: &str) -> Option<Hotkey> {
    let parts: Vec<String> = s.split('+').map(|p| p.trim().to_lowercase()).collect();

    let mut modifiers = NSEventModifierFlags::empty();
    let mut keycode = None;

    for part in &parts {
        match part.as_str() {
            "cmd" | "command" | "\u{2318}" => {
                modifiers |= NSEventModifierFlags::NSEventModifierFlagCommand
            }
            "ctrl" | "control" | "\u{2303}" => {
                modifiers |= NSEventModifierFlags::NSEventModifierFlagControl
            }
            "alt" | "option" | "opt" | "\u{2325}" => {
                modifiers |= NSEventModifierFlags::NSEventModifierFlagOption
            }
            "shift" | "\u{21E7}" => modifiers |= NSEventModifierFlags::NSEventModifierFlagShift,
            key => keycode = key_name_to_code(key),
        }
    }

    keycode.map(|kc| Hotkey::new(modifiers, kc))
}

fn key_name_to_code(name: &str) -> Option<u16> {
    Some(match name {
        "a" => keycodes::A,
        "b" => keycodes::B,
        "c" => keycodes::C,
        "d" => keycodes::D,
        "e" => keycodes::E,
        "f" => keycodes::F,
        "g" => keycodes::G,
        "h" => keycodes::H,
        "i" => keycodes::I,
        "j" => keycodes::J,
        "k" => keycodes::K,
        "l" => keycodes::L,
        "m" => keycodes::M,
        "n" => keycodes::N,
        "o" => keycodes::O,
        "p" => keycodes::P,
        "q" => keycodes::Q,
        "r" => keycodes::R,
        "s" => keycodes::S,
        "t" => keycodes::T,
        "u" => keycodes::U,
        "v" => keycodes::V,
        "w" => keycodes::W,
        "x" => keycodes::X,
        "y" => keycodes::Y,
        "z" => keycodes::Z,
        "space" | " " => keycodes::SPACE,
        "return" | "enter" => keycodes::RETURN,
        "tab" => keycodes::TAB,
        "escape" | "esc" => keycodes::ESCAPE,
        _ => return None,
    })
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hotkey_default() {
        let hotkey = Hotkey::default();
        assert_eq!(hotkey.keycode, keycodes::SPACE);
        assert!(hotkey
            .modifiers
            .contains(NSEventModifierFlags::NSEventModifierFlagCommand));
        assert!(hotkey
            .modifiers
            .contains(NSEventModifierFlags::NSEventModifierFlagShift));
    }

    #[test]
    fn test_parse_hotkey_cmd_space() {
        let hotkey = parse_hotkey("cmd+space").unwrap();
        assert_eq!(hotkey.keycode, keycodes::SPACE);
        assert!(hotkey
            .modifiers
            .contains(NSEventModifierFlags::NSEventModifierFlagCommand));
    }

    #[test]
    fn test_parse_hotkey_ctrl_shift_p() {
        let hotkey = parse_hotkey("ctrl+shift+p").unwrap();
        assert_eq!(hotkey.keycode, keycodes::P);
        assert!(hotkey
            .modifiers
            .contains(NSEventModifierFlags::NSEventModifierFlagControl));
        assert!(hotkey
            .modifiers
            .contains(NSEventModifierFlags::NSEventModifierFlagShift));
    }

    #[test]
    fn test_parse_hotkey_alt_space() {
        let hotkey = parse_hotkey("alt+space").unwrap();
        assert_eq!(hotkey.keycode, keycodes::SPACE);
        assert!(hotkey
            .modifiers
            .contains(NSEventModifierFlags::NSEventModifierFlagOption));
    }

    #[test]
    fn test_parse_hotkey_invalid() {
        assert!(parse_hotkey("invalid").is_none());
        assert!(parse_hotkey("cmd+invalid").is_none());
    }
}
