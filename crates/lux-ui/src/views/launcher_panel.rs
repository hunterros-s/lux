//! Launcher panel view - the main UI composition.
//!
//! This view coordinates the search input, results list, and action menu.
//! It subscribes to backend state changes for reactive updates.
//!
//! ## Architecture
//!
//! - Backend owns view configuration (placeholder, title, selection_mode)
//! - UI owns ephemeral display state (cursor, scroll, cached results)
//! - State changes flow reactively via subscription

use std::cmp::Ordering;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    div, img, prelude::*, px, size, App, AsyncApp, Context, ElementId, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, KeyContext, ParentElement, Pixels,
    Render, SharedString, Size, Styled, WeakEntity, Window,
};
use gpui_component::{v_virtual_list, VirtualListScrollHandle};
use lux_core::{ActionResult, BackendError, Group, Item, ItemId, SelectionMode};

use crate::actions::{
    CursorDown, CursorUp, Dismiss, OpenActionMenu, RunLuaHandler, ToggleSelection,
};
use crate::backend::{Backend, BackendState};
use crate::model::{ActionMenuItem, ActionMenuState, ExecutionFeedback, ListEntry};
use crate::theme::ThemeExt;
use crate::views::{scroll_to_cursor, SearchInput, SearchInputEvent};

// =============================================================================
// Events
// =============================================================================

/// Events emitted by LauncherPanel.
#[derive(Debug, Clone)]
pub enum LauncherPanelEvent {
    /// Request to dismiss the launcher.
    Dismiss,
}

// =============================================================================
// View Display State
// =============================================================================

/// Ephemeral display state per view depth.
///
/// This is UI-owned state that resets on view push/pop.
/// Backend owns the view configuration (placeholder, title, selection_mode).
#[derive(Debug)]
struct ViewDisplayState {
    /// View identifier for keybinding context.
    view_id: Option<String>,
    /// Cursor position as index into items.
    cursor_index: usize,
    /// Selection mode from backend.
    selection_mode: SelectionMode,
    /// Selected item IDs.
    selected_ids: HashSet<ItemId>,
    /// Current query text.
    query: String,
    /// Cached search results.
    cached_groups: Vec<Group>,
    /// Flattened entries for rendering.
    flat_entries: Vec<ListEntry>,
    /// Item IDs in display order.
    item_ids: Vec<ItemId>,
    /// Generation counter for async cancellation.
    generation: u64,
    /// Whether a search is in progress.
    loading: bool,
}

impl Default for ViewDisplayState {
    fn default() -> Self {
        Self {
            view_id: None,
            cursor_index: 0,
            selection_mode: SelectionMode::Single,
            selected_ids: HashSet::new(),
            query: String::new(),
            cached_groups: Vec::new(),
            flat_entries: Vec::new(),
            item_ids: Vec::new(),
            generation: 0,
            loading: false,
        }
    }
}

impl ViewDisplayState {
    /// Update groups and rebuild indices.
    fn set_groups(&mut self, groups: Vec<Group>) {
        self.cached_groups = groups;
        self.rebuild_indices();
        self.clamp_cursor();
    }

    fn rebuild_indices(&mut self) {
        self.flat_entries.clear();
        self.item_ids.clear();
        let mut flat_index = 0;

        for group in &self.cached_groups {
            if let Some(title) = &group.title {
                self.flat_entries.push(ListEntry::GroupHeader {
                    title: title.clone(),
                });
            }
            for item in &group.items {
                self.flat_entries.push(ListEntry::Item {
                    item: item.clone(),
                    flat_index,
                });
                self.item_ids.push(item.item_id());
                flat_index += 1;
            }
        }
    }

    fn clamp_cursor(&mut self) {
        if self.cursor_index >= self.item_ids.len() {
            self.cursor_index = self.item_ids.len().saturating_sub(1);
        }
    }

    fn cursor_up(&mut self) {
        if self.cursor_index > 0 {
            self.cursor_index -= 1;
        }
    }

    fn cursor_down(&mut self) {
        if self.cursor_index + 1 < self.item_ids.len() {
            self.cursor_index += 1;
        }
    }

    fn cursor_item(&self) -> Option<&Item> {
        self.item_ids.get(self.cursor_index).and_then(|id| {
            for group in &self.cached_groups {
                for item in &group.items {
                    if item.item_id() == *id {
                        return Some(item);
                    }
                }
            }
            None
        })
    }

