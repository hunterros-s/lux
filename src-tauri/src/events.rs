//! Event system for Lux launcher.
//!
//! Uses an enum-based event bus with `tokio::sync::broadcast` for
//! simple, efficient pub/sub communication.

use serde::Serialize;
use tokio::sync::broadcast;

use crate::plugin_api::types::Groups;

/// All events in the Lux system.
#[derive(Debug, Clone)]
pub enum LuxEvent {
    /// Launcher panel was shown with optional search results
    PanelShown(Option<Groups>),
    /// Launcher panel was hidden
    PanelHidden,
}

/// Simple event bus using tokio broadcast channels.
pub struct EventBus {
    sender: broadcast::Sender<LuxEvent>,
}

impl EventBus {
    /// Create a new event bus.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self { sender }
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<LuxEvent> {
        self.sender.subscribe()
    }

    /// Publish an event to all subscribers.
    pub fn publish(&self, event: LuxEvent) {
        let _ = self.sender.send(event);
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for EventBus {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

/// Event payload for sending to the frontend via Tauri.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TauriEvent {
    PanelShown {
        #[serde(skip_serializing_if = "Option::is_none")]
        results: Option<Groups>,
    },
    PanelHidden,
}

impl TauriEvent {
    /// Convert a LuxEvent to a TauriEvent if it should be sent to frontend.
    pub fn from_lux_event(event: &LuxEvent) -> Option<Self> {
        match event {
            LuxEvent::PanelShown(results) => Some(TauriEvent::PanelShown {
                results: results.clone(),
            }),
            LuxEvent::PanelHidden => Some(TauriEvent::PanelHidden),
        }
    }

    /// The Tauri event name for this event type.
    pub fn event_name(&self) -> &'static str {
        match self {
            TauriEvent::PanelShown { .. } => "lux:panel-shown",
            TauriEvent::PanelHidden => "lux:panel-hidden",
        }
    }
}
