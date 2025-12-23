//! Theme system for the Lux launcher.
//!
//! Provides a two-layer theming system:
//! - `ThemeSettings`: User-configurable preferences (persisted)
//! - `Theme`: Computed colors derived from settings + system appearance

use gpui::{hsla, px, App, Global, Hsla, Pixels, SharedString};

// =============================================================================
// Theme Settings (User-Configurable)
// =============================================================================

/// User-configurable theme settings.
///
/// These are persisted and can be modified by the user.
/// The actual `Theme` is derived from these settings plus system state.
#[derive(Debug, Clone)]
pub struct ThemeSettings {
    /// Light, dark, or follow system.
    pub appearance: Appearance,
    /// Accent hue (0.0-1.0). Default is blue (210/360).
    pub accent_hue: f32,
    /// Main font family.
    pub font_family: SharedString,
    /// Base font size.
    pub font_size: Pixels,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            appearance: Appearance::System,
            accent_hue: 210.0 / 360.0, // Blue
            font_family: "Inter".into(),
            font_size: px(14.0),
        }
    }
}

impl Global for ThemeSettings {}

/// Appearance mode preference.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Appearance {
    Light,
    Dark,
    #[default]
    System,
}

// =============================================================================
// Theme (Computed)
// =============================================================================

/// The active theme with computed colors.
///
/// Derived from `ThemeSettings` + system appearance.
/// Access via `cx.global::<Theme>()` or `cx.theme()` in render methods.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Whether this is a dark theme.
    pub is_dark: bool,

    // -------------------------------------------------------------------------
    // Background Colors
    // -------------------------------------------------------------------------
    /// Main window background.
    pub background: Hsla,
    /// Elevated surface (cards, panels).
    pub surface: Hsla,
    /// Surface when hovered.
    pub surface_hover: Hsla,

    // -------------------------------------------------------------------------
    // Text Colors
    // -------------------------------------------------------------------------
    /// Primary text color.
    pub text: Hsla,
    /// Secondary/muted text.
    pub text_muted: Hsla,
    /// Placeholder text in inputs.
    pub text_placeholder: Hsla,

    // -------------------------------------------------------------------------
    // Interactive Colors
    // -------------------------------------------------------------------------
    /// Cursor/highlight background.
    pub cursor: Hsla,
    /// Selected item background.
    pub selection: Hsla,
    /// Accent color for focus rings, highlights.
    pub accent: Hsla,

    // -------------------------------------------------------------------------
    // Semantic Colors
    // -------------------------------------------------------------------------
    /// Success state.
    pub success: Hsla,
    /// Warning state.
    pub warning: Hsla,
    /// Error state.
    pub error: Hsla,

    // -------------------------------------------------------------------------
    // Border Colors
    // -------------------------------------------------------------------------
    /// Subtle border.
    pub border: Hsla,
    /// Focused border (derived from accent).
    pub border_focused: Hsla,

    // -------------------------------------------------------------------------
    // Typography
    // -------------------------------------------------------------------------
    /// Main font family.
    pub font_family: SharedString,
    /// Base font size.
    pub font_size: Pixels,
    /// Small font size (subtitles, metadata).
    pub font_size_small: Pixels,
    /// Large font size (titles).
    pub font_size_large: Pixels,

    // -------------------------------------------------------------------------
    // Spacing
    // -------------------------------------------------------------------------
    /// Base spacing unit.
    pub spacing: Pixels,
    /// Border radius for rounded elements.
    pub radius: Pixels,
    /// Icon size in result rows.
    pub icon_size: Pixels,
    /// Height of result item rows.
    pub item_height: Pixels,
    /// Height of group header rows.
    pub group_header_height: Pixels,
}

impl Theme {
    /// Create a theme from settings and system appearance.
    pub fn from_settings(settings: &ThemeSettings, system_is_dark: bool) -> Self {
        let is_dark = match settings.appearance {
            Appearance::Dark => true,
            Appearance::Light => false,
            Appearance::System => system_is_dark,
        };

        let palette = if is_dark {
            Palette::dark(settings.accent_hue)
        } else {
            Palette::light(settings.accent_hue)
        };

        // Convert font_size to f32 for arithmetic
        let base_size: f32 = settings.font_size.into();

        Self {
            is_dark,

            // Backgrounds
            background: palette.bg_base,
            surface: palette.bg_elevated,
            surface_hover: palette.bg_hover,

            // Text
            text: palette.fg_primary,
            text_muted: palette.fg_secondary,
            text_placeholder: palette.fg_tertiary,

            // Interactive - derived from accent
            cursor: palette.bg_hover,
            selection: palette.accent.with_alpha(if is_dark { 0.3 } else { 0.2 }),
            accent: palette.accent,

            // Semantic
            success: palette.success,
            warning: palette.warning,
            error: palette.error,

            // Borders - focused derived from accent
            border: palette.border,
            border_focused: palette.accent,

            // Typography - derived from settings
            font_family: settings.font_family.clone(),
            font_size: settings.font_size,
            font_size_small: px(base_size - 2.0),
            font_size_large: px(base_size + 2.0),

            // Spacing
            spacing: px(8.0),
            radius: px(8.0),
            icon_size: px(24.0),
            item_height: px(40.0),
            group_header_height: px(28.0),
        }
    }

    /// Create default dark theme.
    pub fn dark() -> Self {
        Self::from_settings(&ThemeSettings::default(), true)
    }

