//! What the map editor draws over the map: the grid, the stamp's ghosts, the stroke in progress,
//! the chosen tile, the region rectangles, the brush outline and the label with the cursor tile's
//! x, y, z.

use super::state::Overlay;
use super::types::Area;
use crate::map::assets::MapAssets;
use crate::map::view::{IsoView, Transform};
use eframe::egui::{self, Color32, Pos2, Stroke};

fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8,
        (a * 255.0) as u8,
    )
}

pub fn paint(
    ctx: &egui::Context,
    painter: &egui::Painter,
    t: &Transform,
    overlay: &Overlay,
    view: &mut IsoView,
    assets: &MapAssets,
) {
    let line = |a: Pos2, b: Pos2, width: f32, color: Color32| {
        painter.line_segment([a, b], Stroke::new(width, color));
    };
    // The grid: the two top sides of each visible tile with its ground inside the band.
    // Faint, and gone when zoomed far out (it would only be noise there).
    if let Some((from, to)) = overlay.grid.filter(|_| t.zoom >= 0.6) {
        let color = rgba(1.0, 1.0, 1.0, 0.035);
        let clip = t.rect.expand(50.0);
        for item in view.scene_items() {
            if !item.is_land() || !(from..=to).contains(&item.z) {
                continue;
            }
            let Some([top, right, _, left]) = item.diamond() else {
                continue;
            };
            let (top, right, left) = (t.to_screen(top), t.to_screen(right), t.to_screen(left));
            if !clip.contains(top) {
                continue;
            }
            line(top, right, 1.0, color);
            line(top, left, 1.0, color);
        }
    }
    for ghost in &overlay.ghosts {
        view.paint_static(
            ctx,
            painter,
            t,
            assets,
            (ghost.graphic, ghost.hue),
            (ghost.x, ghost.y, ghost.z),
            Color32::from_white_alpha(153),
        );
    }
    if let Some(color) = overlay.fill_color {
        for &(tile, z) in &overlay.fill {
            let [top, right, bottom, left] = t.diamond(tile.0, tile.1, z.map(i32::from));
            painter.add(egui::Shape::convex_polygon(
                vec![top, right, bottom, left],
                color,
                Stroke::NONE,
            ));
        }
    }
    let gold = rgba(1.0, 0.85, 0.4, 0.95);
    if let Some((tile, z)) = overlay.selected {
        let points = t.diamond(tile.0, tile.1, z.map(i32::from));
        for i in 0..4 {
            line(
                points[i],
                points[(i + 1) % 4],
                3.0,
                rgba(0.42, 0.72, 1.0, 0.95),
            );
        }
    }
    // A tile rectangle, on the visible ground of each corner (the editor gives the heights,
    // inside the band): the chosen region in blue, the one being drawn (or the clip to paste) in
    // orange.
    let rectangle = |(area, z): (Area, [i8; 4]), color: Color32| {
        let points = [
            t.diamond(area.x0, area.y0, [z[0] as i32; 4])[0],
            t.diamond(area.x1, area.y0, [z[1] as i32; 4])[1],
            t.diamond(area.x1, area.y1, [z[2] as i32; 4])[2],
            t.diamond(area.x0, area.y1, [z[3] as i32; 4])[3],
        ];
        for i in 0..4 {
            line(points[i], points[(i + 1) % 4], 2.0, color);
        }
    };
    if let Some(area) = overlay.region {
        rectangle(area, rgba(0.42, 0.72, 1.0, 0.95));
    }
    if let Some(area) = overlay.rect {
        rectangle(area, rgba(1.0, 0.6, 0.1, 0.95));
    }
    for &(tile, z, sides) in &overlay.outline {
        let [top, right, bottom, left] = t.diamond(tile.0, tile.1, z.map(i32::from));
        for (bit, a, b) in [
            (1, top, right),
            (2, right, bottom),
            (4, bottom, left),
            (8, left, top),
        ] {
            if sides & bit != 0 {
                line(a, b, 2.0, gold);
            }
        }
    }
    if let Some((tile, z, text)) = &overlay.label {
        let [_, right, ..] = t.diamond(tile.0, tile.1, z.map(i32::from));
        let font = egui::FontId::proportional(15.0);
        let galley = painter.layout_no_wrap(text.clone(), font, gold);
        let at = right + egui::vec2(10.0, -galley.size().y);
        let rect = egui::Rect::from_min_size(at, galley.size()).expand2(egui::vec2(4.0, 3.0));
        painter.rect_filled(rect, 2.0, rgba(0.0, 0.0, 0.0, 0.65));
        painter.galley(at, galley, gold);
    }
}
