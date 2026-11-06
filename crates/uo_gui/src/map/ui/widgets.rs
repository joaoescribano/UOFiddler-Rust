//! The panel components, built on the `style` tokens: the sidebar, the list
//! column (search, filters, rows), the detail (header, collapsible sections,
//! fields), the actions bar and the toasts. Screens only combine these; the
//! look lives here.

use super::style::{self, color};
use eframe::egui::{
    self, Align, Color32, Frame, Layout, Margin, Response, RichText, Rounding, Sense, Stroke, Ui,
};
use egui_phosphor::regular as icon;

/// A frame with a card's background and border.
pub fn card_frame() -> Frame {
    Frame::none()
        .fill(color::SURFACE)
        .stroke(Stroke::new(1.0_f32, color::BORDER))
        .rounding(Rounding::same(style::RADIUS))
        .inner_margin(Margin::same(10.0))
}

/// A card: the content on a bordered surface.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_frame().show(ui, add).inner
}

/// A sidebar row: icon and name; the selected one in gold, and the
/// background lights up under the mouse.
pub fn nav_item(ui: &mut Ui, glyph: &str, label: &str, selected: bool) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), Sense::click());
    let hover = ui
        .ctx()
        .animate_bool(response.id.with("hover"), response.hovered() || selected);
    let fill = if selected {
        color::ACCENT_BG
    } else {
        color::SURFACE_2.gamma_multiply(hover)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, Rounding::same(style::RADIUS), fill);
    if selected {
        let bar = egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height()));
        painter.rect_filled(bar, Rounding::same(2.0), color::ACCENT);
    }
    let tint = if selected { color::ACCENT } else { color::TEXT };
    painter.text(
        rect.left_center() + egui::vec2(14.0, 0.0),
        egui::Align2::LEFT_CENTER,
        glyph,
        style::icon(),
        tint,
    );
    painter.text(
        rect.left_center() + egui::vec2(42.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        style::body(),
        tint,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The search field, with the magnifying glass.
pub fn search(ui: &mut Ui, text: &mut String, hint: &str) -> Response {
    Frame::none()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(Stroke::new(1.0_f32, color::BORDER))
        .rounding(Rounding::same(style::RADIUS))
        .inner_margin(Margin::symmetric(8.0, 4.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::MAGNIFYING_GLASS).color(color::TEXT_DIM));
                ui.add(
                    egui::TextEdit::singleline(text)
                        .hint_text(hint)
                        .frame(false)
                        .desired_width(f32::INFINITY),
                )
            })
            .inner
        })
        .inner
}

/// Whether `text` contains the search `query` (already lowercase, as the
/// searches keep it), ignoring case: `orc` finds `OrcCaptain`.
pub fn found(text: &str, query: &str) -> bool {
    text.to_lowercase().contains(query)
}

/// A list row: selected in gold, with a smaller text on the side.
pub fn list_row(ui: &mut Ui, selected: bool, label: &str, detail: Option<&str>) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), Sense::click());
    let hover = ui
        .ctx()
        .animate_bool(response.id.with("hover"), response.hovered());
    let fill = if selected {
        color::ACCENT_BG
    } else {
        color::SURFACE_2.gamma_multiply(hover)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, Rounding::same(4.0), fill);
    let label = painter.text(
        rect.left_center() + egui::vec2(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        style::body(),
        if selected { color::ACCENT } else { color::TEXT },
    );
    let mut response = response;
    if let Some(detail) = detail {
        // The detail on the right when it fits next to the name; otherwise in
        // the tooltip.
        let galley = painter.layout_no_wrap(detail.to_string(), style::small(), color::TEXT_DIM);
        if label.right() + 12.0 + galley.size().x <= rect.right() - 10.0 {
            painter.galley(
                rect.right_center() - egui::vec2(10.0 + galley.size().x, galley.size().y / 2.0),
                galley,
                color::TEXT_DIM,
            );
        } else {
            response = response.on_hover_text(detail);
        }
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The detail header: the module's icon, the name and the kind below it.
pub fn detail_header(ui: &mut Ui, glyph: &str, title: &str, kind: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(glyph).font(egui::FontId::proportional(30.0)));
        ui.vertical(|ui| {
            ui.label(RichText::new(title).font(style::title()).strong());
            ui.label(RichText::new(kind).color(color::TEXT_DIM));
        });
    });
    ui.add_space(4.0);
    ui.separator();
}

