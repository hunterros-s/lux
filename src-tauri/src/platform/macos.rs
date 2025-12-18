//! macOS-specific functionality for Lux launcher.
//!
//! Handles NSPanel setup, vibrancy effects, and global shortcuts.

use std::sync::Arc;

use tauri::{App, Emitter, Listener, Manager};
use tauri_nspanel::objc2_app_kit::{NSWindowCollectionBehavior, NSWindowStyleMask};
use tauri_nspanel::{tauri_panel, ManagerExt, WebviewWindowExt};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};

use crate::events::{EventBus, LuxEvent};
use crate::lua_runtime::LuaRuntime;
use crate::plugin_api::QueryEngine;

// =============================================================================
// Panel Configuration
// =============================================================================

// Custom panel type for spotlight-like behavior with event handling.
tauri_panel! {
    panel!(SpotlightPanel {
        config: {
            can_become_key_window: true,
            is_floating_panel: true
        }
    })

    panel_event!(SpotlightPanelEventHandler {
        window_did_resign_key(notification: &NSNotification) -> (),
    })
}

/// Set up the main window as a spotlight-like panel.
pub fn setup_panel(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    // These unwraps are intentional - if the main window doesn't exist at startup,
    // the app cannot function and should fail fast.
    let window = app
        .get_webview_window("main")
        .expect("Main window must exist during setup");

    // Apply vibrancy effect
    apply_vibrancy(
        &window,
        NSVisualEffectMaterial::HudWindow,
        Some(NSVisualEffectState::Active),
        Some(12.0),
    )
    .expect("Failed to apply vibrancy - macOS vibrancy effect unavailable");

    // Convert to panel for fullscreen support
    let panel = window
        .to_panel::<SpotlightPanel>()
        .expect("Failed to convert window to panel - NSPanel conversion failed");

    // Set style mask for non-activating panel (doesn't steal focus)
    panel.set_style_mask(NSWindowStyleMask::NonactivatingPanel);

    // Set collection behavior for fullscreen support
    // Transient: panel doesn't persist across space changes (avoids flash when switching spaces)
    panel.set_collection_behavior(
        NSWindowCollectionBehavior::Transient
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );

    // Set floating panel behavior
    panel.set_floating_panel(true);

    // Set window level high enough to appear over fullscreen
    panel.set_level(25); // NSPopUpMenuWindowLevel

    // Set up event handler to hide panel when it loses focus
    let event_handler = SpotlightPanelEventHandler::new();
    let app_handle = app.handle().clone();

    event_handler.window_did_resign_key(move |_notification| {
        if let Ok(panel) = app_handle.get_webview_panel("main") {
            let _ = app_handle.emit("spotlight-hide", ());
            panel.hide();
        }
    });

    panel.set_event_handler(Some(event_handler.as_ref()));

    Ok(())
}

/// Register the global shortcut with its handler.
/// Called when frontend is ready.
pub fn register_shortcut_with_handler(
    app: &tauri::AppHandle,
    event_bus: Arc<EventBus>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tauri_plugin_global_shortcut::ShortcutState;

    let shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space);

    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }

            let Ok(panel) = app.get_webview_panel("main") else {
                return;
            };

            if panel.is_visible() {
                tracing::debug!("Hiding panel");
                event_bus.publish(LuxEvent::PanelHidden);
                let _ = app.emit("spotlight-hide", ());
                panel.hide();
            } else {
                tracing::debug!("Showing panel");
                panel.show_and_make_key();
                let _ = app.emit("spotlight-show", ());

                tauri::async_runtime::spawn(emit_panel_shown(
                    event_bus.clone(),
                    app.state::<Arc<QueryEngine>>().inner().clone(),
                    app.state::<Option<Arc<LuaRuntime>>>().inner().clone(),
                ));
            }
        })?;

    tracing::info!("Global shortcut registered");
    Ok(())
}

/// Set up the hide request listener.
pub fn setup_hide_listener(app: &App) {
    let app_handle = app.handle().clone();
    app.listen("request-hide", move |_| {
        let handle = app_handle.clone();
        let handle_inner = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            if let Ok(panel) = handle_inner.get_webview_panel("main") {
                panel.hide();
            }
        });
    });
}

/// Execute empty search and emit panel-shown with results.
async fn emit_panel_shown(
    event_bus: Arc<EventBus>,
    engine: Arc<QueryEngine>,
    lua_rt: Option<Arc<LuaRuntime>>,
) {
    tracing::debug!("Searching for panel-shown");
    let results = match lua_rt {
        Some(rt) => rt.with_lua(move |lua| engine.search(lua, "")).await.ok(),
        None => None,
    };
    tracing::info!(
        "panel-shown: {} groups",
        results.as_ref().map(|r| r.len()).unwrap_or(0)
    );
    event_bus.publish(LuxEvent::PanelShown(results));
}

// =============================================================================
// Icon Extraction
// =============================================================================

// Icon extraction is in sources/applications.rs
