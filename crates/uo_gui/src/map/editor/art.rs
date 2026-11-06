//! The art of statics and lands as egui textures, for the inspector, the brush card and the
//! border dialog (the map itself draws through the view's atlas).

use super::state::MapEditor;
use crate::map::assets::MapAssets;
use crate::map::atlas::Image;
use crate::map::ui::style;
use eframe::egui;
use egui_phosphor::regular as icon;

/// The art of a graphic: the texture, or nothing when it has no art.
pub(super) type Art = Option<egui::TextureHandle>;

impl MapEditor {
    /// The art of a static (with a hue), as an egui texture.
    pub(super) fn art(
        &mut self,
        ctx: &egui::Context,
        assets: &MapAssets,
        graphic: u16,
        hue: u16,
    ) -> Art {
        if let Some(known) = self.art.get(&(graphic, hue)) {
            return known.clone();
        }
        let texture = assets
            .static_image(graphic, hue)
            .map(|i| texture(ctx, &format!("map-art-{graphic}-{hue}"), i));
        self.art.insert((graphic, hue), texture.clone());
        texture
    }

    /// The art of a land tile, as an egui texture.
    pub(super) fn land_art(
        &mut self,
        ctx: &egui::Context,
        assets: &MapAssets,
        graphic: u16,
    ) -> Art {
        if let Some(known) = self.land_art.get(&graphic) {
            return known.clone();
        }
        let texture = assets
            .land_image(graphic)
            .map(|i| texture(ctx, &format!("map-land-{graphic}"), i));
        self.land_art.insert(graphic, texture.clone());
        texture
    }
}

/// A UO image as an egui texture, without smoothing the pixels.
fn texture(ctx: &egui::Context, name: &str, image: Image) -> egui::TextureHandle {
    let pixels = egui::ColorImage::from_rgba_unmultiplied([image.width, image.height], &image.rgba);
    ctx.load_texture(name, pixels, egui::TextureOptions::NEAREST)
}

/// A texture in a box of at most `fit` pixels, centred.
pub(super) fn thumbnail(ui: &mut egui::Ui, texture: Option<&egui::TextureHandle>, fit: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(fit, fit), egui::Sense::hover());
    let Some(texture) = texture else {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icon::IMAGE_BROKEN,
            style::icon(),
            style::color::BORDER,
        );
        return;
    };
    let size = texture.size_vec2();
    let scale = (fit / size.x.max(size.y)).min(1.0);
    ui.painter().image(
        texture.id(),
        egui::Rect::from_center_size(rect.center(), size * scale),
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}
