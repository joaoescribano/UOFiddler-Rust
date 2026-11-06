//! The isometric map view: a camera over the facet (drag to pan, wheel to zoom), the scene drawn
//! through the atlas in a few meshes, and the tile under the cursor.

use super::assets::MapAssets;
use super::atlas::{Atlas, SpriteKey};
use super::iso::{tile_at, tile_origin, TILE_HALF, Z_STEP};
use super::scene::{in_quad, Cut, Item, Kind, Region, Scene, SceneKey};
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, Sense};

pub const MIN_ZOOM: f32 = 0.35;
pub const MAX_ZOOM: f32 = 4.0;

/// World pixels <-> screen pixels for one frame.
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub rect: Rect,
    pub camera: Pos2,
    pub zoom: f32,
}

impl Transform {
    pub fn to_screen(&self, world: Pos2) -> Pos2 {
        self.rect.center() + (world - self.camera) * self.zoom
    }

    pub fn to_world(&self, screen: Pos2) -> Pos2 {
        self.camera + (screen - self.rect.center()) / self.zoom
    }

    /// The four corners of a tile's diamond on screen, at the given corner heights.
    pub fn diamond(&self, x: i32, y: i32, z: [i32; 4]) -> [Pos2; 4] {
        let (ox, oy) = tile_origin(x, y);
        [
            pos2(ox + TILE_HALF, oy - z[0] as f32 * Z_STEP),
            pos2(ox + 2.0 * TILE_HALF, oy + TILE_HALF - z[1] as f32 * Z_STEP),
            pos2(ox + TILE_HALF, oy + 2.0 * TILE_HALF - z[2] as f32 * Z_STEP),
            pos2(ox, oy + TILE_HALF - z[3] as f32 * Z_STEP),
        ]
        .map(|p| self.to_screen(p))
    }
}

/// What the cursor is over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hover {
    pub x: i32,
    pub y: i32,
    /// Height of the land under the cursor.
    pub z: i32,
    /// The front-most static under the cursor: (graphic, z, index among the tile's statics).
    pub static_hit: Option<(i32, i32, u16, i32, usize)>,
}

pub struct IsoView {
    /// Camera centre in world pixels.
    pub camera: Pos2,
    pub zoom: f32,
    pub cut: Cut,
    pub show_statics: bool,
    pub show_grid: bool,
    /// Bump to rebuild the scene (the map changed under it).
    pub generation: u64,
    atlas: Atlas,
    scene: Scene,
    /// Drag with the left button pans (the viewer); the editor pans with the middle/right one.
    pub left_drag_pans: bool,
}

impl Default for IsoView {
    fn default() -> Self {
        Self {
            camera: Pos2::ZERO,
            zoom: 1.0,
            cut: Cut::default(),
            show_statics: true,
            show_grid: false,
            generation: 0,
            atlas: Atlas::default(),
            scene: Scene::default(),
            left_drag_pans: true,
        }
    }
}

impl IsoView {
    /// Puts the camera over a tile.
    pub fn center_on(&mut self, x: i32, y: i32, z: i32) {
        let (ox, oy) = tile_origin(x, y);
        self.camera = pos2(ox + TILE_HALF, oy + TILE_HALF - z as f32 * Z_STEP);
    }

    /// The tile at the centre of the view (at height zero).
    pub fn center_tile(&self) -> (i32, i32) {
        tile_at(self.camera.x, self.camera.y)
    }

    pub fn scene_items(&self) -> &[Item] {
        &self.scene.items
    }

    pub fn statics_drawn(&self) -> usize {
        self.scene.statics
    }

    pub fn sprites_cached(&self) -> usize {
        self.atlas.len()
    }

    /// The tiles a screen rectangle can show: its corners in tile space, with room for land
    /// raised up to z 127 and statics hanging above the top edge.
    fn region_for(&self, t: &Transform) -> Region {
        let tl = t.to_world(t.rect.left_top());
        let br = t.to_world(t.rect.right_bottom());
        let (top, bottom) = (tl.y - 160.0, br.y + 127.0 * Z_STEP + 120.0);
        let h = TILE_HALF * 2.0;
        let x0 = ((tl.x + top) / h).floor() as i32;
        let x1 = ((br.x + bottom) / h).ceil() as i32;
        let y0 = ((top - br.x) / h).floor() as i32;
        let y1 = ((bottom - tl.x) / h).ceil() as i32;
        Region { x0, y0, x1, y1 }
    }

