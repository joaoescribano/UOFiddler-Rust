//! The terrain brush: the automatic borders between two lands and the sets to vary. UO does
//! not say in the tiledata which land tile is the border of which: the borders carry the name
//! of one of the two lands and the art shows each corner of the diamond on one side. So the
//! art is read: the color of each corner (a small patch near it, inwards) tells which land it
//! belongs to, and the tiles of the two names are grouped by the **corner mask** of the inside
//! land.
//!
//! When painting, the core (the stroke's tiles) gets the whole land, and the ring around it
//! gets the border: a grid vertex is inside when one of the four tiles touching it is in the
//! core, and each ring tile gets the corners touching it, added to those it already had of the
//! inside land (a border painted earlier does not open a hole).
//!
//! With a fixed working Z, the core and the borders that change also get the ground height at
//! that Z (`painted`); in automatic mode, the height stays.

use crate::map::assets::MapAssets;
use crate::map::iso::flags;
use std::collections::{HashMap, HashSet};
use uo_core::MapTile;

/// The corners of a tile's diamond, as bits.
pub use super::types::corner::{ALL, BOTTOM, LEFT, RIGHT, TOP};

/// An RGB color in floating point.
pub type Rgb = [f32; 3];

/// The black of the void, around caves.
pub const VOID: Rgb = [0.0, 0.0, 0.0];

/// The side of the land art.
const SIZE: u32 = 44;
/// The sample points near each corner, set back inside the diamond.
const SAMPLES: [(u8, u32, u32); 4] = [
    (TOP, 22, 8),
    (RIGHT, 35, 22),
    (BOTTOM, 22, 35),
    (LEFT, 8, 22),
];
/// Half the side of the sample patch.
const PATCH: i32 = 3;

/// The average color of a small patch around a point of a 44x44 RGBA art. Transparent pixels
/// are left out.
fn patch_color(rgba: &[u8], cx: u32, cy: u32) -> Option<Rgb> {
    let mut sum = [0.0f32; 3];
    let mut count = 0.0;
    for dy in -PATCH..=PATCH {
        for dx in -PATCH..=PATCH {
            let (x, y) = (cx as i32 + dx, cy as i32 + dy);
            let at = ((y as u32 * SIZE + x as u32) * 4) as usize;
            if rgba[at + 3] < 128 {
                continue;
            }
            for c in 0..3 {
                sum[c] += rgba[at + c] as f32;
            }
            count += 1.0;
        }
    }
    (count > 0.0).then(|| [sum[0] / count, sum[1] / count, sum[2] / count])
}

/// The average color of each corner (top, right, bottom, left) of a 44x44 RGBA land art.
pub fn corner_colors(width: u32, height: u32, rgba: &[u8]) -> Option<[Rgb; 4]> {
    if width != SIZE || height != SIZE || rgba.len() < (SIZE * SIZE * 4) as usize {
        return None;
    }
    let mut out = [[0.0; 3]; 4];
    for (i, &(_, cx, cy)) in SAMPLES.iter().enumerate() {
        out[i] = patch_color(rgba, cx, cy)?;
    }
    Some(out)
}

/// The art is of a single land, without a border: the corners, the middle of the sides and
/// the center have almost the same color. The corners alone are not enough: some borders have
/// the other land in the middle of the tile.
pub fn whole(width: u32, height: u32, rgba: &[u8]) -> bool {
    if width != SIZE || height != SIZE || rgba.len() < (SIZE * SIZE * 4) as usize {
        return false;
    }
    let points = SAMPLES.iter().map(|&(_, x, y)| (x, y)).chain([
        (30, 14),
        (30, 30),
        (14, 30),
        (14, 14),
        (22, 22),
    ]);
    let colors: Option<Vec<Rgb>> = points.map(|(x, y)| patch_color(rgba, x, y)).collect();
    let Some(colors) = colors else {
        return false;
    };
    let center = mean(&colors);
    colors.iter().all(|c| distance(*c, center) < 24.0)
}

fn distance(a: Rgb, b: Rgb) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The average of the colors.
pub fn mean(colors: &[Rgb]) -> Rgb {
    let n = colors.len().max(1) as f32;
    let mut out = [0.0; 3];
    for c in colors {
        for i in 0..3 {
            out[i] += c[i] / n;
        }
    }
    out
}