    fn cursor_to_list_index(&self) -> usize {
        for (i, entry) in self.flat_entries.iter().enumerate() {
            if let ListEntry::Item { flat_index, .. } = entry {
                if *flat_index == self.cursor_index {
                    return i;
                }
            }
        }
        0
    }

    /// Toggle selection at cursor based on selection mode.
    ///
    /// - Single: no-op (selection follows cursor automatically)
    /// - Multi/Custom: toggles selection at cursor
    fn toggle_selection_at_cursor(&mut self) {
        // In Single mode, selection follows cursor - toggle is a no-op
        if matches!(self.selection_mode, SelectionMode::Single) {
            return;
        }

        // Multi/Custom mode: explicit toggle
        if let Some(id) = self.item_ids.get(self.cursor_index).cloned() {
            if self.selected_ids.contains(&id) {
                self.selected_ids.remove(&id);
            } else {
                self.selected_ids.insert(id);
            }
        }
    }

    fn selected_items(&self) -> Vec<Item> {
        let mut items = Vec::new();
        for group in &self.cached_groups {
            for item in &group.items {
                if self.selected_ids.contains(&item.item_id()) {
                    items.push(item.clone());
                }
            }
        }
        items
    }
}

// =============================================================================
// Launcher Panel
// =============================================================================

/// The main launcher UI composition.
pub struct LauncherPanel {
    /// Backend for search/actions.
    backend: Arc<dyn Backend>,
    /// Display state per view depth.
    view_states: Vec<ViewDisplayState>,
    /// Action menu state when open.
    action_menu: Option<ActionMenuState>,
    /// Execution feedback.
    execution_feedback: Option<ExecutionFeedback>,
    /// Search input view.
    search_input: Entity<SearchInput>,
    /// Focus handle.
    focus_handle: FocusHandle,
    /// Scroll handle for results list.
    scroll_handle: VirtualListScrollHandle,
}

