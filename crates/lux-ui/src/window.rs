//! Window management for the Lux launcher.
//!
//! This module provides `LauncherWindow` which owns the window lifecycle,
//! hotkey management, and activation handling.

use std::sync::Arc;

use gpui::{
    px, size, App, AppContext, AsyncApp, Bounds, Entity, Task, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use tokio::sync::mpsc::{self, Receiver};

use lux_plugin_api::{BuiltInHotkey, GlobalHandler, KeymapRegistry};

use crate::backend::Backend;
use crate::keymap::apply_keybindings;
use crate::platform::{
    has_accessibility_permission, parse_hotkey, prompt_accessibility_permission,
    set_activation_policy_accessory, Hotkey, HotkeyCallback, HotkeyManager, MultiHotkeyManager,
};
use crate::theme::Theme;
use crate::views::{LauncherPanel, LauncherPanelEvent};

// =============================================================================
// Window Configuration
// =============================================================================

/// Default window dimensions.
pub const DEFAULT_WIDTH: f32 = 760.0;
pub const DEFAULT_HEIGHT: f32 = 480.0;

/// Create window options for the launcher panel.
///
/// Note: Window bounds will be set after creation since we need App context.
fn create_window_options() -> WindowOptions {
    WindowOptions {
        window_bounds: None, // Will be set via Bounds::centered
        titlebar: None,
        focus: true,
        show: false, // Start hidden, show on hotkey
        kind: WindowKind::PopUp,
        is_movable: false,
        window_background: WindowBackgroundAppearance::Blurred,
        ..Default::default()
    }
}

// =============================================================================
// Hotkey Event Channel
// =============================================================================

/// Events sent from the hotkey callback to the GPUI main thread.
#[derive(Debug, Clone)]
pub enum HotkeyEvent {
    /// Toggle launcher visibility.
    Toggle,
    /// Run a Lua handler by ID.
    RunLuaHandler(String),
}

// =============================================================================
// Launcher Window
// =============================================================================

/// The main launcher window manager.
///
/// This struct owns:
/// - The GPUI window handle
/// - The hotkey manager for global hotkey
/// - A channel receiver for hotkey events
///
/// ## Architecture
///
/// The hotkey callback runs on the main thread but outside GPUI's control.
/// We use a tokio async channel to communicate from the callback to GPUI:
///
/// ```text
/// [Hotkey Callback] ---(channel)---> [GPUI async task] ---(update)---> [Window]
/// ```
///
/// ## Usage
///
/// ```ignore
/// let backend = Arc::new(MockBackend::new());
/// let hotkey = Hotkey::cmd_space();
/// LauncherWindow::run(hotkey, backend);
/// ```
pub struct LauncherWindow {
    /// The GPUI window handle.
    window_handle: WindowHandle<LauncherPanel>,
    /// Legacy single hotkey manager (kept for migration, will be removed).
    _hotkey_manager: Option<HotkeyManager>,
    /// Multi-hotkey manager for Lua-registered hotkeys.
    _multi_hotkey_manager: Option<MultiHotkeyManager>,
    /// Task polling the hotkey channel (kept alive).
    _hotkey_task: Task<()>,
}

impl LauncherWindow {
    /// Create a new launcher window.
    ///
    /// This will:
    /// 1. Check for accessibility permissions (required for global hotkey)
    /// 2. Create the window with the LauncherPanel
    /// 3. Register the global hotkey (legacy) and Lua-configured hotkeys
    /// 4. Set up the hotkey-to-GPUI bridge
    ///
    /// Returns `None` if the window couldn't be created.
    pub fn new(
        hotkey: Hotkey,
        backend: Arc<dyn Backend>,
        keymap: &KeymapRegistry,
        cx: &mut App,
    ) -> Option<Self> {
        // Check accessibility permissions
        if !has_accessibility_permission() {
            tracing::warn!("Accessibility permissions not granted, prompting user");
            prompt_accessibility_permission();
        }

        // Create window options with centered bounds
        let window_size = size(px(DEFAULT_WIDTH), px(DEFAULT_HEIGHT));
        let bounds = Bounds::centered(None, window_size, cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..create_window_options()
        };

        // Create the window and get panel entity for event subscription
        let mut panel_entity: Option<Entity<LauncherPanel>> = None;
        let window_handle = cx
            .open_window(options, |window, cx| {
                // Initialize theme as a global
                let theme = Theme::default();
                cx.set_global(theme);

                // Create the root view - capture window in the closure
                let panel = cx.new(|inner_cx| LauncherPanel::new(backend.clone(), window, inner_cx));
                panel_entity = Some(panel.clone());
                panel
            })
            .ok()?;

        // Subscribe to panel events (dismiss on escape)
        let panel_entity = panel_entity?;
        cx.subscribe(
            &panel_entity,
            |_, event: &LauncherPanelEvent, cx| match event {
                LauncherPanelEvent::Dismiss => {
                    cx.hide();
                }
            },
        )
        .detach();

        // Create hotkey channel (tokio async mpsc)
        let (tx, rx) = mpsc::channel::<HotkeyEvent>(32);

        // Create legacy hotkey manager with channel sender (for the default toggle)
        let tx_toggle = tx.clone();
        let hotkey_manager = HotkeyManager::new(hotkey, move || {
            // Just signal, don't touch GPUI from here
            // Use try_send to avoid blocking if channel is full
            let _ = tx_toggle.try_send(HotkeyEvent::Toggle);
        });

        if hotkey_manager.is_none() {
            tracing::warn!(
                "Failed to register legacy hotkey - accessibility permissions may be required"
            );
        }

        // Create multi-hotkey manager for Lua-configured hotkeys
        let multi_hotkey_manager = MultiHotkeyManager::new();
        if let Some(ref manager) = multi_hotkey_manager {
            apply_global_hotkeys(keymap, manager, tx.clone());
        } else {
            tracing::warn!(
                "Failed to create multi-hotkey manager - accessibility permissions may be required"
            );
        }

        // Spawn task to receive hotkey events
        let handle_clone = window_handle;
        let backend_clone = backend;
        let hotkey_task = cx.spawn(async move |cx: &mut AsyncApp| {
            Self::handle_hotkey_events(rx, handle_clone, backend_clone, cx).await;
        });

        Some(Self {
            window_handle,
            _hotkey_manager: hotkey_manager,
            _multi_hotkey_manager: multi_hotkey_manager,
            _hotkey_task: hotkey_task,
        })
    }

    /// Handle hotkey events from the channel.
    async fn handle_hotkey_events(
        mut rx: Receiver<HotkeyEvent>,
        handle: WindowHandle<LauncherPanel>,
        backend: Arc<dyn Backend>,
        cx: &mut AsyncApp,
    ) {
        while let Some(event) = rx.recv().await {
            match event {
                HotkeyEvent::Toggle => {
                    // Check if window is active
                    let is_active = handle
                        .update(cx, |_panel, window, _cx| window.is_window_active())
                        .unwrap_or(false);

                    if is_active {
                        // Window is focused, hide the app
                        let _ = cx.update(|app| {
                            app.hide();
                        });
                    } else {
                        // Window is not focused, show and activate it
                        let _ = handle.update(cx, |panel, window, cx| {
                            panel.show(window, cx);
                            window.activate_window();
                        });
                    }
                }
                HotkeyEvent::RunLuaHandler(id) => {
                    // Run the Lua handler with empty context (app may be hidden)
                    let backend_clone = backend.clone();
                    let result = backend_clone.run_global_hotkey_handler(&id).await;

                    match result {
                        Ok(action_result) => {
                            // If the handler wants to push a view, show the window first
                            if matches!(
                                action_result,
                                lux_core::ActionResult::PushView { .. }
                                    | lux_core::ActionResult::ReplaceView { .. }
                            ) {
                                let _ = handle.update(cx, |panel, window, cx| {
                                    panel.show(window, cx);
                                    window.activate_window();
                                });
                            }
                            // TODO: Apply the action result to the panel
                            tracing::debug!("Global hotkey handler result: {:?}", action_result);
                        }
                        Err(e) => {
                            tracing::error!("Global hotkey handler failed: {:?}", e);
                        }
                    }
                }
            }
        }
    }

    /// Show and activate the launcher window.
    pub fn show(&self, cx: &mut App) {
        let _ = self.window_handle.update(cx, |_panel, window, _cx| {
            window.activate_window();
        });
    }

    /// Hide the launcher (hides the app).
    pub fn hide(&self, cx: &mut App) {
        cx.hide();
    }

    /// Get the window handle.
    pub fn handle(&self) -> WindowHandle<LauncherPanel> {
        self.window_handle
    }

    /// Check if the window is currently visible/active.
    ///
    /// This queries the actual window state rather than tracking separately.
    pub fn is_visible(&self, cx: &mut App) -> bool {
        self.window_handle.is_active(cx).unwrap_or(false)
    }
}

// =============================================================================
// Global Hotkey Registration
// =============================================================================

/// Apply Lua-configured global hotkeys to the multi-hotkey manager.
fn apply_global_hotkeys(
    keymap: &KeymapRegistry,
    manager: &MultiHotkeyManager,
    tx: tokio::sync::mpsc::Sender<HotkeyEvent>,
) {
    for pending in keymap.take_hotkeys() {
        // Parse the hotkey string
        let Some(hotkey) = parse_hotkey(&pending.key) else {
            tracing::warn!("Invalid hotkey string: '{}', skipping", pending.key);
            continue;
        };

        // Create the callback based on handler type
        let callback: HotkeyCallback = match pending.handler {
            GlobalHandler::BuiltIn(BuiltInHotkey::ToggleLauncher) => {
                let tx = tx.clone();
                Arc::new(move || {
                    let _ = tx.try_send(HotkeyEvent::Toggle);
                })
            }
            GlobalHandler::Function { id } => {
                let tx = tx.clone();
                Arc::new(move || {
                    let _ = tx.try_send(HotkeyEvent::RunLuaHandler(id.clone()));
                })
            }
        };

        // Register the hotkey
        manager.register(hotkey, callback);
        tracing::debug!("Registered global hotkey from Lua: {}", pending.key);
    }
}

// =============================================================================
// App Entry Point
// =============================================================================

/// Initialize and run the launcher application.
///
/// This is the main entry point that sets up everything needed for the launcher:
/// 1. Creates the GPUI application
/// 2. Sets up keybindings (defaults + Lua-configured)
/// 3. Creates the launcher window with hotkey
/// 4. Runs the main loop
///
/// ## Arguments
///
/// - `hotkey`: Global hotkey to toggle the launcher
/// - `backend`: Backend for search/actions
/// - `keymap`: KeymapRegistry with Lua-configured bindings
///
/// ## Example
///
/// ```ignore
/// use lux_ui::window::run_launcher;
/// use lux_ui::backend::RuntimeBackend;
/// use lux_ui::platform::Hotkey;
/// use std::sync::Arc;
///
/// fn main() {
///     let registry = PluginRegistry::new();
///     // ... load Lua config ...
///     let backend = Arc::new(RuntimeBackend::new(engine, runtime, registry.clone()));
///     let hotkey = Hotkey::cmd_space();
///     run_launcher(hotkey, backend, registry.keymap());
/// }
/// ```
pub fn run_launcher(hotkey: Hotkey, backend: Arc<dyn Backend>, keymap: Arc<KeymapRegistry>) {
    gpui::Application::new().run(move |cx| {
        // Hide from dock (run as accessory app like Spotlight)
        set_activation_policy_accessory();

        // Initialize gpui-component
        gpui_component::init(cx);

        // Apply keybindings from registry (defaults + user overrides)
        // Defaults were registered in main.rs, user config may have modified them
        apply_keybindings(&keymap, cx);

        // Create the launcher window (pass keymap for global hotkeys)
        let launcher = LauncherWindow::new(hotkey, backend, &keymap, cx);

        if launcher.is_none() {
            tracing::error!("Failed to create launcher window");
            cx.quit();
            return;
        }

        let launcher = launcher.unwrap();

        // Show the window initially
        launcher.show(cx);

        // Keep the launcher alive by storing it as a global
        cx.set_global(launcher);
    });
}

// =============================================================================
// Global Storage
// =============================================================================

impl gpui::Global for LauncherWindow {}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_options() {
        let options = create_window_options();
        assert!(options.titlebar.is_none());
        assert!(!options.show);
        assert!(matches!(options.kind, WindowKind::PopUp));
        assert!(!options.is_movable);
    }
}