/// The corners of a tile that look more like the inside land than the outside one, and the
/// average error (how much each corner differs from what it looks like).
pub fn mask_of(corners: &[Rgb; 4], inside: Rgb, outside: Rgb) -> (u8, f32) {
    let mut mask = 0;
    let mut error = 0.0;
    for (i, &(bit, _, _)) in SAMPLES.iter().enumerate() {
        let (a, b) = (distance(corners[i], inside), distance(corners[i], outside));
        if a < b {
            mask |= bit;
        }
        error += a.min(b);
    }
    (mask, error / 4.0)
}

/// The borders between two lands: the whole tiles of the inside and, for each mask of inside
/// corners, the tiles that have it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Transitions {
    pub pure: Vec<u16>,
    pub masks: HashMap<u8, Vec<u16>>,
}

/// Groups the candidates (the tiles of both names) by mask. Each one keeps only those that fit
/// almost as well as the best: the same name brings borders with similar lands, which fit
/// worse.
pub fn classify(inside: Rgb, outside: Rgb, candidates: &[(u16, [Rgb; 4])]) -> Transitions {
    let mut groups: HashMap<u8, Vec<(u16, f32)>> = HashMap::new();
    // Too far from both: the border is with a third land.
    let limit = (distance(inside, outside) * 0.35).max(14.0);
    for (graphic, corners) in candidates {
        let (mask, error) = mask_of(corners, inside, outside);
        if mask == 0 || error > limit {
            continue;
        }
        groups.entry(mask).or_default().push((*graphic, error));
    }
    let best = |mut fits: Vec<(u16, f32)>, limit: usize| -> Vec<u16> {
        fits.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let cutoff = fits.first().map_or(0.0, |f| f.1) * 1.6 + 8.0;
        fits.into_iter()
            .filter(|f| f.1 <= cutoff)
            .take(limit)
            .map(|f| f.0)
            .collect()
    };
    let mut out = Transitions::default();
    for (mask, fits) in groups {
        if mask == ALL {
            out.pure = best(fits, 12);
        } else {
            out.masks.insert(mask, best(fits, 6));
        }
    }
    out
}

/// The middles of the diamond's sides, set back inwards, with the two corners each side
/// joins.
const EDGES: [(u8, u32, u32); 4] = [
    (TOP | RIGHT, 30, 14),
    (RIGHT | BOTTOM, 30, 30),
    (BOTTOM | LEFT, 14, 30),
    (LEFT | TOP, 14, 14),
];

/// The shape of a floor piece from its transparency (UOIddler's `alphaFit`): the cut of floor
/// borders runs along a side, so a corner belongs to the core when both sides touching it are
/// opaque. The error measures how far the sides are from opaque or transparent. The art is the
/// 44x44 one with the static at the bottom (UOIddler's `floorAsDiamond`); `None` for what is
/// not a floor.
pub fn floor_mask(width: u32, height: u32, rgba: &[u8]) -> Option<(u8, f32)> {
    if width != SIZE || height > SIZE || rgba.len() < (width * height * 4) as usize {
        return None;
    }
    let top = SIZE - height;
    let opaque_at = |x: u32, y: u32| -> bool {
        if y < top {
            return false;
        }
        let at = (((y - top) * width + x) * 4 + 3) as usize;
        rgba[at] >= 128
    };
    let mut opaque = [false; 4];
    let mut error = 0.0;
    for (i, &(_, cx, cy)) in EDGES.iter().enumerate() {
        let mut count = 0.0;
        let mut on = 0.0;
        for y in cy - 2..=cy + 2 {
            for x in cx - 2..=cx + 2 {
                count += 1.0;
                if opaque_at(x, y) {
                    on += 1.0;
                }
            }
        }
        let level: f32 = on / count;
        opaque[i] = level >= 0.5;
        error += level.min(1.0 - level);
    }
    let mut mask = 0;
    for &(bit, _, _) in &SAMPLES {
        let all = EDGES
            .iter()
            .enumerate()
            .filter(|(_, (corners, _, _))| corners & bit != 0)
            .all(|(i, _)| opaque[i]);
        if all {
            mask |= bit;
        }
    }
    Some((mask, error / 4.0 * 255.0))
}