/// A column title: icon and name.
pub fn column_title(ui: &mut Ui, glyph: &str, title: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(glyph).font(egui::FontId::proportional(22.0)));
        ui.label(RichText::new(title).font(style::title()));
    });
}

/// A collapsible detail section, in a card.
pub fn section<R>(
    ui: &mut Ui,
    id: &str,
    title: &str,
    open: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    card_frame()
        .inner_margin(Margin::symmetric(10.0, 6.0))
        .show(ui, |ui| {
            // The section takes the whole width, open or closed.
            ui.set_min_width(ui.available_width());
            egui::CollapsingHeader::new(RichText::new(title).strong())
                .id_salt(id)
                .default_open(open)
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    add(ui)
                })
                .body_returned
        })
        .inner
}

/// A grid of fields: dimmed label on the left, the control on the right.
pub fn fields<R>(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing(egui::vec2(16.0, 8.0))
        .min_col_width(90.0)
        .show(ui, add)
        .inner
}

/// A field label.
pub fn label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).color(color::TEXT_DIM));
}

/// How an action button looks.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Normal,
    /// The main action: gold.
    Primary,
    /// Delete and the like: red.
    Danger,
}

/// A button with an icon and a text.
pub fn action(ui: &mut Ui, glyph: &str, text: &str, kind: Kind) -> Response {
    let label = format!("{glyph}  {text}");
    let (fill, fg) = match kind {
        Kind::Normal => (color::SURFACE_2, color::TEXT),
        Kind::Primary => (color::ACCENT, Color32::from_rgb(0x20, 0x18, 0x08)),
        Kind::Danger => (color::SURFACE_2, color::DANGER),
    };
    ui.add(
        egui::Button::new(RichText::new(label).color(fg))
            .fill(fill)
            .min_size(egui::vec2(96.0, 32.0)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// An icon-only button, with the tooltip under the mouse.
pub fn icon_button(ui: &mut Ui, glyph: &str, tip: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(glyph).font(style::icon())).frame(false))
        .on_hover_text(tip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The list column, on the left. Call before `detail_panel`.
pub fn list_panel(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    list_panel_sized(ui, id, style::LIST_WIDTH, add);
}

/// The list column with another width (the slot grid is wider).
pub fn list_panel_sized(ui: &mut Ui, id: &str, width: f32, add: impl FnOnce(&mut Ui)) {
    egui::SidePanel::left(format!("{id}-list"))
        .resizable(false)
        .exact_width(width)
        .frame(
            Frame::none()
                .fill(color::BG)
                .inner_margin(Margin::same(12.0))
                .stroke(Stroke::new(1.0_f32, color::BORDER)),
        )
        .show_inside(ui, add);
}

/// The detail, in what is left to the right of the list column.
pub fn detail_panel(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::CentralPanel::default()
        .frame(
            Frame::none()
                .fill(color::BG)
                .inner_margin(Margin::same(14.0)),
        )
        .show_inside(ui, add);
}

/// What to show when nothing is selected or the list is empty.
pub fn empty(ui: &mut Ui, glyph: &str, text: &str) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(glyph)
                .font(egui::FontId::proportional(42.0))
                .color(color::BORDER),
        );
        ui.label(RichText::new(text).color(color::TEXT_DIM));
    });
}

/// The notices that show up in the corner and fade on their own: the
/// "saved", the errors.
#[derive(Default)]
pub struct Toasts {
    /// The text, whether it is an error, and until when (set the first time
    /// it shows).
    items: Vec<(String, bool, Option<f64>)>,
}

impl Toasts {
    /// How long a notice stays, in seconds.
    const LIFE: f64 = 3.5;

    pub fn push(&mut self, text: impl Into<String>, error: bool) {
        self.items.push((text.into(), error, None));
        if self.items.len() > 4 {
            self.items.remove(0);
        }
    }

