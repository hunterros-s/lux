//! Search input component with proper IME support.
//!
//! This module provides a text input optimized for search use cases.
//! It implements `EntityInputHandler` for proper IME composition support.

use std::ops::Range;

use gpui::{
    div, fill, point, prelude::*, px, relative, size, App, Bounds, ClipboardItem, Context,
    CursorStyle, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler, EventEmitter,
    FocusHandle, Focusable, GlobalElementId, InteractiveElement, IntoElement, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, ParentElement, Pixels,
    Point, Render, ShapedLine, SharedString, Style, Styled, TextRun, UTF16Selection,
    UnderlineStyle, Window,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::actions::{
    Backspace, Copy, Cut, Delete, End, Home, MoveLeft, MoveRight, Paste, SelectLeft, SelectRight,
    Submit, TextSelectAll,
};
use crate::theme::ThemeExt;

// =============================================================================
// Events
// =============================================================================

/// Events emitted by SearchInput.
#[derive(Debug, Clone)]
pub enum SearchInputEvent {
    /// Text content changed.
    Changed(String),
    /// Enter pressed - execute current selection.
    Submit,
    /// Backspace on empty input - pop view stack.
    Back,
}

// =============================================================================
// SearchInput (Public API)
// =============================================================================

/// Search input component with proper IME support.
///
/// This is the public wrapper that forwards events from the inner `TextEditor`.
/// Use `focus_handle()` to manage focus from the parent.
pub struct SearchInput {
    editor: Entity<TextEditor>,
}

impl SearchInput {
    /// Create a new search input with the given placeholder text.
    pub fn new(
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| TextEditor::new(placeholder.into(), window, cx));

        // Forward events from editor so parent doesn't need to access .editor
        cx.subscribe(&editor, |_this, _editor, event: &SearchInputEvent, cx| {
            cx.emit(event.clone());
        })
        .detach();

        Self { editor }
    }

    /// Get the focus handle for this input.
    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.read(cx).focus_handle.clone()
    }

    /// Get the current text content.
    pub fn text<'a>(&self, cx: &'a App) -> &'a str {
        &self.editor.read(cx).text
    }

    /// Set the text content.
    pub fn set_text(&self, text: impl Into<String>, cx: &mut App) {
        self.editor.update(cx, |editor, cx| {
            editor.text = text.into();
            editor.selected_range = editor.text.len()..editor.text.len();
            editor.marked_range = None;
            cx.notify();
        });
    }

    /// Clear the text content.
    pub fn clear(&self, cx: &mut App) {
        self.set_text("", cx);
    }

    /// Set the placeholder text.
    pub fn set_placeholder(&self, placeholder: impl Into<SharedString>, cx: &mut App) {
        self.editor.update(cx, |editor, cx| {
            editor.placeholder = placeholder.into();
            cx.notify();
        });
    }
}

impl EventEmitter<SearchInputEvent> for SearchInput {}

impl Focusable for SearchInput {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.read(cx).focus_handle.clone()
    }
}

impl Render for SearchInput {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.editor.clone()
    }
}

// =============================================================================
// TextEditor (Internal Implementation)
// =============================================================================

/// Internal text editor implementation with full IME support.
///
/// All byte offsets in `selected_range` and `marked_range` are UTF-8 byte offsets.
/// Grapheme boundaries are computed when handling cursor movement.
struct TextEditor {
    /// Current text content (UTF-8).
    text: String,
    /// Selection range in byte offsets. Start == end means cursor with no selection.
    selected_range: Range<usize>,
    /// Whether selection was made right-to-left (cursor at start).
    selection_reversed: bool,
    /// IME composition range in byte offsets, if active.
    marked_range: Option<Range<usize>>,
    /// Placeholder text shown when empty.
    placeholder: SharedString,
    /// Focus handle for keyboard input.
    focus_handle: FocusHandle,
    /// Cached shaped text from last render (for hit testing).
    last_layout: Option<ShapedLine>,
    /// Cached element bounds from last render (for hit testing).
    last_bounds: Option<Bounds<Pixels>>,
    /// Whether mouse is currently selecting.
    is_selecting: bool,
}