/// The borders of a floor from the shapes of its pieces (UOIddler's
/// `classifyFloorTransitions`): the statics of the same name grouped by mask, each one keeping
/// only those that fit almost as well as the best.
pub fn classify_floor(candidates: &[(u16, u8, f32)]) -> Transitions {
    let mut groups: Vec<(u8, Vec<(u16, f32)>)> = Vec::new();
    for &(graphic, mask, error) in candidates {
        match groups.iter_mut().find(|(m, _)| *m == mask) {
            Some((_, list)) => list.push((graphic, error)),
            None => groups.push((mask, vec![(graphic, error)])),
        }
    }
    let best = |mut fits: Vec<(u16, f32)>, limit: usize| -> Vec<u16> {
        fits.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let cutoff = fits.first().map_or(0.0, |f| f.1) * 1.6 + 8.0;
        fits.into_iter()
            .filter(|f| f.1 <= cutoff)
            .take(limit)
            .map(|f| f.0)
            .collect()
    };
    let mut out = Transitions::default();
    for (mask, fits) in groups {
        if mask == ALL {
            out.pure = best(fits, 12);
        } else if mask != 0 {
            out.masks.insert(mask, best(fits, 6));
        }
    }
    out
}

/// A stable number per tile, to choose between variations without the same stroke changing
/// look every time.
pub fn roll(x: i32, y: i32) -> usize {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h as usize
}

/// The tile for a mask: one of those that have it; with none, one of the most similar mask
/// that exists, preferring the one with more inside corners (the border does not open a
/// hole). `None` when there is no border at all between the two.
pub fn pick(set: &Transitions, mask: u8, roll: usize) -> Option<u16> {
    let options = |m: u8| -> &[u16] {
        if m == ALL {
            &set.pure
        } else {
            set.masks.get(&m).map_or(&[], |v| v.as_slice())
        }
    };
    let mut list = options(mask);
    if list.is_empty() {
        let mut known: Vec<u8> = set.masks.keys().copied().collect();
        if !set.pure.is_empty() {
            known.push(ALL);
        }
        known.sort_by_key(|m| ((m ^ mask).count_ones(), 4 - m.count_ones(), *m));
        list = options(*known.first()?);
    }
    (!list.is_empty()).then(|| list[roll % list.len()])
}

/// The core and the ring around it: the mask of inside corners of each tile (the core with all
/// of them). A vertex is inside when one of the four tiles touching it is in the core.
pub fn autotile(core: &[(i32, i32)]) -> Vec<((i32, i32), u8)> {
    let inside: HashSet<(i32, i32)> = core.iter().copied().collect();
    let vertex = |vx: i32, vy: i32| {
        [(vx - 1, vy - 1), (vx, vy - 1), (vx - 1, vy), (vx, vy)]
            .iter()
            .any(|t| inside.contains(t))
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for &(x, y) in core {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (tx, ty) = (x + dx, y + dy);
                if tx < 0 || ty < 0 || !seen.insert((tx, ty)) {
                    continue;
                }
                let mask = if inside.contains(&(tx, ty)) {
                    ALL
                } else {
                    let mut m = 0;
                    if vertex(tx, ty) {
                        m |= TOP;
                    }
                    if vertex(tx + 1, ty) {
                        m |= RIGHT;
                    }
                    if vertex(tx + 1, ty + 1) {
                        m |= BOTTOM;
                    }
                    if vertex(tx, ty + 1) {
                        m |= LEFT;
                    }
                    m
                };
                if mask != 0 {
                    out.push(((tx, ty), mask));
                }
            }
        }
    }
    out
}

/// The new state of the tiles of a terrain stroke: each tile of the plan (`plan`, the new land
/// of each one, of the core and of the border ring) gets the land and, with a fixed Z (`z`),
/// the ground height at that Z (like UOIddler's `heightOf`: only the tiles the plan changes;
/// what already had the rolled land stays as it is). Without a fixed Z (auto), each tile keeps
/// its current height. `edit` gives the saved change of a tile; `z_now`, its current ground
/// height (a height that already holds is not saved again).
pub fn painted(
    plan: &[((i32, i32), u16)],
    z: Option<i8>,
    edit: impl Fn(i32, i32) -> MapTile,
    z_now: impl Fn(i32, i32) -> Option<i8>,
) -> Vec<MapTile> {
    let mut out: Vec<MapTile> = Vec::new();
    let mut seen = HashSet::new();
    for &((x, y), graphic) in plan {
        if !seen.insert((x, y)) {
            continue;
        }
        let mut tile = edit(x, y);
        tile.land = Some(graphic);
        out.push(tile);
    }
    let Some(z) = z else {
        return out;
    };
    for tile in &mut out {
        if z_now(tile.x, tile.y) != Some(z) {
            tile.land_z = Some(z);
        }
    }
    out
}