    /// Create default light theme.
    pub fn light() -> Self {
        Self::from_settings(&ThemeSettings::default(), false)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Global for Theme {}

// =============================================================================
// Palette (Internal)
// =============================================================================

/// Internal color palette for deriving theme colors.
///
/// Not exposed publicly - just an implementation detail.
struct Palette {
    bg_base: Hsla,
    bg_elevated: Hsla,
    bg_hover: Hsla,
    fg_primary: Hsla,
    fg_secondary: Hsla,
    fg_tertiary: Hsla,
    accent: Hsla,
    border: Hsla,
    success: Hsla,
    warning: Hsla,
    error: Hsla,
}

impl Palette {
    fn dark(accent_hue: f32) -> Self {
        Self {
            // Semi-transparent backgrounds for vibrancy/blur effect
            bg_base: hsla(0.0, 0.0, 0.10, 0.60),
            bg_elevated: hsla(0.0, 0.0, 1.0, 0.08), // subtle white for search box
            bg_hover: hsla(0.0, 0.0, 1.0, 0.12),    // white overlay to brighten
            fg_primary: hsla(0.0, 0.0, 0.95, 0.90),
            fg_secondary: hsla(0.0, 0.0, 0.60, 0.90),
            fg_tertiary: hsla(0.0, 0.0, 0.40, 0.90),
            accent: hsla(accent_hue, 0.80, 0.60, 1.0),
            border: hsla(0.0, 0.0, 1.0, 0.15),
            success: hsla(140.0 / 360.0, 0.70, 0.50, 1.0),
            warning: hsla(40.0 / 360.0, 0.90, 0.50, 1.0),
            error: hsla(0.0, 0.80, 0.50, 1.0),
        }
    }

    fn light(accent_hue: f32) -> Self {
        Self {
            // Semi-transparent backgrounds for vibrancy/blur effect
            bg_base: hsla(0.0, 0.0, 0.98, 0.60),
            bg_elevated: hsla(0.0, 0.0, 0.0, 0.05), // subtle black for search box
            bg_hover: hsla(0.0, 0.0, 0.0, 0.08),    // black overlay to darken
            fg_primary: hsla(0.0, 0.0, 0.10, 1.0),
            fg_secondary: hsla(0.0, 0.0, 0.45, 1.0),
            fg_tertiary: hsla(0.0, 0.0, 0.60, 1.0),
            accent: hsla(accent_hue, 0.80, 0.50, 1.0),
            border: hsla(0.0, 0.0, 0.0, 0.15),
            success: hsla(140.0 / 360.0, 0.70, 0.40, 1.0),
            warning: hsla(40.0 / 360.0, 0.90, 0.45, 1.0),
            error: hsla(0.0, 0.80, 0.45, 1.0),
        }
    }
}

// =============================================================================
// Hsla Extension
// =============================================================================

/// Extension trait for Hsla alpha modification.
trait HslaExt {
    fn with_alpha(self, a: f32) -> Hsla;
}

impl HslaExt for Hsla {
    fn with_alpha(self, a: f32) -> Hsla {
        Hsla { a, ..self }
    }
}

// =============================================================================
// Theme Extensions
// =============================================================================

/// Extension trait for convenient theme access.
pub trait ThemeExt {
    /// Get the current theme.
    fn theme(&self) -> &Theme;
}

impl ThemeExt for App {
    fn theme(&self) -> &Theme {
        self.global::<Theme>()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = ThemeSettings::default();
        assert_eq!(settings.appearance, Appearance::System);
        assert!((settings.accent_hue - 210.0 / 360.0).abs() < 0.001);
    }

    #[test]
    fn test_theme_from_settings_dark() {
        let settings = ThemeSettings::default();
        let theme = Theme::from_settings(&settings, true);
        assert!(theme.is_dark);
    }

    #[test]
    fn test_theme_from_settings_light() {
        let settings = ThemeSettings::default();
        let theme = Theme::from_settings(&settings, false);
        assert!(!theme.is_dark);
    }

    #[test]
    fn test_appearance_override() {
        // Force dark even when system is light
        let settings = ThemeSettings {
            appearance: Appearance::Dark,
            ..Default::default()
        };
        let theme = Theme::from_settings(&settings, false);
        assert!(theme.is_dark);

        // Force light even when system is dark
        let settings = ThemeSettings {
            appearance: Appearance::Light,
            ..Default::default()
        };
        let theme = Theme::from_settings(&settings, true);
        assert!(!theme.is_dark);
    }

    #[test]
    fn test_custom_accent_hue() {
        let settings = ThemeSettings {
            accent_hue: 0.0, // Red
            ..Default::default()
        };

        let theme = Theme::from_settings(&settings, true);
        // Accent should use the custom hue
        assert!((theme.accent.h - 0.0).abs() < 0.001);
        // Border focused should also use accent
        assert!((theme.border_focused.h - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_font_size_derivation() {
        let settings = ThemeSettings {
            font_size: px(16.0),
            ..Default::default()
        };

        let theme = Theme::from_settings(&settings, true);
        let base: f32 = theme.font_size.into();
        let small: f32 = theme.font_size_small.into();
        let large: f32 = theme.font_size_large.into();

        assert!((base - 16.0).abs() < 0.001);
        assert!((small - 14.0).abs() < 0.001);
        assert!((large - 18.0).abs() < 0.001);
    }

    #[test]
    fn test_selection_alpha_differs_by_mode() {
        let settings = ThemeSettings::default();

        let dark = Theme::from_settings(&settings, true);
        let light = Theme::from_settings(&settings, false);

        // Dark mode has higher selection alpha
        assert!(dark.selection.a > light.selection.a);
    }
}