    /// The live notices, at the bottom right of the screen, fading out in
    /// their last half second.
    pub fn show(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        for (_, _, until) in &mut self.items {
            until.get_or_insert(now + Self::LIFE);
        }
        self.items
            .retain(|(_, _, until)| until.is_some_and(|u| u > now));
        if self.items.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new("admin-toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -16.0])
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                for (text, error, until) in &self.items {
                    let left = until.unwrap_or(now + Self::LIFE) - now;
                    let alpha = (left / 0.5).clamp(0.0, 1.0) as f32;
                    let (glyph, tint) = if *error {
                        (icon::WARNING_CIRCLE, color::DANGER)
                    } else {
                        (icon::CHECK_CIRCLE, color::SUCCESS)
                    };
                    Frame::none()
                        .fill(color::SURFACE_2.gamma_multiply(alpha))
                        .stroke(Stroke::new(1.0_f32, tint.gamma_multiply(alpha)))
                        .rounding(Rounding::same(style::RADIUS))
                        .inner_margin(Margin::symmetric(12.0, 8.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(glyph).color(tint.gamma_multiply(alpha)));
                                ui.label(
                                    RichText::new(text).color(color::TEXT.gamma_multiply(alpha)),
                                );
                            });
                        });
                    ui.add_space(6.0);
                }
            });
        ctx.request_repaint();
    }
}

/// The actions bar at the foot of the detail, aligned to the right. Call
/// before `scroll_body`, which takes the rest of the height.
pub fn actions_bar(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    egui::TopBottomPanel::bottom(format!("{id}-actions"))
        .frame(
            Frame::none()
                .fill(color::BG)
                .inner_margin(Margin::symmetric(0.0, 8.0)),
        )
        .show_separator_line(true)
        .show_inside(ui, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), add);
        });
}

/// The detail body, which scrolls.
pub fn scroll_body(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    egui::ScrollArea::vertical()
        .id_salt(format!("{id}-body"))
        .auto_shrink([false, false])
        .show(ui, add);
}

/// The art card: the image centered in a fixed frame and the caption below.
/// The frame does not change size with the image, so nothing around it jumps.
pub fn art_card(ui: &mut Ui, texture: Option<&egui::TextureHandle>, caption: &str) {
    card_frame().show(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_width(140.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(140.0, 100.0), Sense::hover());
            match texture {
                Some(texture) => {
                    let size = texture.size_vec2();
                    let scale = (rect.width() / size.x).min(rect.height() / size.y).min(2.0);
                    let shown = egui::Rect::from_center_size(rect.center(), size * scale);
                    ui.painter().image(
                        texture.id(),
                        shown,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                None => {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        icon::IMAGE_BROKEN,
                        egui::FontId::proportional(32.0),
                        color::BORDER,
                    );
                }
            }
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(caption)
                        .font(style::small())
                        .color(color::TEXT_DIM),
                );
            });
        });
    });
}

/// A three-state filter for the list column: any, only those that have the
/// thing, only those that don't.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Has {
    #[default]
    Any,
    Yes,
    No,
}

impl Has {
    pub fn keeps(self, has: bool) -> bool {
        match self {
            Has::Any => true,
            Has::Yes => has,
            Has::No => !has,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Has::Any => "Any",
            Has::Yes => "Yes",
            Has::No => "No",
        }
    }
}

/// The selector of a `Has` filter.
pub fn has_combo(ui: &mut Ui, id: &str, value: &mut Has) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.label())
        .width(110.0)
        .show_ui(ui, |ui| {
            for v in [Has::Any, Has::Yes, Has::No] {
                ui.selectable_value(value, v, v.label());
            }
        });
}

/// The small text of a chip, already laid out.
fn chip_galley(ui: &Ui, text: &str, tint: Color32) -> std::sync::Arc<egui::Galley> {
    ui.painter()
        .layout_no_wrap(text.to_string(), style::small(), tint)
}

/// The size of a chip for the text.
fn chip_size(galley: &egui::Galley) -> egui::Vec2 {
    galley.size() + egui::vec2(14.0, 4.0)
}

/// A pill: a rect fully rounded at the ends, with the 1-pixel border drawn
/// inside it.
fn paint_pill(ui: &Ui, rect: egui::Rect, fill: Color32, border: Color32) {
    // egui 0.29 centers strokes on the edge; shrinking by half the width
    // keeps the border inside, as the source's `StrokeKind::Inside` did.
    let rect = rect.shrink(0.5);
    ui.painter().rect(
        rect,
        Rounding::same(rect.height() / 2.0),
        fill,
        Stroke::new(1.0_f32, border),
    );
}

