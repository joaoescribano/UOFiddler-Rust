//! The current brush in view: in the bar, among the options of the tools that place something,
//! the art, name, id and hue of what it places (the static of the stamp and of the fill, the land
//! of the terrain brush); a click opens the palette on the tool's brush. Next to the cursor, a
//! thumbnail of what will be placed; on terrain with the automatic border, the preview of the
//! borders in use (a core tile and the ring of eight around it, with the pieces the stroke would
//! place), in the bar and at the cursor.

use super::art::{thumbnail, Art};
use super::borders;
use super::state::{MapEditor, Tool};
use super::text::{t, tf};
use crate::map::assets::MapAssets;
use crate::map::ui::{picker, style};
use eframe::egui::{self, Margin, RichText, Sense, Stroke};

/// What the tool's brush places, for the card and the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Brush {
    /// A static, with its hue.
    Static(u16, u16),
    /// A land tile.
    Land(u16),
    /// Tools without art: height and the eraser.
    None,
}

impl MapEditor {
    /// The current tool's brush. The stamp and terrain keep their own; the others that place
    /// something (the region fill) use the stamp's.
    pub(super) fn current_brush(&self) -> Brush {
        match self.tool {
            Tool::Terrain => Brush::Land(self.land),
            Tool::Height | Tool::Erase => Brush::None,
            _ => Brush::Static(self.graphic, self.hue),
        }
    }
}

/// The preview tiles (0 to 2 in x and y) as small diamonds, with each land's art; `tile` is the
/// width of one diamond.
fn mosaic(
    ui: &mut egui::Ui,
    state: &mut MapEditor,
    assets: &MapAssets,
    tiles: &[((i32, i32), u16)],
    tile: f32,
) {
    let half = tile / 2.0;
    let side = tile * 3.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), Sense::hover());
    let ctx = ui.ctx().clone();
    let painter = ui.painter_at(rect);
    // (0, 0) at the top, in the middle: x goes right and down, y goes left and down.
    let origin = rect.min + egui::vec2(side / 2.0 - half, 0.0);
    for &((x, y), graphic) in tiles {
        let at = origin + egui::vec2((x - y) as f32 * half, (x + y) as f32 * half);
        let cell = egui::Rect::from_min_size(at, egui::vec2(tile, tile));
        match state.land_art(&ctx, assets, graphic) {
            Some(texture) => {
                painter.image(
                    texture.id(),
                    cell,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            None => {
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        cell.center_top(),
                        cell.right_center(),
                        cell.center_bottom(),
                        cell.left_center(),
                    ],
                    style::color::SURFACE_2,
                    Stroke::new(1.0_f32, style::color::BORDER),
                ));
            }
        }
    }
}

/// The preview of the borders in use, when terrain paints with the automatic border.
fn border_preview(ui: &mut egui::Ui, state: &mut MapEditor, assets: &MapAssets, tile: f32) -> bool {
    if state.tool != Tool::Terrain || !state.auto_border {
        return false;
    }
    let Some((set, core)) = state.border_preview(assets) else {
        return false;
    };
    let tiles = borders::preview(&set, core);
    mosaic(ui, state, assets, &tiles, tile);
    true
}