    /// Lays out the view in `rect`, rebuilding the scene when needed, and draws it.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        assets: &MapAssets,
    ) -> (egui::Response, Transform, Option<Hover>) {
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        let pan = if self.left_drag_pans {
            response.dragged_by(egui::PointerButton::Primary)
                || response.dragged_by(egui::PointerButton::Middle)
                || response.dragged_by(egui::PointerButton::Secondary)
        } else {
            response.dragged_by(egui::PointerButton::Middle)
                || response.dragged_by(egui::PointerButton::Secondary)
        };
        if pan {
            self.camera -= response.drag_delta() / self.zoom;
        }
        if response.hovered() {
            let (scroll, ctrl) = ui.input(|i| (i.raw_scroll_delta.y, i.modifiers.command));
            if scroll != 0.0 && !ctrl {
                // Zoom around the cursor.
                let before = Transform {
                    rect,
                    camera: self.camera,
                    zoom: self.zoom,
                };
                let pointer = response.hover_pos().unwrap_or(rect.center());
                let anchor = before.to_world(pointer);
                let factor = if scroll > 0.0 { 1.15 } else { 1.0 / 1.15 };
                self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
                self.camera = anchor - (pointer - rect.center()) / self.zoom;
            }
        }
        let t = Transform {
            rect,
            camera: self.camera,
            zoom: self.zoom,
        };

        let needed = self.region_for(&t);
        let current = self.scene.key;
        let fresh = |key: &SceneKey| {
            key.map_id == assets.map_id
                && key.cut == self.cut
                && key.show_statics == self.show_statics
                && key.generation == self.generation
                && key.region.contains(&needed)
        };
        if !current.as_ref().is_some_and(fresh) {
            let key = SceneKey {
                map_id: assets.map_id,
                // Two blocks of slack, so panning does not rebuild every few tiles.
                region: needed.grown(16).snapped_to_blocks(),
                cut: self.cut,
                show_statics: self.show_statics,
                generation: self.generation,
            };
            self.scene.build(ui.ctx(), &mut self.atlas, assets, key);
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(10, 12, 14));
        self.paint(&painter, &t);
        if self.show_grid {
            self.paint_grid(&painter, &t);
        }
        let hover = response.hover_pos().and_then(|p| self.pick(&t, p));
        (response, t, hover)
    }

    fn paint(&self, painter: &egui::Painter, t: &Transform) {
        let view = Rect::from_min_max(
            t.to_world(t.rect.left_top()),
            t.to_world(t.rect.right_bottom()),
        );
        let mut mesh: Option<egui::Mesh> = None;
        let mut page = usize::MAX;
        let flush = |mesh: &mut Option<egui::Mesh>| {
            if let Some(m) = mesh.take() {
                if !m.is_empty() {
                    painter.add(egui::Shape::mesh(m));
                }
            }
        };
        for item in &self.scene.items {
            if item.max.x < view.min.x
                || item.min.x > view.max.x
                || item.max.y < view.min.y
                || item.min.y > view.max.y
            {
                continue;
            }
            let sprite = match item.kind {
                Kind::Sprite { sprite, .. } | Kind::Quad { sprite, .. } => sprite,
            };
            if sprite.page != page || mesh.as_ref().is_some_and(|m| m.vertices.len() > 60_000) {
                flush(&mut mesh);
                page = sprite.page;
                mesh = Some(egui::Mesh::with_texture(self.atlas.texture(page)));
            }
            let m = mesh.as_mut().unwrap();
            match item.kind {
                Kind::Sprite { sprite, at } => {
                    let r = Rect::from_min_size(t.to_screen(at), vec2(sprite.w, sprite.h) * t.zoom);
                    let uv = Rect::from_min_max(sprite.uv(0.0, 0.0), sprite.uv(1.0, 1.0));
                    m.add_rect_with_uv(r, uv, Color32::WHITE);
                }
                Kind::Quad {
                    sprite,
                    points,
                    light,
                } => {
                    // The texture is a rotated square: top (0,0), right (1,0), bottom (1,1),
                    // left (0,1); half a texel in so the atlas edge never shows at the seams.
                    let inset_u = 0.5 / sprite.w;
                    let inset_v = 0.5 / sprite.h;
                    let uv = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
                    let base = m.vertices.len() as u32;
                    for i in 0..4 {
                        let (u, v) = uv[i];
                        let shade = (light[i] * 255.0) as u8;
                        m.vertices.push(egui::epaint::Vertex {
                            pos: t.to_screen(points[i]),
                            uv: sprite.uv(
                                inset_u + u * (1.0 - 2.0 * inset_u),
                                inset_v + v * (1.0 - 2.0 * inset_v),
                            ),
                            color: Color32::from_rgb(shade, shade, shade),
                        });
                    }
                    m.indices.extend_from_slice(&[
                        base,
                        base + 1,
                        base + 2,
                        base,
                        base + 2,
                        base + 3,
                    ]);
                }
            }
        }
        flush(&mut mesh);
    }

    fn paint_grid(&self, painter: &egui::Painter, t: &Transform) {
        let stroke = egui::Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 14));
        for item in &self.scene.items {
            if let Some(d) = item.diamond() {
                let d = d.map(|p| t.to_screen(p));
                if !t.rect.contains(d[0]) && !t.rect.contains(d[2]) {
                    continue;
                }
                painter.add(egui::Shape::closed_line(d.to_vec(), stroke));
            }
        }
    }

    /// The land tile and the front-most static under a screen point.
    pub fn pick(&self, t: &Transform, screen: Pos2) -> Option<Hover> {
        let p = t.to_world(screen);
        let mut land: Option<&Item> = None;
        let mut stat: Option<&Item> = None;
        for item in self.scene.items.iter().rev() {
            if p.x < item.min.x || p.x >= item.max.x || p.y < item.min.y || p.y >= item.max.y {
                continue;
            }
            if item.is_land() {
                if land.is_none() && item.diamond().is_some_and(|d| in_quad(p, &d)) {
                    land = Some(item);
                }
            } else if stat.is_none() && self.opaque_at(item, p) {
                stat = Some(item);
            }
            if land.is_some() && stat.is_some() {
                break;
            }
        }
        let (x, y, z) = match (land, stat) {
            (Some(l), _) => (l.x, l.y, l.z),
            (None, Some(s)) => (s.x, s.y, s.z),
            (None, None) => {
                let (x, y) = tile_at(p.x, p.y);
                (x, y, 0)
            }
        };
        // A static only counts when it is in front of the land picked.
        let static_hit = stat
            .filter(|s| land.is_none_or(|l| s.depth >= l.depth))
            .map(|s| (s.x, s.y, s.graphic, s.z, s.static_index.unwrap_or(0)));
        Some(Hover {
            x,
            y,
            z,
            static_hit,
        })
    }

    /// Draws one static, tinted (the editor's see-through ghosts), at a tile and height.
    #[allow(clippy::too_many_arguments)]
    pub fn paint_static(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        t: &Transform,
        assets: &MapAssets,
        (graphic, hue): (u16, u16),
        (x, y, z): (i32, i32, i32),
        tint: Color32,
    ) {
        let hue = hue & 0x3FFF;
        let Some(sprite) = self.atlas.get(ctx, SpriteKey::Static { graphic, hue }, || {
            assets.static_image(graphic, hue)
        }) else {
            return;
        };
        let (ox, oy) = tile_origin(x, y);
        let at = pos2(
            ox + TILE_HALF - (sprite.w / 2.0).floor(),
            oy + 2.0 * TILE_HALF - sprite.h - z as f32 * Z_STEP,
        );
        let r = Rect::from_min_size(t.to_screen(at), vec2(sprite.w, sprite.h) * t.zoom);
        let uv = Rect::from_min_max(sprite.uv(0.0, 0.0), sprite.uv(1.0, 1.0));
        let mut mesh = egui::Mesh::with_texture(self.atlas.texture(sprite.page));
        mesh.add_rect_with_uv(r, uv, tint);
        painter.add(egui::Shape::mesh(mesh));
    }

    /// Statics are picked by their box (the atlas pixels live on the GPU); a box that is mostly
    /// empty, like a tree, still picks, which is what an editor wants anyway.
    fn opaque_at(&self, _item: &Item, _p: Pos2) -> bool {
        true
    }
}