impl LauncherPanel {
    /// Create a new launcher panel.
    pub fn new(backend: Arc<dyn Backend>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        // Create search input
        let search_input = cx.new(|cx| SearchInput::new("Search...", window, cx));

        // Subscribe to search input events
        cx.subscribe(&search_input, Self::on_search_input_event)
            .detach();

        let scroll_handle = VirtualListScrollHandle::new();

        // Subscribe to backend state changes
        let state_rx = backend.subscribe();
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut rx = state_rx;
            while rx.changed().await.is_ok() {
                let state = rx.borrow().clone();
                let _ = this.update(cx, |this, cx| {
                    this.on_backend_state_changed(state, cx);
                });
            }
        })
        .detach();

        // Initialize with one view state - subscription will sync
        let view_states = vec![ViewDisplayState::default()];

        // Hide when window loses focus (user clicks outside)
        cx.observe_window_activation(window, |_this, window, cx| {
            if !window.is_window_active() {
                cx.emit(LauncherPanelEvent::Dismiss);
            }
        })
        .detach();

        let mut this = Self {
            backend,
            view_states,
            action_menu: None,
            execution_feedback: None,
            search_input,
            focus_handle,
            scroll_handle,
        };

        // Trigger initial search
        this.trigger_search(String::new(), cx);

        this
    }

    /// Show the launcher and focus it.
    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Reset to fresh state
        self.reset_state(cx);

        // Focus search input
        self.search_input.update(cx, |input, cx| {
            let handle = input.focus_handle(cx);
            window.focus(&handle, cx);
        });
        cx.notify();
    }

    /// Reset launcher to fresh state (clear input, trigger fresh search).
    fn reset_state(&mut self, cx: &mut Context<Self>) {
        // Clear search input
        self.search_input.update(cx, |input, cx| {
            input.clear(cx);
        });

        // Trigger fresh search with empty query to show default results
        self.trigger_search(String::new(), cx);
    }

    /// Hide the launcher.
    pub fn hide(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(LauncherPanelEvent::Dismiss);
        cx.notify();
    }

    // -------------------------------------------------------------------------
    // Backend State Changes
    // -------------------------------------------------------------------------

    fn on_backend_state_changed(&mut self, state: BackendState, cx: &mut Context<Self>) {
        let new_depth = state.len();
        let current_depth = self.view_states.len();

        tracing::info!(
            "on_backend_state_changed: backend_depth={}, ui_depth={}",
            new_depth,
            current_depth
        );

        match new_depth.cmp(&current_depth) {
            Ordering::Greater => {
                // View pushed - create new display state
                tracing::info!(
                    "View pushed, adding {} display states",
                    new_depth - current_depth
                );
                for _ in current_depth..new_depth {
                    self.view_states.push(ViewDisplayState::default());
                }
                // Trigger search for new view
                self.trigger_search(String::new(), cx);
            }
            Ordering::Less => {
                // View popped - restore previous display state
                while self.view_states.len() > new_depth && self.view_states.len() > 1 {
                    self.view_states.pop();
                }
                // Scroll to preserved cursor
                if let Some(display) = self.view_states.last() {
                    scroll_to_cursor(&self.scroll_handle, display.cursor_to_list_index());
                }
            }
            Ordering::Equal => {}
        }

        // Sync view config from backend (selection_mode, placeholder, view_id)
        if let Some(view) = state.last() {
            if let Some(display) = self.view_states.last_mut() {
                display.selection_mode = view.selection;
                display.view_id = view.id.clone();
            }
            if let Some(placeholder) = &view.placeholder {
                self.search_input.update(cx, |input, cx| {
                    input.set_placeholder(placeholder.clone(), cx);
                });
            }
        }

        cx.notify();
    }

    // -------------------------------------------------------------------------
    // Action Handlers
    // -------------------------------------------------------------------------

    fn on_cursor_up(&mut self, _: &CursorUp, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(display) = self.view_states.last_mut() {
            display.cursor_up();
            scroll_to_cursor(&self.scroll_handle, display.cursor_to_list_index());
            cx.notify();
        }
    }

    fn on_cursor_down(&mut self, _: &CursorDown, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(display) = self.view_states.last_mut() {
            display.cursor_down();
            scroll_to_cursor(&self.scroll_handle, display.cursor_to_list_index());
            cx.notify();
        }
    }

    fn on_open_action_menu(
        &mut self,
        _: &OpenActionMenu,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.action_menu.is_some() {
            return;
        }

        let Some(display) = self.view_states.last() else {
            return;
        };

        let items: Vec<_> = if display.selected_ids.is_empty() {
            display.cursor_item().cloned().into_iter().collect()
        } else {
            display.selected_items()
        };

        if !items.is_empty() {
            self.fetch_actions(items, cx);
        }
    }

    fn on_toggle_selection(
        &mut self,
        _: &ToggleSelection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(display) = self.view_states.last_mut() {
            display.toggle_selection_at_cursor();
            cx.notify();
        }
    }

    fn on_run_lua_handler(
        &mut self,
        action: &RunLuaHandler,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Get items to pass to the handler
        let Some(display) = self.view_states.last() else {
            return;
        };

        let items: Vec<_> = if display.selected_ids.is_empty() {
            display.cursor_item().cloned().into_iter().collect()
        } else {
            display.selected_items()
        };

        // Call the Lua handler via backend
        let handler_id = action.id.clone();
        let backend = self.backend.clone();
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = backend.run_key_handler(&handler_id, items).await;
            let _ = this.update(cx, |this, cx| {
                this.apply_action_result(result, cx);
            });
        })
        .detach();
    }

    fn on_dismiss(&mut self, _: &Dismiss, _window: &mut Window, cx: &mut Context<Self>) {
        tracing::info!(
            "on_dismiss: view_states.len()={}, action_menu={}, input='{}'",
            self.view_states.len(),
            self.action_menu.is_some(),
            self.search_input.read(cx).text(cx)
        );

        // 1. Close action menu if open
        if self.action_menu.take().is_some() {
            cx.notify();
            return;
        }

        // 2. Clear input text if non-empty
        let input_text = self.search_input.read(cx).text(cx).to_string();
        if !input_text.is_empty() {
            self.search_input.update(cx, |input, cx| input.clear(cx));
            return;
        }

        // 3. Pop view stack if not at root
        if self.view_states.len() > 1 {
            tracing::info!("on_dismiss: popping view stack");
            self.pop_view(cx);
            return;
        }

        // 4. Dismiss (hide) at root
        tracing::info!("on_dismiss: dismissing at root");
        cx.emit(LauncherPanelEvent::Dismiss);
    }

    // -------------------------------------------------------------------------
    // Search Input Events
    // -------------------------------------------------------------------------

    fn on_search_input_event(
        &mut self,
        _search_input: Entity<SearchInput>,
        event: &SearchInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SearchInputEvent::Changed(query) => {
                self.trigger_search(query.clone(), cx);
            }
            SearchInputEvent::Submit => {
                self.execute_default_action(cx);
            }
            SearchInputEvent::Back => {
                self.pop_view(cx);
            }
        }
    }

    // -------------------------------------------------------------------------
    // Backend Integration
    // -------------------------------------------------------------------------

    fn trigger_search(&mut self, query: String, cx: &mut Context<Self>) {
        let Some(display) = self.view_states.last_mut() else {
            return;
        };

        display.generation += 1;
        let gen = display.generation;
        display.query = query.clone();
        display.loading = true;
        cx.notify();

        let backend = self.backend.clone();
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = backend.search(query).await;
            let _ = this.update(cx, |this, cx| {
                this.apply_search_results(gen, result, cx);
            });
        })
        .detach();
    }

    fn apply_search_results(
        &mut self,
        generation: u64,
        result: Result<Vec<Group>, BackendError>,
        cx: &mut Context<Self>,
    ) {
        let Some(view_display) = self.view_states.last_mut() else {
            return;
        };

        if view_display.generation != generation {
            return;
        }

        view_display.loading = false;

        match result {
            Ok(groups) => {
                let total_items: usize = groups.iter().map(|g| g.items.len()).sum();
                tracing::debug!(
                    "apply_search_results: received {} groups with {} total items",
                    groups.len(),
                    total_items
                );
                view_display.set_groups(groups);
                tracing::debug!(
                    "apply_search_results: after set_groups, {} flat entries",
                    view_display.flat_entries.len()
                );
            }
            Err(e) => {
                tracing::debug!("Search failed: {}", e);
            }
        }

        cx.notify();
    }

    fn fetch_actions(&mut self, items: Vec<Item>, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = backend.get_actions(items).await;
            let _ = this.update(cx, |this, cx| {
                this.apply_actions(result, cx);
            });
        })
        .detach();
    }

    fn apply_actions(
        &mut self,
        result: Result<Vec<lux_plugin_api::ActionInfo>, BackendError>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(action_infos) => {
                if action_infos.is_empty() {
                    return;
                }

                let actions: Vec<ActionMenuItem> = action_infos
                    .into_iter()
                    .map(|info| ActionMenuItem {
                        plugin: info.plugin_name,
                        action_index: info.action_index,
                        title: info.title,
                        icon: info.icon,
                    })
                    .collect();

                self.action_menu = Some(ActionMenuState::new(actions));
            }
            Err(e) => {
                tracing::error!("Failed to get actions: {}", e);
            }
        }

        cx.notify();
    }

    fn execute_default_action(&mut self, cx: &mut Context<Self>) {
        let Some(display) = self.view_states.last() else {
            return;
        };

        let items: Vec<_> = if display.selected_ids.is_empty() {
            display.cursor_item().cloned().into_iter().collect()
        } else {
            display.selected_items()
        };

        if items.is_empty() {
            return;
        }

        let backend = self.backend.clone();
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let actions = backend.get_actions(items.clone()).await;
            if let Ok(action_infos) = actions {
                if let Some(first) = action_infos.first() {
                    let result = backend
                        .execute_action(first.plugin_name.clone(), first.action_index, items)
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.apply_action_result(result, cx);
                    });
                }
            }
        })
        .detach();
    }

    fn apply_action_result(
        &mut self,
        result: Result<ActionResult, BackendError>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(ActionResult::Dismiss) => {
                cx.emit(LauncherPanelEvent::Dismiss);
            }
            Ok(ActionResult::Pop) => {
                // State change will come via subscription
            }
            Ok(ActionResult::PushView { .. }) => {
                // State change will come via subscription
            }
            Ok(ActionResult::ReplaceView { .. }) => {
                // State change will come via subscription
            }
            Ok(ActionResult::Continue) => {
                // Refresh search
                if let Some(display) = self.view_states.last() {
                    let query = display.query.clone();
                    self.trigger_search(query, cx);
                }
            }
            Ok(ActionResult::Complete { message, .. }) => {
                self.execution_feedback = Some(ExecutionFeedback::Complete { message });
                cx.notify();
            }
            Ok(ActionResult::Progress { message }) => {
                self.execution_feedback = Some(ExecutionFeedback::Progress { message });
                cx.notify();
            }
            Ok(ActionResult::Fail { error }) => {
                self.execution_feedback = Some(ExecutionFeedback::Failed { error });
                cx.notify();
            }
            Err(e) => {
                tracing::error!("Action failed: {}", e);
                self.execution_feedback = Some(ExecutionFeedback::Failed {
                    error: e.to_string(),
                });
                cx.notify();
            }
        }
    }

    fn pop_view(&mut self, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        cx.background_executor()
            .spawn(async move {
                let _ = backend.pop_view().await;
                // State change will come via subscription
            })
            .detach();
    }

    // -------------------------------------------------------------------------
    // Click Handlers
    // -------------------------------------------------------------------------

    fn on_item_click(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(display) = self.view_states.last_mut() {
            display.cursor_index = index;
            cx.notify();
        }
    }

    fn on_item_double_click(&mut self, _index: usize, cx: &mut Context<Self>) {
        self.execute_default_action(cx);
    }

    // -------------------------------------------------------------------------
    // Render Helpers
    // -------------------------------------------------------------------------

    /// Render a group header row.
    fn render_group_header(title: &str, theme: &crate::theme::Theme) -> gpui::AnyElement {
        div()
            .w_full()
            .h(theme.group_header_height)
            .px_3()
            .flex()
            .items_end()
            .pb_1()
            .child(
                div()
                    .text_color(theme.text_muted)
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(title.to_uppercase()),
            )
            .into_any_element()
    }

    /// Render a result item row (without click handler - that's added by caller).
    fn render_result_item(
        item: &Item,
        is_cursor: bool,
        is_selected: bool,
        theme: &crate::theme::Theme,
    ) -> gpui::Stateful<gpui::Div> {
        let bg_color = if is_cursor {
            theme.cursor
        } else if is_selected {
            theme.selection
        } else {
            gpui::transparent_black()
        };

        let item_id = item.id.clone();
        let title = item.title.clone();
        let subtitle = item.subtitle.clone();
        let icon = item.icon.clone();

        let mut row = div()
            .id(ElementId::Name(SharedString::from(format!(
                "item-{}",
                item_id
            ))))
            .w_full()
            .h(theme.item_height)
            .px_3()
            .flex()
            .items_center()
            .gap_3()
            .bg(bg_color)
            .rounded(theme.radius)
            .cursor_pointer()
            // Add subtle accent border when cursor is on this item
            .when(is_cursor, |this| {
                this.border_1().border_color(theme.accent.alpha(0.5))
            })
            .hover(|style| style.bg(theme.surface_hover));

        // Icon (always rendered - placeholder if not provided)
        let icon_size = theme.icon_size;
        let icon_el = if let Some(icon_str) = icon {
            if icon_str.starts_with('/') {
                use std::path::PathBuf;
                img(PathBuf::from(icon_str))
                    .size(icon_size)
                    .into_any_element()
            } else {
                div()
                    .w(icon_size)
                    .h(icon_size)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon_str)
                    .into_any_element()
            }
        } else {
            // Placeholder: subtle rounded square
            div()
                .w(icon_size)
                .h(icon_size)
                .rounded(px(4.0))
                .bg(theme.surface_hover)
                .into_any_element()
        };
        row = row.child(icon_el);

        // Title and subtitle on same line
        let mut content = div()
            .flex_1()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .overflow_hidden()
            .child(
                div()
                    .text_color(theme.text)
                    .text_ellipsis()
                    .overflow_hidden()
                    .child(title),
            );

        if let Some(sub) = subtitle {
            content = content.child(
                div()
                    .text_color(theme.text_muted)
                    .text_sm()
                    .text_ellipsis()
                    .flex_shrink_0()
                    .child(sub),
            );
        }

        row.child(content)
    }
}