/// The height a tile's ground will have after the terrain stroke in progress, for the
/// preview: the fixed Z on the stroke and, with the automatic border, on the ring of eight
/// around it (the ring tiles that do not change land do not rise, but the preview does not read
/// the art to know which they are); elsewhere, the current one.
pub fn preview_z<'a>(
    stroke: &'a HashSet<(i32, i32)>,
    ring: bool,
    z: i8,
    z_now: impl Fn(i32, i32) -> i8 + 'a,
) -> impl Fn(i32, i32) -> i8 + 'a {
    move |x, y| {
        let near = if ring { 1 } else { 0 };
        let painted =
            (-near..=near).any(|dy| (-near..=near).any(|dx| stroke.contains(&(x + dx, y + dy))));
        if painted {
            z
        } else {
            z_now(x, y)
        }
    }
}

/// What was read from the files to paint terrain, kept: the tiles by name, the corner colors of
/// each one (and whether it is whole), the variation sets and the borders between two lands.
#[derive(Default)]
pub struct Cache {
    names: Option<HashMap<String, Vec<u16>>>,
    corners: HashMap<u16, Option<[Rgb; 4]>>,
    whole: HashSet<u16>,
    vary: HashMap<u16, Vec<u16>>,
    borders: HashMap<(u16, String), Transitions>,
    /// The floor borders per static, and the statics by name.
    floors: HashMap<u16, Transitions>,
    static_names: Option<HashMap<String, Vec<u16>>>,
}

impl Cache {
    /// The name of a land in the tiledata.
    pub fn name(assets: &MapAssets, graphic: u16) -> String {
        assets.land_name(graphic).trim().to_lowercase()
    }

    /// The flags of a land (rock wall, water...).
    fn flags(assets: &MapAssets, graphic: u16) -> u64 {
        assets.land_flags(graphic)
    }

    /// The lands with this name.
    fn named(&mut self, assets: &MapAssets, name: &str) -> Vec<u16> {
        let names = self.names.get_or_insert_with(|| {
            let mut names: HashMap<String, Vec<u16>> = HashMap::new();
            for graphic in 0..0x4000u16 {
                let name = Self::name(assets, graphic);
                if !name.is_empty() {
                    names.entry(name).or_default().push(graphic);
                }
            }
            names
        });
        names.get(name).cloned().unwrap_or_default()
    }

    /// The corner colors of a land.
    pub fn corners(&mut self, assets: &MapAssets, graphic: u16) -> Option<[Rgb; 4]> {
        if let Some(known) = self.corners.get(&graphic) {
            return *known;
        }
        let image = assets.land_image(graphic);
        let colors = image
            .as_ref()
            .and_then(|i| corner_colors(i.width as u32, i.height as u32, &i.rgba));
        if image.is_some_and(|i| whole(i.width as u32, i.height as u32, &i.rgba)) {
            self.whole.insert(graphic);
        }
        self.corners.insert(graphic, colors);
        colors
    }

    /// The tiles to vary with this one. In UO, the variations of a whole land come in a run,
    /// with the same name and the same flags (grass from 0x03 to 0x06, dirt from 0x71 to
    /// 0x78); of those in the run, the whole ones of similar color stay. Always includes the
    /// chosen one.
    pub fn vary(&mut self, assets: &MapAssets, chosen: u16) -> Vec<u16> {
        if let Some(known) = self.vary.get(&chosen) {
            return known.clone();
        }
        let Some(base) = self.corners(assets, chosen).map(|c| mean(&c)) else {
            return vec![chosen];
        };
        let (name, flags) = (Self::name(assets, chosen), Self::flags(assets, chosen));
        let same = |g: u16| Self::name(assets, g) == name && Self::flags(assets, g) == flags;
        let mut run = vec![chosen];
        let mut g = chosen;
        while g > 0 && same(g - 1) && run.len() < 32 {
            g -= 1;
            run.push(g);
        }
        let mut g = chosen;
        while g < 0x3FFF && same(g + 1) && run.len() < 64 {
            g += 1;
            run.push(g);
        }
        run.sort_unstable();
        let mut set = vec![chosen];
        for graphic in run {
            if graphic == chosen {
                continue;
            }
            let Some(c) = self.corners(assets, graphic) else {
                continue;
            };
            if self.whole.contains(&graphic) && distance(mean(&c), base) < 18.0 {
                set.push(graphic);
            }
        }
        self.vary.insert(chosen, set.clone());
        set
    }

