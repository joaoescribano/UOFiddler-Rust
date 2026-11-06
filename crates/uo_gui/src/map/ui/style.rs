//! The panel's design system: colors, measurements and typography in one
//! place, and the egui theme built from them. Dark, with the gold of the
//! game's valuables as the accent. The components (`widgets`) only use these
//! tokens; no screen picks a color or a spacing on its own.

use eframe::egui::{self, Color32, FontFamily, FontId, Margin, Rounding, Stroke};

/// The colors.
pub mod color {
    use super::Color32;

    /// The window background.
    pub const BG: Color32 = Color32::from_rgb(0x17, 0x18, 0x1c);
    /// Columns and cards.
    pub const SURFACE: Color32 = Color32::from_rgb(0x1f, 0x21, 0x26);
    /// Section headers, fields, rows under the mouse.
    pub const SURFACE_2: Color32 = Color32::from_rgb(0x29, 0x2c, 0x33);
    /// Separator lines.
    pub const BORDER: Color32 = Color32::from_rgb(0x38, 0x3c, 0x45);
    pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe6, 0xe1);
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9a, 0x9c, 0xa3);
    /// The gold: what is selected, the main action.
    pub const ACCENT: Color32 = Color32::from_rgb(0xd8, 0xa8, 0x4e);
    /// The dimmed gold background of the selected row.
    pub const ACCENT_BG: Color32 = Color32::from_rgb(0x4a, 0x3c, 0x22);
    pub const DANGER: Color32 = Color32::from_rgb(0xd9, 0x5c, 0x4f);
    pub const SUCCESS: Color32 = Color32::from_rgb(0x6c, 0xb8, 0x6a);
}

/// Measurements.
pub const RADIUS: f32 = 6.0;
pub const GAP: f32 = 8.0;
pub const SIDEBAR_WIDTH: f32 = 190.0;
pub const LIST_WIDTH: f32 = 250.0;
/// The list column when it shows the slot grid.
pub const LIST_WIDTH_GRID: f32 = 330.0;

/// The text sizes.
pub fn title() -> FontId {
    FontId::new(20.0, FontFamily::Proportional)
}

pub fn heading() -> FontId {
    FontId::new(16.0, FontFamily::Proportional)
}

pub fn body() -> FontId {
    FontId::new(14.0, FontFamily::Proportional)
}

pub fn small() -> FontId {
    FontId::new(12.0, FontFamily::Proportional)
}

pub fn icon() -> FontId {
    FontId::new(18.0, FontFamily::Proportional)
}

/// Once per context: the icons in the fonts and the theme. This restyles the
/// whole context; to theme only one part of the app, call
/// [`super::install_fonts`] once and [`apply`] on that part's `Ui` style.
pub fn install(ctx: &egui::Context) {
    super::install_fonts(ctx);
    ctx.style_mut(apply);
}

/// The panel theme, written into a style: text sizes, spacing and the
/// visuals built from the color tokens.
pub fn apply(style: &mut egui::Style) {
    style.text_styles = [
        (egui::TextStyle::Heading, heading()),
        (egui::TextStyle::Body, body()),
        (egui::TextStyle::Button, body()),
        (egui::TextStyle::Small, small()),
        (
            egui::TextStyle::Monospace,
            FontId::new(13.0, FontFamily::Monospace),
        ),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(GAP, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size.y = 26.0;
    style.spacing.window_margin = Margin::same(0.0);
    style.animation_time = 0.12;
    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.window_fill = color::BG;
    v.panel_fill = color::BG;
    v.extreme_bg_color = Color32::from_rgb(0x13, 0x14, 0x17);
    v.faint_bg_color = color::SURFACE;
    v.window_stroke = Stroke::new(1.0_f32, color::BORDER);
    v.window_rounding = Rounding::same(RADIUS + 2.0);
    v.menu_rounding = Rounding::same(RADIUS);
    v.selection.bg_fill = color::ACCENT_BG;
    v.selection.stroke = Stroke::new(1.0_f32, color::ACCENT);
    v.hyperlink_color = color::ACCENT;
    v.override_text_color = Some(color::TEXT);
    let radius = Rounding::same(RADIUS);
    for (w, fill, stroke) in [
        (&mut v.widgets.noninteractive, color::SURFACE, color::BORDER),
        (&mut v.widgets.inactive, color::SURFACE_2, color::BORDER),
        (&mut v.widgets.hovered, color::SURFACE_2, color::ACCENT),
        (&mut v.widgets.active, color::ACCENT_BG, color::ACCENT),
        (&mut v.widgets.open, color::SURFACE_2, color::ACCENT),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::new(1.0_f32, stroke);
        w.rounding = radius;
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, color::TEXT_DIM);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, color::TEXT);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, color::TEXT);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, color::ACCENT);
}