/// Draws a chip: the pill with the color's dimmed background and the text on it.
fn paint_chip(ui: &Ui, rect: egui::Rect, galley: std::sync::Arc<egui::Galley>, tint: Color32) {
    paint_pill(
        ui,
        rect,
        tint.gamma_multiply(0.16),
        tint.gamma_multiply(0.55),
    );
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, tint);
}

/// A chip: a short state (online, banned, general) in a pill of the given color.
pub fn chip(ui: &mut Ui, text: &str, tint: Color32) -> Response {
    let galley = chip_galley(ui, text, tint);
    let (rect, response) = ui.allocate_exact_size(chip_size(&galley), Sense::hover());
    paint_chip(ui, rect, galley, tint);
    response
}

/// A chip that toggles on and off, to pick several from a list (creatures,
/// flags): on in gold; the border lights up under the mouse.
pub fn toggle_chip(ui: &mut Ui, on: &mut bool, text: &str) -> Response {
    let tint = if *on { color::ACCENT } else { color::TEXT_DIM };
    let galley = chip_galley(ui, text, tint);
    let mut size = galley.size() + egui::vec2(18.0, 8.0);
    // As tall as the other controls around it (a toolbar sets `interact_size`).
    size.y = size.y.max(ui.spacing().interact_size.y);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    let hover = ui
        .ctx()
        .animate_bool(response.id.with("hover"), response.hovered());
    let (fill, border) = if *on {
        (color::ACCENT_BG, color::ACCENT)
    } else {
        (
            color::SURFACE_2,
            color::BORDER.lerp_to_gamma(color::ACCENT, hover),
        )
    };
    paint_pill(ui, rect, fill, border);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, tint);
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A list row with a chip on the right, in place of the smaller text.
pub fn list_row_chip(
    ui: &mut Ui,
    selected: bool,
    label: &str,
    chip: Option<(&str, Color32)>,
) -> Response {
    let response = list_row(ui, selected, label, None);
    if let Some((text, tint)) = chip {
        let galley = chip_galley(ui, text, tint);
        let size = chip_size(&galley);
        let min = response.rect.right_center() - egui::vec2(8.0 + size.x, size.y / 2.0);
        paint_chip(ui, egui::Rect::from_min_size(min, size), galley, tint);
    }
    response
}

/// A few options side by side, one selected (in gold), taking the given
/// width. Returns whether the choice changed.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    value: &mut T,
    options: &[(T, &str)],
    width: f32,
) -> bool {
    let mut changed = false;
    Frame::none()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(Stroke::new(1.0_f32, color::BORDER))
        .rounding(Rounding::same(style::RADIUS))
        .inner_margin(Margin::same(3.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let n = options.len().max(1) as f32;
                let each = ((width - 8.0 - 2.0 * (n - 1.0)) / n).max(40.0);
                for (option, text) in options {
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(each, 24.0), Sense::click());
                    let selected = *value == *option;
                    let hover = ui
                        .ctx()
                        .animate_bool(response.id.with("hover"), response.hovered());
                    let (fill, tint) = if selected {
                        (color::ACCENT_BG, color::ACCENT)
                    } else {
                        (
                            color::SURFACE_2.gamma_multiply(hover),
                            color::TEXT_DIM.lerp_to_gamma(color::TEXT, hover),
                        )
                    };
                    let painter = ui.painter();
                    painter.rect_filled(rect, Rounding::same(style::RADIUS - 2.0), fill);
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        text,
                        style::body(),
                        tint,
                    );
                    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                    if response.clicked() && !selected {
                        *value = *option;
                        changed = true;
                    }
                }
            });
        });
    changed
}

/// A selector of one name among the options, or none (the first in the list).
pub fn optional_name(ui: &mut Ui, id: &str, value: &mut Option<String>, options: &[String]) {
    let none = "None";
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.as_deref().unwrap_or(none))
        .width(200.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, none);
            for o in options {
                ui.selectable_value(value, Some(o.clone()), o.as_str());
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_ignores_case() {
        assert!(found("OrcCaptain", "orc"));
        assert!(found("rat", "rat"));
        assert!(!found("rat", "orc"));
    }

    #[test]
    fn has_filter_keeps_by_state() {
        assert!(Has::Any.keeps(true) && Has::Any.keeps(false));
        assert!(Has::Yes.keeps(true) && !Has::Yes.keeps(false));
        assert!(!Has::No.keeps(true) && Has::No.keeps(false));
    }
}