    /// The color of a tile as the outside land: its own, when it is whole; otherwise, that of
    /// the variations of the first tile with its name.
    fn outside_color(&mut self, assets: &MapAssets, current: u16, name: &str) -> Option<Rgb> {
        let own = self.corners(assets, current)?;
        if self.whole.contains(&current) {
            return Some(mean(&own));
        }
        for first in self.named(assets, name) {
            if self.corners(assets, first).is_some() && self.whole.contains(&first) {
                let colors: Vec<Rgb> = self
                    .vary(assets, first)
                    .into_iter()
                    .filter_map(|g| self.corners(assets, g).map(|c| mean(&c)))
                    .collect();
                return Some(mean(&colors));
            }
        }
        Some(mean(&own))
    }

    /// The borders between the painted land (`chosen`, inside) and the land of `current`
    /// (outside), read from the art: the tiles of both names, without the rock walls when the
    /// painted one is ground. `None` when the art is missing, or when both have the same name.
    pub fn transitions(
        &mut self,
        assets: &MapAssets,
        chosen: u16,
        current: u16,
    ) -> Option<Transitions> {
        let (inside_name, outside_name) = (Self::name(assets, chosen), Self::name(assets, current));
        if outside_name.is_empty() || inside_name == outside_name {
            return None;
        }
        let key = (chosen, outside_name.clone());
        if let Some(known) = self.borders.get(&key) {
            return Some(known.clone());
        }
        let inside = mean(&self.corners(assets, chosen)?);
        let outside = self.outside_color(assets, current, &outside_name)?;
        let impassable = |g: u16| Self::flags(assets, g) & flags::IMPASSABLE != 0;
        let blocked = impassable(chosen);
        let mut candidates = Vec::new();
        for name in [&inside_name, &outside_name] {
            for graphic in self.named(assets, name) {
                if impassable(graphic) != blocked {
                    continue;
                }
                if let Some(c) = self.corners(assets, graphic) {
                    candidates.push((graphic, c));
                }
            }
        }
        let set = classify(inside, outside, &candidates);
        self.borders.insert(key, set.clone());
        Some(set)
    }

    /// The borders of a floor (static), read from the transparency of the pieces with the same
    /// name. `None` when the static is not a floor 44 pixels wide.
    pub fn floor_transitions(&mut self, assets: &MapAssets, graphic: u16) -> Option<Transitions> {
        if let Some(known) = self.floors.get(&graphic) {
            return Some(known.clone());
        }
        let name = |g: u16| assets.item_name(g).trim().to_lowercase();
        let mine = name(graphic);
        if mine.is_empty() {
            return None;
        }
        let shape = |g: u16| -> Option<(u8, f32)> {
            let i = assets.static_image(g, 0)?;
            floor_mask(i.width as u32, i.height as u32, &i.rgba)
        };
        shape(graphic)?;
        let names = self.static_names.get_or_insert_with(|| {
            let mut names: HashMap<String, Vec<u16>> = HashMap::new();
            for g in 1..=u16::MAX {
                let n = name(g);
                if !n.is_empty() {
                    names.entry(n).or_default().push(g);
                }
            }
            names
        });
        let same = names.get(&mine).cloned().unwrap_or_default();
        let mut candidates = Vec::new();
        for g in same {
            if let Some((mask, error)) = shape(g) {
                candidates.push((g, mask, error));
            }
        }
        let set = classify_floor(&candidates);
        self.floors.insert(graphic, set.clone());
        Some(set)
    }