// =============================================================================
// Focusable
// =============================================================================

impl Focusable for LauncherPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

// =============================================================================
// EventEmitter
// =============================================================================

impl EventEmitter<LauncherPanelEvent> for LauncherPanel {}

// =============================================================================
// Render
// =============================================================================

impl Render for LauncherPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let Some(display) = self.view_states.last() else {
            return div().id("launcher-panel-empty").into_any_element();
        };

        // Build item sizes based on entry type (headers vs items have different heights)
        let item_sizes: Rc<Vec<Size<Pixels>>> = Rc::new(
            display
                .flat_entries
                .iter()
                .map(|entry| match entry {
                    ListEntry::GroupHeader { .. } => size(px(0.0), theme.group_header_height),
                    ListEntry::Item { .. } => size(px(0.0), theme.item_height),
                })
                .collect(),
        );

        // Build results list with VirtualList or empty state
        let results_list = if display.flat_entries.is_empty() {
            div()
                .id("results-list-empty")
                .w_full()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .child(div().text_color(theme.text_muted).child("No results"))
                .into_any_element()
        } else {
            let entity = cx.entity().clone();
            v_virtual_list(
                entity,
                "results-list",
                item_sizes,
                |this, range, _window, cx| {
                    let theme = cx.theme().clone();
                    let Some(display) = this.view_states.last() else {
                        return vec![];
                    };

                    let mut elements = Vec::with_capacity(range.len());
                    for ix in range {
                        let Some(entry) = display.flat_entries.get(ix) else {
                            elements.push(div().into_any_element());
                            continue;
                        };

                        match entry {
                            ListEntry::GroupHeader { title } => {
                                elements.push(Self::render_group_header(title, &theme));
                            }
                            ListEntry::Item { item, flat_index } => {
                                let is_cursor = *flat_index == display.cursor_index;
                                let is_selected = display
                                    .item_ids
                                    .get(*flat_index)
                                    .map(|id| display.selected_ids.contains(id))
                                    .unwrap_or(false);

                                let row =
                                    Self::render_result_item(item, is_cursor, is_selected, &theme);
                                let item_index = *flat_index;
                                let row = row.on_click(cx.listener(
                                    move |this: &mut Self,
                                          event: &gpui::ClickEvent,
                                          _window,
                                          cx| {
                                        if event.click_count() >= 2 {
                                            this.on_item_double_click(item_index, cx);
                                        } else {
                                            this.on_item_click(item_index, cx);
                                        }
                                    },
                                ));
                                elements.push(row.into_any_element());
                            }
                        }
                    }
                    elements
                },
            )
            .track_scroll(&self.scroll_handle)
            .w_full()
            .h_full()
            .into_any_element()
        };

        // Build dynamic key context with view ID
        let mut key_context = KeyContext::default();
        key_context.add("Launcher");
        if let Some(ref view_id) = display.view_id {
            key_context.set("view_id", view_id.clone());
        }

        // Main container
        div()
            .id("launcher-panel")
            .key_context(key_context)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_cursor_up))
            .on_action(cx.listener(Self::on_cursor_down))
            .on_action(cx.listener(Self::on_open_action_menu))
            .on_action(cx.listener(Self::on_toggle_selection))
            .on_action(cx.listener(Self::on_run_lua_handler))
            .on_action(cx.listener(Self::on_dismiss))
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .rounded(theme.radius)
            .overflow_hidden()
            // Search input at top
            .child(
                div()
                    .w_full()
                    .p_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(self.search_input.clone()),
            )
            // Results list with padding
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .overflow_hidden()
                    .p_2()
                    .child(results_list),
            )
            .into_any_element()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_launcher_panel_events() {
        let _event = LauncherPanelEvent::Dismiss;
    }

    #[test]
    fn test_view_display_state_cursor() {
        let mut state = ViewDisplayState::default();
        assert_eq!(state.cursor_index, 0);

        // Empty state - cursor should stay at 0
        state.cursor_down();
        assert_eq!(state.cursor_index, 0);

        // Add some items
        state.set_groups(vec![lux_core::Group::new(
            "Test",
            vec![
                lux_core::Item::new("1", "Item 1"),
                lux_core::Item::new("2", "Item 2"),
            ],
        )]);

        assert_eq!(state.item_ids.len(), 2);
        assert_eq!(state.cursor_index, 0);

        state.cursor_down();
        assert_eq!(state.cursor_index, 1);

        state.cursor_down();
        assert_eq!(state.cursor_index, 1); // Can't go past end

        state.cursor_up();
        assert_eq!(state.cursor_index, 0);
    }
}