/// The art in a box of `fit` pixels, enlarged up to 3 times (without smoothing): a small item
/// shows big too.
fn big_thumbnail(ui: &mut egui::Ui, texture: Option<&egui::TextureHandle>, fit: f32) {
    let Some(texture) = texture else {
        return thumbnail(ui, None, fit);
    };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(fit, fit), Sense::hover());
    let size = texture.size_vec2();
    let scale = (fit / size.x.max(size.y)).min(3.0);
    ui.painter().image(
        texture.id(),
        egui::Rect::from_center_size(rect.center(), size * scale),
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

/// The brush's name and art.
fn describe(state: &mut MapEditor, ctx: &egui::Context, assets: &MapAssets) -> (Art, String) {
    match state.current_brush() {
        Brush::Land(graphic) => (
            state.land_art(ctx, assets, graphic),
            assets.land_name(graphic).to_string(),
        ),
        Brush::Static(graphic, hue) => (
            state.art(ctx, assets, graphic, hue),
            assets.item_name(graphic).to_string(),
        ),
        Brush::None => (None, String::new()),
    }
}

/// The widest the name gets in the bar card; the rest becomes an ellipsis.
const NAME_WIDTH: f32 = 110.0;

/// The current brush in the bar, among the options of the tools that place something (stamp,
/// terrain, the region fill), compact at the row's height: the art in a box of that height, the
/// name cut with an ellipsis, the id in small type and, on terrain with the automatic border, the
/// preview of the borders at the same size. The whole name and the hue go in the tooltip. A
/// click opens the palette on the brush.
pub(super) fn bar_card(ui: &mut egui::Ui, state: &mut MapEditor, assets: &MapAssets, height: f32) {
    let brush = state.current_brush();
    if !matches!(state.tool, Tool::Stamp | Tool::Terrain | Tool::Region) {
        return;
    }
    let ctx = ui.ctx().clone();
    let (art, name) = describe(state, &ctx, assets);
    let inner = (height - 4.0).max(12.0);
    let frame = egui::Frame::none()
        .fill(style::color::SURFACE_2)
        .stroke(Stroke::new(1.0_f32, style::color::ACCENT.gamma_multiply(0.6)))
        .rounding(egui::Rounding::same(4.0))
        .inner_margin(Margin::symmetric(4.0, 1.0))
        .show(ui, |ui| {
            ui.set_height(inner);
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                big_thumbnail(ui, art.as_ref(), inner);
                let shown = if name.is_empty() { "-" } else { name.as_str() };
                // The name on the left, up to `NAME_WIDTH`; the rest becomes an ellipsis.
                ui.allocate_ui_with_layout(
                    egui::vec2(NAME_WIDTH, inner),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| ui.add(egui::Label::new(RichText::new(shown).strong()).truncate()),
                );
                details(ui, assets, brush);
                border_preview(ui, state, assets, inner / 3.0);
            });
        });
    let tip = match brush {
        Brush::Static(graphic, hue) => format!(
            "{name}  0x{graphic:04X}  {}",
            tf("map.brush_hue", &[&hue])
        ),
        Brush::Land(graphic) => format!("{name}  0x{graphic:04X}"),
        Brush::None => String::new(),
    };
    let response = frame
        .response
        .interact(Sense::click())
        .on_hover_text(format!("{tip}\n{}", t("map.brush_card_hint")))
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() && !state.palette {
        state.toggle_palette();
    }
}

/// The brush's id and hue, in small type.
fn details(ui: &mut egui::Ui, assets: &MapAssets, brush: Brush) {
    let small = |text: String| {
        RichText::new(text)
            .font(style::small())
            .color(style::color::TEXT_DIM)
    };
    match brush {
        Brush::Static(graphic, hue) => {
            ui.label(small(format!("0x{graphic:04X}")));
            let color = assets
                .hues
                .and_then(|hues| picker::hue_color(hues, hue))
                .filter(|_| hue > 0);
            if let Some(color) = color {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
                ui.painter().rect_filled(rect, egui::Rounding::same(2.0), color);
            }
        }
        Brush::Land(graphic) => {
            ui.label(small(format!("0x{graphic:04X}")));
        }
        Brush::None => {}
    }
}

/// The thumbnail next to the cursor, over the map: what the stamp places, or the land and the
/// border preview.
pub(super) fn follower(ctx: &egui::Context, state: &mut MapEditor, assets: &MapAssets) {
    let Some(at) = state.cursor_screen else {
        return;
    };
    if !matches!(state.tool, Tool::Stamp | Tool::Terrain) || state.stroke.is_some() {
        return;
    }
    let (art, _) = describe(state, ctx, assets);
    egui::Area::new(egui::Id::new("map-brush-follower"))
        .fixed_pos(at + egui::vec2(26.0, 18.0))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(style::color::SURFACE.gamma_multiply(0.85))
                .stroke(Stroke::new(1.0_f32, style::color::ACCENT))
                .rounding(egui::Rounding::same(4.0))
                .inner_margin(Margin::same(4.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        big_thumbnail(ui, art.as_ref(), 40.0);
                        border_preview(ui, state, assets, 18.0);
                    });
                });
        });
}

#[cfg(test)]
mod tests {
    use super::super::state::PaletteMode;
    use super::*;

    #[test]
    fn each_tool_shows_its_own_brush_and_the_palette_follows() {
        let mut editor = MapEditor::default();
        editor.graphic = 0x0E76;
        editor.hue = 33;
        editor.land = 0x0071;
        editor.choose(Tool::Terrain);
        assert_eq!(editor.current_brush(), Brush::Land(0x0071));
        assert_eq!(editor.palette_mode, PaletteMode::Lands);
        editor.choose(Tool::Stamp);
        assert_eq!(editor.current_brush(), Brush::Static(0x0E76, 33));
        assert_eq!(editor.palette_mode, PaletteMode::Objects);
        editor.choose(Tool::Height);
        assert_eq!(editor.current_brush(), Brush::None);
        // Back on terrain, the land from before comes back.
        editor.choose(Tool::Terrain);
        assert_eq!(editor.current_brush(), Brush::Land(0x0071));
        editor.toggle_palette();
        assert!(editor.palette);
        assert_eq!(editor.palette_mode, PaletteMode::Lands);
        editor.toggle_palette();
        assert!(!editor.palette);
    }
}
