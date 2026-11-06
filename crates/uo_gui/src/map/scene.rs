//! The scene of a region: land and statics sorted by depth, with the sprite and the position in
//! world pixels already resolved. Built again when the camera leaves the region or the map
//! changes, not every frame.

use super::assets::{Blocks, MapAssets};
use super::atlas::{Atlas, Sprite, SpriteKey};
use super::heights::HeightGrid;
use super::iso::{
    average_land_z, depth, flags, is_drawable_static, static_depth, tile_origin, TILE_HALF,
    TILE_SIZE, Z_STEP,
};
use eframe::egui::{self, pos2, Pos2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Region {
    pub fn contains(&self, other: &Region) -> bool {
        self.x0 <= other.x0 && self.y0 <= other.y0 && self.x1 >= other.x1 && self.y1 >= other.y1
    }

    pub fn snapped_to_blocks(&self) -> Region {
        Region {
            x0: self.x0.div_euclid(8) * 8,
            y0: self.y0.div_euclid(8) * 8,
            x1: self.x1.div_euclid(8) * 8 + 8,
            y1: self.y1.div_euclid(8) * 8 + 8,
        }
    }

    pub fn grown(&self, by: i32) -> Region {
        Region {
            x0: self.x0 - by,
            y0: self.y0 - by,
            x1: self.x1 + by,
            y1: self.y1 + by,
        }
    }
}

/// What the view hides, like the client does under a roof, and like the height band of the
/// editor: nothing below `floor` or above `cut` is drawn, and roofs go away with `hide_roofs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cut {
    pub floor: Option<i32>,
    pub cut: Option<i32>,
    pub hide_roofs: bool,
}

impl Cut {
    pub fn hides(&self, z: i32) -> bool {
        self.cut.is_some_and(|c| z > c) || self.floor.is_some_and(|f| z < f)
    }

    fn hides_static(&self, z: i32, tile_flags: u64) -> bool {
        self.hides(z) || (self.hide_roofs && tile_flags & flags::ROOF != 0)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    /// An atlas sprite, top-left corner in world pixels.
    Sprite { sprite: Sprite, at: Pos2 },
    /// Sloped land: the texture stretched over the diamond of the four corners (top, right,
    /// bottom, left), with the light of each.
    Quad {
        sprite: Sprite,
        points: [Pos2; 4],
        light: [f32; 4],
    },
}

#[derive(Debug, Clone, Copy)]
pub struct Item {
    pub kind: Kind,
    /// Box in world pixels, for culling and picking.
    pub min: Pos2,
    pub max: Pos2,
    pub depth: f32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub graphic: u16,
    /// `None` for land; the index of the static among the tile's statics otherwise.
    pub static_index: Option<usize>,
}

impl Item {
    pub fn is_land(&self) -> bool {
        self.static_index.is_none()
    }

    /// The land diamond in world pixels (top, right, bottom, left), for picking.
    pub fn diamond(&self) -> Option<[Pos2; 4]> {
        match self.kind {
            Kind::Quad { points, .. } => Some(points),
            Kind::Sprite { at, .. } if self.is_land() => Some([
                pos2(at.x + TILE_HALF, at.y),
                pos2(at.x + TILE_SIZE, at.y + TILE_HALF),
                pos2(at.x + TILE_HALF, at.y + TILE_SIZE),
                pos2(at.x, at.y + TILE_HALF),
            ]),
            _ => None,
        }
    }
}

/// What a scene was built from; a different key means building again.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneKey {
    pub map_id: usize,
    pub region: Region,
    pub cut: Cut,
    pub show_statics: bool,
    /// Bumped by whoever changes the map (edits, saves, reloads).
    pub generation: u64,
}

#[derive(Default)]
pub struct Scene {
    pub key: Option<SceneKey>,
    pub items: Vec<Item>,
    pub statics: usize,
}

/// Land below this graphic are "NO DRAW" markers.
const FIRST_DRAWN_LAND: u16 = 3;

impl Scene {
    /// Builds the scene of `key.region`. When the atlas fills up it is cleared and the build runs
    /// once more; what still does not fit is left out.
    pub fn build(
        &mut self,
        ctx: &egui::Context,
        atlas: &mut Atlas,
        assets: &MapAssets,
        key: SceneKey,
    ) {
        self.try_build(ctx, atlas, assets, key);
        if atlas.full {
            atlas.reset();
            self.try_build(ctx, atlas, assets, key);
        }
    }