    /// The borders "with the black" of a land, like those of the cave floor (0x0245) that ends
    /// in the void: the tiles of the same name, with the outside corner black. The cave border
    /// pieces are impassable (the void cannot be walked), so here the flags separate nothing.
    pub fn void_transitions(&mut self, assets: &MapAssets, chosen: u16) -> Option<Transitions> {
        // The empty name is never a land's: it keeps the borders with the black.
        let key = (chosen, String::new());
        if let Some(known) = self.borders.get(&key) {
            return Some(known.clone());
        }
        let inside = mean(&self.corners(assets, chosen)?);
        let name = Self::name(assets, chosen);
        // Each corner near the land or the black: the same name brings tiles of another color
        // (the crystals of the caves), which the average would let through.
        let far = distance(inside, VOID) * 0.45;
        let mut candidates = Vec::new();
        for graphic in self.named(assets, &name) {
            if let Some(c) = self.corners(assets, graphic) {
                if c.iter()
                    .all(|c| distance(*c, inside).min(distance(*c, VOID)) <= far)
                {
                    candidates.push((graphic, c));
                }
            }
        }
        let set = classify(inside, VOID, &candidates);
        self.borders.insert(key, set.clone());
        Some(set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use uo_core::{FileIndex, MapEdits, MapManager, PlacedStatic, TileDataManager};

    const GREEN: Rgb = [40.0, 120.0, 30.0];
    const BROWN: Rgb = [110.0, 80.0, 50.0];

    /// A 44x44 land art with each corner of the given color.
    fn art(corners: [Rgb; 4]) -> Vec<u8> {
        let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
        for y in 0..SIZE {
            for x in 0..SIZE {
                // The corner nearest to the pixel sets its color.
                let nearest = SAMPLES
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, (_, cx, cy))| {
                        (x as i32 - *cx as i32).pow(2) + (y as i32 - *cy as i32).pow(2)
                    })
                    .map(|(i, _)| i)
                    .unwrap();
                let c = corners[nearest];
                let at = ((y * SIZE + x) * 4) as usize;
                rgba[at..at + 4].copy_from_slice(&[c[0] as u8, c[1] as u8, c[2] as u8, 255]);
            }
        }
        rgba
    }

    fn corners_for(mask: u8) -> [Rgb; 4] {
        std::array::from_fn(|i| {
            if mask & SAMPLES[i].0 != 0 {
                GREEN
            } else {
                BROWN
            }
        })
    }

    /// The client files the real-file tests read (`UOFIDDLER_UO_DIR`): the tiledata and the
    /// art, loaded like the app does.
    struct RealFiles {
        maps: MapManager,
        edits: MapEdits,
        art: FileIndex,
        tiledata: TileDataManager,
    }

    impl RealFiles {
        fn open() -> Option<RealFiles> {
            let dir = PathBuf::from(std::env::var_os("UOFIDDLER_UO_DIR")?);
            let file = |name: &str| client_file(&dir, name).unwrap_or_else(|| panic!("{name}"));
            Some(RealFiles {
                maps: MapManager::new(),
                edits: MapEdits::new(),
                art: FileIndex::from_uop(file("artlegacymul.uop"), ".tga", 0x14000).unwrap(),
                tiledata: TileDataManager::from_file(file("tiledata.mul")).unwrap(),
            })
        }