impl TextEditor {
    fn new(placeholder: SharedString, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        // Select all on focus
        cx.on_focus(&focus_handle, window, |this: &mut Self, _window, cx| {
            this.select_all_internal(cx);
        })
        .detach();

        Self {
            text: String::new(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            placeholder,
            focus_handle,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
        }
    }

    // -------------------------------------------------------------------------
    // Cursor/Selection Helpers
    // -------------------------------------------------------------------------

    /// Get cursor position (the "active" end of selection).
    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    /// Move cursor to offset, collapsing selection.
    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify();
    }

    /// Extend selection to offset.
    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }

        // Flip if selection crossed over
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }

        cx.notify();
    }

    /// Select all text.
    fn select_all_internal(&mut self, cx: &mut Context<Self>) {
        self.selected_range = 0..self.text.len();
        self.selection_reversed = false;
        cx.notify();
    }

    // -------------------------------------------------------------------------
    // Grapheme Navigation
    // -------------------------------------------------------------------------

    /// Find previous grapheme boundary before offset.
    fn previous_boundary(&self, offset: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    /// Find next grapheme boundary after offset.
    fn next_boundary(&self, offset: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.text.len())
    }

    // -------------------------------------------------------------------------
    // UTF-16 Conversion (for platform IME APIs)
    // -------------------------------------------------------------------------

    fn offset_to_utf16(&self, utf8_offset: usize) -> usize {
        self.text[..utf8_offset].encode_utf16().count()
    }

    fn offset_from_utf16(&self, utf16_offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.text.chars() {
            if utf16_count >= utf16_offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    // -------------------------------------------------------------------------
    // Hit Testing
    // -------------------------------------------------------------------------

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.text.is_empty() {
            return 0;
        }

        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };

        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.text.len();
        }

        line.closest_index_for_x(position.x - bounds.left())
    }

    // -------------------------------------------------------------------------
    // Action Handlers
    // -------------------------------------------------------------------------

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.text.is_empty() {
            cx.emit(SearchInputEvent::Back);
            return;
        }

        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn left(&mut self, _: &MoveLeft, _window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &MoveRight, _window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &TextSelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_all_internal(cx);
    }

    fn home(&mut self, _: &Home, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.text.len(), cx);
    }

    fn copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.text[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            // Replace newlines with spaces for single-line input
            let text = text.replace('\n', " ");
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.text[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn submit(&mut self, _: &Submit, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(SearchInputEvent::Submit);
    }

    // -------------------------------------------------------------------------
    // Mouse Handlers
    // -------------------------------------------------------------------------

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;

        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }
}

impl EventEmitter<SearchInputEvent> for TextEditor {}

impl Focusable for TextEditor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

// =============================================================================
// EntityInputHandler Implementation (IME Support)
// =============================================================================

impl EntityInputHandler for TextEditor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.text[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.text = format!(
            "{}{}{}",
            &self.text[..range.start],
            new_text,
            &self.text[range.end..]
        );

        let new_cursor = range.start + new_text.len();
        self.selected_range = new_cursor..new_cursor;
        self.marked_range = None;

        cx.emit(SearchInputEvent::Changed(self.text.clone()));
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.text = format!(
            "{}{}{}",
            &self.text[..range.start],
            new_text,
            &self.text[range.end..]
        );

        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }

        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .map(|r| r.start + range.start..r.end + range.start)
            .unwrap_or_else(|| {
                let cursor = range.start + new_text.len();
                cursor..cursor
            });

        cx.emit(SearchInputEvent::Changed(self.text.clone()));
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);

        Some(Bounds::from_corners(
            point(
                element_bounds.left() + layout.x_for_index(range.start),
                element_bounds.top(),
            ),
            point(
                element_bounds.left() + layout.x_for_index(range.end),
                element_bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds.as_ref()?;
        let layout = self.last_layout.as_ref()?;

        let local_point = bounds.localize(&point)?;
        let utf8_index = layout.index_for_x(local_point.x)?;
        Some(self.offset_to_utf16(utf8_index))
    }
}

// =============================================================================
// Render Implementation
// =============================================================================

impl Render for TextEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let is_focused = self.focus_handle.is_focused(window);

        div()
            .id("search-input")
            .key_context("SearchInput")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            // Action handlers
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::submit))
            // Note: Dismiss is handled by LauncherPanel, not here
            // Mouse handlers
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            // Styling
            .w_full()
            .px_3()
            .py_2()
            .bg(theme.surface)
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .when(is_focused, |this| this.border_color(theme.border_focused))
            // Text element
            .child(TextInputElement {
                editor: cx.entity().clone(),
            })
    }
}