    fn try_build(
        &mut self,
        ctx: &egui::Context,
        atlas: &mut Atlas,
        assets: &MapAssets,
        key: SceneKey,
    ) {
        let region = key.region;
        let mut blocks = Blocks::new(assets);
        // One block of border, for corner heights and normals.
        let border = region.grown(8);
        let heights = HeightGrid::new(border.x0, border.y0, border.x1, border.y1, |x, y| {
            blocks.land_z(x, y)
        });
        let mut items = Vec::new();
        let mut statics = 0;
        for by in region.y0.div_euclid(8)..region.y1.div_euclid(8) {
            for bx in region.x0.div_euclid(8)..region.x1.div_euclid(8) {
                let Some(block) = blocks.block(bx, by).cloned() else {
                    continue;
                };
                for cy in 0..8 {
                    for cx in 0..8 {
                        let (x, y) = (bx * 8 + cx, by * 8 + cy);
                        let cell = block.cell(x, y);
                        if key.cut.hides(cell.z as i32) {
                            continue;
                        }
                        if let Some(item) =
                            land_item(ctx, atlas, assets, &heights, x, y, cell.graphic)
                        {
                            items.push(item);
                        }
                    }
                }
                if !key.show_statics {
                    continue;
                }
                let mut seen: std::collections::HashMap<(u8, u8), usize> = Default::default();
                for s in &block.statics {
                    let index = seen.entry((s.x, s.y)).or_insert(0);
                    let static_index = *index;
                    *index += 1;
                    let tile_flags = assets.item_flags(s.graphic);
                    if !is_drawable_static(s.graphic, tile_flags) {
                        continue;
                    }
                    if key.cut.hides_static(s.z as i32, tile_flags) {
                        continue;
                    }
                    let (x, y) = (bx * 8 + s.x as i32, by * 8 + s.y as i32);
                    let hue = s.hue & 0x3FFF;
                    let Some(sprite) = atlas.get(
                        ctx,
                        SpriteKey::Static {
                            graphic: s.graphic,
                            hue,
                        },
                        || assets.static_image(s.graphic, hue),
                    ) else {
                        continue;
                    };
                    let (ox, oy) = tile_origin(x, y);
                    let at = pos2(
                        ox + TILE_HALF - (sprite.w / 2.0).floor(),
                        oy + TILE_SIZE - sprite.h - s.z as f32 * Z_STEP,
                    );
                    items.push(Item {
                        kind: Kind::Sprite { sprite, at },
                        min: at,
                        max: pos2(at.x + sprite.w, at.y + sprite.h),
                        depth: static_depth(
                            x,
                            y,
                            s.z as i32,
                            tile_flags,
                            assets.item_height(s.graphic),
                        ),
                        x,
                        y,
                        z: s.z as i32,
                        graphic: s.graphic,
                        static_index: Some(static_index),
                    });
                    statics += 1;
                }
            }
        }
        items.sort_by(|a, b| {
            a.depth
                .partial_cmp(&b.depth)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.items = items;
        self.statics = statics;
        self.key = Some(key);
    }
}

/// Land: the stretched texture when the tile has one and some corner has relief; the flat 44x44
/// art otherwise.
fn land_item(
    ctx: &egui::Context,
    atlas: &mut Atlas,
    assets: &MapAssets,
    heights: &HeightGrid,
    x: i32,
    y: i32,
    graphic: u16,
) -> Option<Item> {
    if graphic < FIRST_DRAWN_LAND {
        return None;
    }
    let (ox, oy) = tile_origin(x, y);
    let z = heights.z(x, y);
    let texture = assets.land_texture(graphic);
    let corners = [
        z,
        heights.z(x + 1, y),
        heights.z(x + 1, y + 1),
        heights.z(x, y + 1),
    ];
    let vertices = [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)];
    let relief = vertices
        .iter()
        .any(|&(vx, vy)| heights.normal(vx, vy).is_some());
    if texture != 0 && relief {
        if let Some(sprite) = atlas.get(ctx, SpriteKey::Texture(texture), || {
            assets.texture_image(texture)
        }) {
            let points = [
                pos2(ox + TILE_HALF, oy - corners[0] as f32 * Z_STEP),
                pos2(ox + TILE_SIZE, oy + TILE_HALF - corners[1] as f32 * Z_STEP),
                pos2(ox + TILE_HALF, oy + TILE_SIZE - corners[2] as f32 * Z_STEP),
                pos2(ox, oy + TILE_HALF - corners[3] as f32 * Z_STEP),
            ];
            let light = std::array::from_fn(|i| heights.light(vertices[i].0, vertices[i].1));
            let min = points.iter().fold(points[0], |m, p| m.min(*p));
            let max = points.iter().fold(points[0], |m, p| m.max(*p));
            return Some(Item {
                kind: Kind::Quad {
                    sprite,
                    points,
                    light,
                },
                min,
                max,
                depth: depth(x, y, average_land_z(corners) - 2),
                x,
                y,
                z,
                graphic,
                static_index: None,
            });
        }
    }
    let sprite = atlas.get(ctx, SpriteKey::Land(graphic), || assets.land_image(graphic))?;
    let at = pos2(ox, oy - z as f32 * Z_STEP);
    Some(Item {
        kind: Kind::Sprite { sprite, at },
        min: at,
        max: pos2(at.x + sprite.w, at.y + sprite.h),
        depth: depth(x, y, z - 2),
        x,
        y,
        z,
        graphic,
        static_index: None,
    })
}

/// Point inside a convex quad given in order.
pub fn in_quad(p: Pos2, q: &[Pos2; 4]) -> bool {
    let mut sign = 0.0f32;
    for i in 0..4 {
        let (a, b) = (q[i], q[(i + 1) % 4]);
        let c = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        if c != 0.0 {
            if sign != 0.0 && c.signum() != sign {
                return false;
            }
            sign = c.signum();
        }
    }
    true
}