        fn assets(&self) -> MapAssets<'_> {
            MapAssets {
                maps: &self.maps,
                map_id: 0,
                edits: &self.edits,
                art: Some(&self.art),
                textures: None,
                tiledata: Some(&self.tiledata),
                hues: None,
            }
        }
    }

    /// A file of the client folder, by name in any letter case.
    fn client_file(dir: &Path, name: &str) -> Option<PathBuf> {
        std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
    }

    #[test]
    fn the_art_tells_which_corners_are_inside() {
        let colors = corner_colors(SIZE, SIZE, &art(corners_for(TOP | RIGHT))).unwrap();
        assert_eq!(mask_of(&colors, GREEN, BROWN).0, TOP | RIGHT);
        assert!(corner_colors(10, 10, &[0; 400]).is_none(), "not a land");
        // Grass corners with dirt in the middle: not whole.
        let mut striped = art(corners_for(ALL));
        assert!(whole(SIZE, SIZE, &striped));
        for y in 17..27 {
            for x in 17..27 {
                let at = ((y * SIZE + x) * 4) as usize;
                striped[at..at + 3].copy_from_slice(&[110, 80, 50]);
            }
        }
        assert!(!whole(SIZE, SIZE, &striped));
        let corners = corner_colors(SIZE, SIZE, &striped).unwrap();
        assert_eq!(
            mask_of(&corners, GREEN, BROWN).0,
            ALL,
            "the corners are grass"
        );
    }

    #[test]
    fn candidates_group_by_mask_and_the_nearest_fills_the_gaps() {
        let candidates: Vec<(u16, [Rgb; 4])> = vec![
            (3, corners_for(ALL)),
            (4, corners_for(ALL)),
            (10, corners_for(TOP)),
            (11, corners_for(TOP | RIGHT)),
            (12, corners_for(TOP | RIGHT | BOTTOM)),
            (0x71, corners_for(0)),
        ];
        let set = classify(GREEN, BROWN, &candidates);
        assert_eq!(set.pure, [3, 4]);
        assert_eq!(set.masks[&TOP], [10]);
        assert!(
            !set.masks.contains_key(&0),
            "the whole outside is not a border"
        );
        assert_eq!(pick(&set, TOP | RIGHT, 7), Some(11));
        assert_eq!(pick(&set, ALL, 1), Some(4));
        // Without the exact mask, the most similar one, with more inside corners.
        assert_eq!(pick(&set, RIGHT | BOTTOM, 0), Some(12));
        assert_eq!(pick(&Transitions::default(), TOP, 0), None);
    }

    #[test]
    fn a_single_painted_tile_gets_a_ring_of_eight_borders() {
        let tiles = autotile(&[(5, 5)]);
        let mask = |x, y| tiles.iter().find(|(t, _)| *t == (x, y)).map(|(_, m)| *m);
        assert_eq!(tiles.len(), 9);
        assert_eq!(mask(5, 5), Some(ALL));
        // To the east (x+1): the top and left corners touch the core.
        assert_eq!(mask(6, 5), Some(TOP | LEFT));
        // To the north (y-1): the bottom and left ones.
        assert_eq!(mask(5, 4), Some(BOTTOM | LEFT));
        // On the upper left diagonal, only the bottom one.
        assert_eq!(mask(4, 4), Some(BOTTOM));
        assert_eq!(mask(6, 6), Some(TOP));
    }

    /// With the client files (`UOFIDDLER_UO_DIR`): between grass (0x0003) and dirt (0x0071)
    /// there are borders for almost every mask, and the whole grass has variations.
    #[test]
    fn real_grass_has_borders_with_dirt() {
        let Some(files) = RealFiles::open() else {
            return;
        };
        let assets = files.assets();
        let mut cache = Cache::default();
        let vary = cache.vary(&assets, 0x0003);
        assert_eq!(vary, [3, 4, 5, 6], "the grass variations");
        let dirt = cache.vary(&assets, 0x0071);
        assert!(
            dirt.len() > 1 && dirt.iter().all(|g| (0x71..=0x78).contains(g)),
            "{dirt:x?}"
        );
        let set = cache.transitions(&assets, 0x0003, 0x0071).unwrap();
        for mask in 1..ALL {
            assert!(pick(&set, mask, 0).is_some_and(|g| g != 0x0071), "{mask}");
        }
        assert!(set.masks.len() >= 8, "exact masks: {:?}", set.masks.keys());
        // Grass with grass: the same name, no borders.
        assert!(cache.transitions(&assets, 0x0003, 0x0004).is_none());
    }

    /// With the client files: the cave floor (0x053B) has its border pieces by transparency,
    /// like UOIddler's "Cave floor".
    #[test]
    fn real_cave_floor_statics_have_inset_borders() {
        let Some(files) = RealFiles::open() else {
            return;
        };
        let assets = files.assets();
        let mut cache = Cache::default();
        let set = cache.floor_transitions(&assets, 0x053B).unwrap();
        assert!(set.pure.contains(&0x053B), "{:?}", set.pure);
        assert!(set.masks.len() >= 4, "{:?}", set.masks);
        // The side pieces of UOIddler's set come out with the same mask.
        assert!(
            set.masks
                .get(&(TOP | RIGHT))
                .is_some_and(|t| t.contains(&1345)),
            "{set:?}"
        );
    }

    #[test]
    fn floor_pieces_read_their_shape_from_the_transparency() {
        // A 44x44 piece: opaque in the lower half (the two lower sides), transparent above.
        let mut rgba = vec![0u8; 44 * 44 * 4];
        for y in 0..44u32 {
            for x in 0..44u32 {
                if y >= 22 {
                    rgba[((y * 44 + x) * 4 + 3) as usize] = 255;
                }
            }
        }
        let (mask, error) = floor_mask(44, 44, &rgba).unwrap();
        assert_eq!(mask, BOTTOM, "only the bottom corner has both sides opaque");
        assert!(error < 1.0);
        // A lower floor sits at the bottom, and the top is transparent.
        let full = vec![255u8; 44 * 22 * 4];
        assert_eq!(floor_mask(44, 22, &full).unwrap().0, BOTTOM);
        assert!(floor_mask(40, 22, &full).is_none(), "not a floor");
        let set = classify_floor(&[(1, ALL, 0.0), (2, BOTTOM, 1.0), (3, 0, 0.0)]);
        assert_eq!(set.pure, [1]);
        assert_eq!(set.masks[&BOTTOM], [2]);
        assert!(!set.masks.contains_key(&0));
    }

    /// With the client files: the cave floor (0x0245) has the borders with the black (0x024A
    /// to 0x0265) for the tips, the sides and the corners.
    #[test]
    fn real_cave_floor_has_borders_with_the_black() {
        let Some(files) = RealFiles::open() else {
            return;
        };
        let assets = files.assets();
        let mut cache = Cache::default();
        let set = cache.void_transitions(&assets, 0x0245).unwrap();
        assert!(set.pure.contains(&0x0245), "{:x?}", set.pure);
        for mask in [
            TOP,
            RIGHT,
            BOTTOM,
            LEFT,
            TOP | RIGHT,
            RIGHT | BOTTOM,
            BOTTOM | LEFT,
            LEFT | TOP,
            ALL & !TOP,
            ALL & !LEFT,
        ] {
            let tiles = set.masks.get(&mask).cloned().unwrap_or_default();
            assert!(
                !tiles.is_empty() && tiles.iter().all(|g| (0x024A..=0x0265).contains(g)),
                "mask {mask}: {tiles:x?}"
            );
        }
        // The cave border is impassable, but it gets in: the void cannot be walked.
        assert!(set.masks[&RIGHT].contains(&0x024B) || set.masks[&RIGHT].contains(&0x0263));
    }

    /// A tile without a change.
    fn edit(x: i32, y: i32) -> MapTile {
        MapTile::fresh(x, y)
    }

    #[test]
    fn auto_z_paints_only_the_land_and_keeps_the_height() {
        let plan = [((5, 5), 0x0003), ((6, 5), 0x0010)];
        let tiles = painted(&plan, None, edit, |_, _| Some(7));
        assert_eq!(tiles.len(), 2);
        assert!(tiles.iter().all(|t| t.land_z.is_none()), "{tiles:?}");
        assert_eq!(tiles[0].land, Some(0x0003));
        assert_eq!(tiles[1].land, Some(0x0010));
    }

    #[test]
    fn a_fixed_z_sets_the_height_of_what_the_plan_changes() {
        // The core (5,5) and (6,5); (6,5) already had the painted land and is not in the plan:
        // it stays, with the height it has (as in UOIddler). The border (7,5) changes; (4,5),
        // of the ring, stays as it is.
        let plan = [((5, 5), 0x0003), ((7, 5), 0x0010)];
        let saved = |x, y| MapTile {
            add: if (x, y) == (7, 5) {
                vec![PlacedStatic {
                    graphic: 0x0E76,
                    z: 0,
                    hue: 0,
                }]
            } else {
                vec![]
            },
            ..edit(x, y)
        };
        let z_now = |x, _| Some(if x == 6 { 20 } else { 0 });
        let tiles = painted(&plan, Some(20), saved, z_now);
        let at = |x: i32| tiles.iter().find(|t| (t.x, t.y) == (x, 5)).cloned();
        assert_eq!(tiles.len(), 2);
        let core = at(5).unwrap();
        assert_eq!((core.land, core.land_z), (Some(0x0003), Some(20)));
        assert!(at(6).is_none(), "outside the plan, it stays");
        let ring = at(7).unwrap();
        assert_eq!((ring.land, ring.land_z), (Some(0x0010), Some(20)));
        assert_eq!(ring.add.len(), 1, "the rest of the saved tile stays");
        assert!(
            at(4).is_none(),
            "the ring that does not change land does not rise"
        );
        // The same tile twice in the plan counts once.
        let twice = painted(&[((1, 1), 3), ((1, 1), 4)], Some(1), edit, |_, _| None);
        assert_eq!(twice.len(), 1);
    }

    #[test]
    fn the_preview_shows_the_core_and_the_ring_at_the_fixed_z() {
        let stroke: HashSet<(i32, i32)> = [(5, 5)].into();
        let with_ring = preview_z(&stroke, true, 30, |_, _| 2);
        assert_eq!(with_ring(5, 5), 30);
        assert_eq!(with_ring(6, 6), 30, "the ring goes along");
        assert_eq!(with_ring(7, 5), 2);
        let core_only = preview_z(&stroke, false, 30, |_, _| 2);
        assert_eq!((core_only(5, 5), core_only(6, 5)), (30, 2));
    }

    #[test]
    fn rolls_are_stable_and_spread() {
        assert_eq!(roll(10, 20), roll(10, 20));
        let distinct: HashSet<usize> = (0..50).map(|x| roll(x, 7) % 4).collect();
        assert_eq!(distinct.len(), 4);
    }
}