// =============================================================================
// Custom Text Element (for handle_input and rendering)
// =============================================================================

/// Custom element that renders text with cursor/selection and registers input handler.
struct TextInputElement {
    editor: Entity<TextEditor>,
}

struct TextInputPrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextInputElement {
    type RequestLayoutState = ();
    type PrepaintState = TextInputPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let editor = self.editor.read(cx);
        let theme = cx.theme();

        let content = &editor.text;
        let is_empty = content.is_empty();
        let selected_range = editor.selected_range.clone();
        let cursor = editor.cursor_offset();
        let is_focused = editor.focus_handle.is_focused(window);
        let style = window.text_style();

        // Display text or placeholder
        let (display_text, text_color) = if is_empty {
            (editor.placeholder.clone(), theme.text_placeholder)
        } else {
            (SharedString::from(content.clone()), theme.text)
        };

        // Build text runs (with underline for marked/IME text)
        // Only apply marked_range styling when showing actual text, not placeholder
        let base_run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let runs = if !is_empty {
            if let Some(marked_range) = editor.marked_range.as_ref() {
                vec![
                    TextRun {
                        len: marked_range.start,
                        ..base_run.clone()
                    },
                    TextRun {
                        len: marked_range.end - marked_range.start,
                        underline: Some(UnderlineStyle {
                            color: Some(text_color),
                            thickness: px(1.0),
                            wavy: false,
                        }),
                        ..base_run.clone()
                    },
                    TextRun {
                        len: display_text.len().saturating_sub(marked_range.end),
                        ..base_run
                    },
                ]
                .into_iter()
                .filter(|run| run.len > 0)
                .collect()
            } else {
                vec![base_run]
            }
        } else {
            vec![base_run]
        };

        // Shape text
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        // Build cursor and selection quads
        let (selection_quad, cursor_quad) = if is_empty {
            // Empty: show cursor at start when focused
            let cursor_quad = if is_focused {
                Some(fill(
                    Bounds::new(
                        point(bounds.left(), bounds.top()),
                        size(px(2.), bounds.size.height),
                    ),
                    theme.accent,
                ))
            } else {
                None
            };
            (None, cursor_quad)
        } else if selected_range.is_empty() {
            // Cursor only (no selection)
            let cursor_pos = line.x_for_index(cursor);
            let cursor_quad = if is_focused {
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_pos, bounds.top()),
                        size(px(2.), bounds.size.height),
                    ),
                    theme.accent,
                ))
            } else {
                None
            };
            (None, cursor_quad)
        } else {
            // Selection highlight
            let selection_quad = Some(fill(
                Bounds::from_corners(
                    point(
                        bounds.left() + line.x_for_index(selected_range.start),
                        bounds.top(),
                    ),
                    point(
                        bounds.left() + line.x_for_index(selected_range.end),
                        bounds.bottom(),
                    ),
                ),
                theme.selection,
            ));
            (selection_quad, None)
        };

        TextInputPrepaintState {
            line: Some(line),
            cursor: cursor_quad,
            selection: selection_quad,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Register input handler for IME support
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );

        // Paint selection background
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }

        // Paint text
        if let Some(line) = prepaint.line.take() {
            let _ = line.paint(bounds.origin, window.line_height(), window, cx);

            // Cache layout for hit testing
            self.editor.update(cx, |editor, _cx| {
                editor.last_layout = Some(line);
                editor.last_bounds = Some(bounds);
            });
        }

        // Paint cursor
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
    }
}
