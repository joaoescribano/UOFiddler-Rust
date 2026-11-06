//! The border sets: for each corner shape (the mask of the corners that belong to the core),
//! the tiles that fit, chosen one by one in the border dialog. The painting is UOIddler map
//! editor's, exactly, and the tests check the whole output against that of its code
//! (`uoiddler_cases.txt`):
//!
//! - **Land** (`plan`, UOIddler's `planLandBorder`): the stroke is the core and the ring
//!   outside gets the pieces of the set. The core also counts what was already whole around
//!   it, each ring tile adds the corners of the piece it already is, and a ring tile that ends
//!   up with all four corners keeps what it had.
//! - **Floor** (`floor_plan`, UOIddler's `planFloorBorder`): statics at the stroke's height,
//!   with the borders **inside** it (`inset_masks`): a corner belongs to the core only when
//!   the four tiles touching it are floor. No outer ring.
//! - Without the automatic border, the floor only goes where there is none yet
//!   (`static_plan`).
//!
//! The borders come from a chosen set or from the art: with the most common land around the
//! stroke (`common_outside`) or with the black of the void.

use super::terrain::{self, Transitions, ALL, BOTTOM, LEFT, RIGHT, TOP};
use super::types::BorderSet;
use std::collections::{HashMap, HashSet};

/// Where the borders of the automatic border come from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Source {
    /// Read from the art, with the most common land around the stroke.
    #[default]
    Detect,
    /// Read from the art, with the black of the void around it (caves).
    Void,
    /// A saved set, by name.
    Set(String),
}

/// The corner shapes, in the dialog's order: the full center, the tips, the sides, the inner
/// corners and the diagonals. The 15 masks, each one once. The first field is MacroOnline's
/// translation key of the group; `group_label` gives its English name.
pub const GROUPS: [(&str, &[u8]); 5] = [
    ("admin.borders.full", &[ALL]),
    ("admin.borders.tips", &[TOP, RIGHT, BOTTOM, LEFT]),
    (
        "admin.borders.sides",
        &[TOP | RIGHT, RIGHT | BOTTOM, BOTTOM | LEFT, LEFT | TOP],
    ),
    (
        "admin.borders.inner",
        &[ALL & !TOP, ALL & !RIGHT, ALL & !BOTTOM, ALL & !LEFT],
    ),
    ("admin.borders.diagonals", &[TOP | BOTTOM, RIGHT | LEFT]),
];

/// The English name of a group of `GROUPS`, by its key (what MacroOnline's English table
/// gives); the key itself when it is not one of them.
pub fn group_label(key: &str) -> &str {
    match key {
        "admin.borders.full" => "Full center",
        "admin.borders.tips" => "Tips",
        "admin.borders.sides" => "Sides",
        "admin.borders.inner" => "Inner corners",
        "admin.borders.diagonals" => "Diagonals",
        other => other,
    }
}

/// A saved set in the brush's format (UOIddler's `toTransitionSet`): the full center and the
/// borders, without the empty shapes.
pub fn transitions(set: &BorderSet) -> Transitions {
    Transitions {
        pure: set.tiles(ALL).to_vec(),
        masks: set
            .masks
            .iter()
            .filter(|(mask, tiles)| **mask != ALL && !tiles.is_empty())
            .map(|(mask, tiles)| (*mask, tiles.clone()))
            .collect(),
    }
}

/// A draft read from the art, in the saved format.
pub fn from_transitions(found: &Transitions) -> std::collections::BTreeMap<u8, Vec<u16>> {
    let mut masks: std::collections::BTreeMap<u8, Vec<u16>> = found
        .masks
        .iter()
        .filter(|(_, tiles)| !tiles.is_empty())
        .map(|(mask, tiles)| (*mask, tiles.clone()))
        .collect();
    if !found.pure.is_empty() {
        masks.insert(ALL, found.pure.clone());
    }
    masks
}

/// The mask of each piece of the set (UOIddler's `graphicMasks`): going through the masks in
/// increasing order, the last one that has the piece wins.
fn piece_masks(set: &Transitions) -> HashMap<u16, u8> {
    let mut keys: Vec<u8> = set.masks.keys().copied().collect();
    keys.sort_unstable();
    let mut out = HashMap::new();
    for mask in keys {
        for graphic in &set.masks[&mask] {
            out.insert(*graphic, mask);
        }
    }
    out
}

/// The border pieces of a set: the tiles of every shape but the full center.
pub fn pieces(set: &Transitions) -> impl Iterator<Item = u16> + '_ {
    set.masks
        .iter()
        .filter(|(mask, _)| **mask != ALL)
        .flat_map(|(_, tiles)| tiles.iter().copied())
}

/// The draw for the land core: the brush and, with Vary on, the whole tiles of the chosen
/// saved set (`chosen`: more than one in the full shape becomes variation, as the dialog says)
/// and the variations read from the art (`found`, `Cache::vary`), **minus the border pieces**
/// (`pieces`). The art alone does not separate them: the furrow (0x0009) has the same noisy
/// texture as its pieces (0x000A-0x0015), and `whole` accepted sides and tips with grass; in
/// the core, they opened grass holes in the middle of the path.
pub fn land_pool(
    brush: u16,
    vary: bool,
    chosen: &[u16],
    found: &[u16],
    pieces: &HashSet<u16>,
) -> Vec<u16> {
    let mut pool = vec![brush];
    if !vary {
        return pool;
    }
    for &graphic in chosen.iter().chain(found) {
        let whole = chosen.contains(&graphic) || !pieces.contains(&graphic);
        if whole && !pool.contains(&graphic) {
            pool.push(graphic);
        }
    }
    pool
}

/// The stroke plus two tiles around it (UOIddler's `aroundStroke`), in the order UOIddler
/// finds them, without negatives.
pub fn around_stroke(stroke: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for &(x, y) in stroke {
        for dy in -2..=2 {
            for dx in -2..=2 {
                let t = (x + dx, y + dy);
                if t.0 >= 0 && t.1 >= 0 && seen.insert(t) {
                    out.push(t);
                }
            }
        }
    }
    out
}

/// The tiles of the stroke and those touching it, diagonally too.
fn touching(stroke: &[(i32, i32)]) -> HashSet<(i32, i32)> {
    stroke
        .iter()
        .flat_map(|&(x, y)| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dx| (x + dx, y + dy))))
        .collect()
}

/// The outside land to detect the borders with (UOIddler's `commonOutside`): the most common
/// one around the stroke, other than the brush itself; on a tie, the first one found. `None`
/// with none (the borders go to the black).
pub fn common_outside(
    stroke: &[(i32, i32)],
    land: impl Fn(i32, i32) -> Option<u16>,
    brush: u16,
) -> Option<u16> {
    let painted: HashSet<(i32, i32)> = stroke.iter().copied().collect();
    let mut counts: Vec<(u16, usize)> = Vec::new();
    for (x, y) in around_stroke(stroke) {
        if painted.contains(&(x, y)) {
            continue;
        }
        let Some(graphic) = land(x, y).filter(|g| *g != brush) else {
            continue;
        };
        match counts.iter_mut().find(|(g, _)| *g == graphic) {
            Some((_, n)) => *n += 1,
            None => counts.push((graphic, 1)),
        }
    }
    // Stable sort: on a tie, the one that showed up first stays.
    counts.sort_by_key(|a| std::cmp::Reverse(a.1));
    counts.first().map(|(g, _)| *g)
}

/// The land with borders (UOIddler's `planLandBorder`): the stroke (`stroke`) gets the core
/// (`core`, per tile) and the ring around it, the piece of its mask of inside corners. `inside`
/// are the lands that count as core (the brush, the set's whole tiles and those of the draw);
/// `land` tells the current land of a tile (`None` when the block did not load); `roll`
/// chooses between the pieces of one shape. Returns only what changes.
pub fn plan(
    stroke: &[(i32, i32)],
    set: &Transitions,
    inside: &HashSet<u16>,
    land: impl Fn(i32, i32) -> Option<u16>,
    core: impl Fn(i32, i32) -> u16,
    roll: impl Fn(i32, i32) -> usize,
) -> Vec<((i32, i32), u16)> {
    let painted: HashSet<(i32, i32)> = stroke.iter().copied().collect();
    let piece = piece_masks(set);
    // The inside corners a tile already has: whole, the piece it is, or none.
    let had = |graphic: u16| -> u8 {
        if inside.contains(&graphic) {
            ALL
        } else {
            piece.get(&graphic).copied().unwrap_or(0)
        }
    };
    let around = around_stroke(stroke);
    // The core is the stroke plus what was already of this land around it, to join up.
    let core_tiles: Vec<(i32, i32)> = around
        .iter()
        .copied()
        .filter(|&(x, y)| painted.contains(&(x, y)) || land(x, y).is_some_and(|g| had(g) == ALL))
        .collect();
    let near = touching(stroke);
    let mut out = Vec::new();
    for ((x, y), mask) in terrain::autotile(&core_tiles) {
        if !painted.contains(&(x, y)) && !near.contains(&(x, y)) {
            continue;
        }
        let current = land(x, y);
        let graphic = if painted.contains(&(x, y)) {
            core(x, y)
        } else {
            let Some(current) = current else {
                continue;
            };
            let merged = mask | had(current);
            if merged == ALL {
                // Outside the stroke, what ended up whole keeps what it had.
                continue;
            }
            let Some(graphic) = terrain::pick(set, merged, roll(x, y)) else {
                continue;
            };
            graphic
        };
        if current != Some(graphic) {
            out.push(((x, y), graphic));
        }
    }
    out
}

/// The shape of each tile of a floor area, with the borders inside (UOIddler's
/// `insetMasks`): a corner belongs to the core only when the four tiles touching it are in the
/// area.
pub fn inset_masks(area: &HashSet<(i32, i32)>, tiles: &[(i32, i32)]) -> Vec<((i32, i32), u8)> {
    let has = |x: i32, y: i32| area.contains(&(x, y));
    let vertex =
        |vx: i32, vy: i32| has(vx - 1, vy - 1) && has(vx, vy - 1) && has(vx - 1, vy) && has(vx, vy);
    tiles
        .iter()
        .map(|&(x, y)| {
            let mut mask = 0;
            if vertex(x, y) {
                mask |= TOP;
            }
            if vertex(x + 1, y) {
                mask |= RIGHT;
            }
            if vertex(x + 1, y + 1) {
                mask |= BOTTOM;
            }
            if vertex(x, y + 1) {
                mask |= LEFT;
            }
            ((x, y), mask)
        })
        .collect()
}

/// The floor piece for a shape (UOIddler's `pickFloorPiece`): without the exact one, the one
/// that covers every core corner with the least excess, so the area never gets a hole; only
/// with none like that, the most similar one.
pub fn pick_floor_piece(set: &Transitions, mask: u8, roll: usize) -> Option<u16> {
    let options = |m: u8| -> &[u16] {
        if m == ALL {
            &set.pure
        } else {
            set.masks.get(&m).map_or(&[], |v| v.as_slice())
        }
    };
    let mut keys: Vec<u8> = set.masks.keys().copied().collect();
    keys.sort_unstable();
    let available: Vec<u8> = std::iter::once(ALL)
        .chain(keys)
        .filter(|m| !options(*m).is_empty())
        .collect();
    if available.is_empty() {
        return None;
    }
    let covering: Vec<u8> = available
        .iter()
        .copied()
        .filter(|c| c & mask == mask)
        .collect();
    let mut ranked = if covering.is_empty() {
        available
    } else {
        covering
    };
    // Stable: on a tie, the order from before (the full center, then the masks).
    ranked.sort_by_key(|m| ((m ^ mask).count_ones(), 4 - m.count_ones()));
    let list = options(ranked[0]);
    Some(list[roll % list.len()])
}

/// A floor change on a tile: the floor statics that go and the one that comes in, all at the
/// stroke's height.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloorEdit {
    pub at: (i32, i32),
    pub remove: Vec<u16>,
    pub add: u16,
}

/// The floor with borders (UOIddler's `planFloorBorder`): the pieces go on the edge of the
/// stroke itself, with the cut facing out, all at its height `z`. The neighbouring floor of the
/// same kind at the same height counts, and what already has the right piece stays. `statics`
/// gives the graphics of a tile's statics at height `z`.
pub fn floor_plan(
    stroke: &[(i32, i32)],
    set: &Transitions,
    brush: u16,
    vary: bool,
    statics: impl Fn(i32, i32) -> Vec<u16>,
    roll: impl Fn(i32, i32) -> usize,
) -> Vec<FloorEdit> {
    let painted: HashSet<(i32, i32)> = stroke.iter().copied().collect();
    let whole: HashSet<u16> = std::iter::once(brush)
        .chain(set.pure.iter().copied())
        .collect();
    let pool: Vec<u16> = if vary && !set.pure.is_empty() {
        set.pure.clone()
    } else {
        vec![brush]
    };
    let piece = piece_masks(set);
    let family = |g: u16| whole.contains(&g) || piece.contains_key(&g);
    let floor_here =
        |x: i32, y: i32| -> Vec<u16> { statics(x, y).into_iter().filter(|g| family(*g)).collect() };
    let around = around_stroke(stroke);
    let area: HashSet<(i32, i32)> = around
        .iter()
        .copied()
        .filter(|&(x, y)| painted.contains(&(x, y)) || !floor_here(x, y).is_empty())
        .collect();
    let near = touching(stroke);
    let targets: Vec<(i32, i32)> = around
        .iter()
        .copied()
        .filter(|t| area.contains(t) && near.contains(t))
        .collect();
    let mut out = Vec::new();
    for ((x, y), mask) in inset_masks(&area, &targets) {
        let current = floor_here(x, y);
        let fits = |g: u16| {
            if mask == ALL {
                whole.contains(&g)
            } else {
                piece.get(&g) == Some(&mask)
            }
        };
        if current.len() == 1 && fits(current[0]) {
            continue;
        }
        let graphic = if mask == ALL {
            pool[roll(x, y) % pool.len()]
        } else {
            let Some(graphic) = pick_floor_piece(set, mask, roll(x, y)) else {
                continue;
            };
            graphic
        };
        out.push(FloorEdit {
            at: (x, y),
            remove: current,
            add: graphic,
        });
    }
    out
}

/// The floor without borders (UOIddler's `planStaticPaint`): each tile of the stroke gets one
/// of the variations, except those that already have one of them (or the brush) at the
/// stroke's height.
pub fn static_plan(
    stroke: &[(i32, i32)],
    brush: u16,
    variants: &[u16],
    statics: impl Fn(i32, i32) -> Vec<u16>,
    roll: impl Fn(i32, i32) -> usize,
) -> Vec<((i32, i32), u16)> {
    stroke
        .iter()
        .filter(|&&(x, y)| {
            !statics(x, y)
                .iter()
                .any(|g| *g == brush || variants.contains(g))
        })
        .map(|&(x, y)| ((x, y), variants[roll(x, y) % variants.len()]))
        .collect()
}

/// The preview of a set: one core tile and the ring of eight around it, with the pieces the
/// brush would put (without the one of a missing shape).
pub fn preview(set: &Transitions, core: u16) -> Vec<((i32, i32), u16)> {
    terrain::autotile(&[(1, 1)])
        .into_iter()
        .filter_map(|(at, mask)| {
            let graphic = if mask == ALL {
                core
            } else {
                terrain::pick(set, mask, 0)?
            };
            Some((at, graphic))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::brush::{Brush, Shape};
    use super::super::types::seed_border_sets;
    use super::*;
    use std::collections::BTreeMap;

    const GRASS: u16 = 3;

    /// UOIddler's output for the cases below: its `planLandBorder` and `planFloorBorder` run
    /// in node, with the sets from its database and the draw always on the first.
    const UOIDDLER: &str = include_str!("uoiddler_cases.txt");

    fn seeded(name: &str) -> BorderSet {
        seed_border_sets()
            .into_iter()
            .find(|s| s.name == name)
            .unwrap()
    }

    /// The cave-floor with the inner corners emptied, as the owner did.
    fn emptied() -> BorderSet {
        let mut set = seeded("cave-floor");
        for mask in [7, 11, 13, 14] {
            set.masks.insert(mask, vec![]);
        }
        set
    }

    fn square(x: i32, y: i32, size: u8) -> Vec<(i32, i32)> {
        Brush {
            size,
            shape: Shape::Square,
        }
        .footprint(x, y)
    }

    const L: [(i32, i32); 5] = [(10, 10), (11, 10), (12, 10), (10, 11), (10, 12)];

    /// A test world: the land of each tile (grass by default) and the statics (graphic and
    /// height).
    #[derive(Default)]
    struct World {
        land: HashMap<(i32, i32), u16>,
        statics: HashMap<(i32, i32), Vec<(u16, i8)>>,
    }

    impl World {
        fn land_at(&self, x: i32, y: i32) -> u16 {
            *self.land.get(&(x, y)).unwrap_or(&GRASS)
        }

        fn paint_land(&mut self, set: &BorderSet, stroke: &[(i32, i32)]) {
            let t = transitions(set);
            let brush = t.pure[0];
            let inside: HashSet<u16> = std::iter::once(brush).chain(t.pure.clone()).collect();
            let out = plan(
                stroke,
                &t,
                &inside,
                |x, y| Some(self.land_at(x, y)),
                |_, _| brush,
                |_, _| 0,
            );
            self.land.extend(out);
        }

        fn at_z(&self, x: i32, y: i32, z: i8) -> Vec<u16> {
            self.statics
                .get(&(x, y))
                .map(|list| list.iter().filter(|s| s.1 == z).map(|s| s.0).collect())
                .unwrap_or_default()
        }

        fn paint_floor(&mut self, set: &BorderSet, stroke: &[(i32, i32)], z: i8, vary: bool) {
            let t = transitions(set);
            let edits = floor_plan(
                stroke,
                &t,
                t.pure[0],
                vary,
                |x, y| self.at_z(x, y, z),
                |_, _| 0,
            );
            for edit in edits {
                let list = self.statics.entry(edit.at).or_default();
                for gone in edit.remove {
                    if let Some(at) = list.iter().position(|s| *s == (gone, z)) {
                        list.remove(at);
                    }
                }
                list.push((edit.add, z));
            }
        }

        fn paint_plain(&mut self, stroke: &[(i32, i32)], brush: u16, variants: &[u16]) {
            let out = static_plan(stroke, brush, variants, |x, y| self.at_z(x, y, 0), |_, _| 0);
            for (at, graphic) in out {
                self.statics.entry(at).or_default().push((graphic, 0));
            }
        }

        /// In the fixture's format: `x,y=land+static@z`, by y and x.
        fn dump(&self) -> String {
            let mut cells: BTreeMap<(i32, i32), Vec<String>> = BTreeMap::new();
            for (&(x, y), &g) in &self.land {
                if g != GRASS {
                    cells.insert((y, x), vec![g.to_string()]);
                }
            }
            for (&(x, y), list) in &self.statics {
                if list.is_empty() {
                    continue;
                }
                let parts = cells.entry((y, x)).or_insert_with(|| vec!["-".into()]);
                parts.extend(list.iter().map(|(g, z)| format!("{g}@{z}")));
            }
            cells
                .into_iter()
                .map(|((y, x), parts)| format!("{x},{y}={}", parts.join("+")))
                .collect::<Vec<_>>()
                .join(" ")
        }
    }

    /// The cases, with the same strokes as UOIddler's harness.
    fn run(name: &str) -> World {
        let cave = seeded("cave-floor");
        let floor = seeded("piso-de-caverna");
        let mut w = World::default();
        match name {
            "land_sq1" => w.paint_land(&cave, &square(10, 10, 1)),
            "land_sq3" => w.paint_land(&cave, &square(10, 10, 3)),
            "land_sq5" => w.paint_land(&cave, &square(10, 10, 5)),
            "land_l" => w.paint_land(&cave, &L),
            "land_overlap" => {
                w.paint_land(&cave, &square(10, 10, 3));
                w.paint_land(&cave, &square(12, 11, 3));
            }
            "land_edge" => {
                w.paint_land(&cave, &square(10, 10, 3));
                w.paint_land(&cave, &[(13, 10)]);
            }
            "land_existing" => {
                for (x, y, g) in [(12, 9, 592), (12, 10, 592), (12, 11, 587), (11, 11, 590)] {
                    w.land.insert((x, y), g);
                }
                w.land.insert((11, 10), 581);
                w.paint_land(&cave, &square(9, 10, 3));
            }
            "land_emptied_l" => w.paint_land(&emptied(), &L),
            "land_emptied_sq5_diag" => {
                w.paint_land(&emptied(), &square(10, 10, 3));
                w.paint_land(&emptied(), &square(13, 13, 3));
            }
            "land_furrow_l" => w.paint_land(&seeded("furrow-grass"), &L),
            "floor_sq1" => w.paint_floor(&floor, &square(10, 10, 1), 0, false),
            "floor_sq3" => w.paint_floor(&floor, &square(10, 10, 3), 0, false),
            "floor_sq5" => w.paint_floor(&floor, &square(10, 10, 5), 0, false),
            "floor_l" => w.paint_floor(&floor, &L, 0, false),
            "floor_overlap" => {
                w.paint_floor(&floor, &square(10, 10, 3), 0, false);
                w.paint_floor(&floor, &square(12, 11, 3), 0, false);
            }
            "floor_next" => {
                w.paint_floor(&floor, &square(14, 10, 3), 0, false);
                w.paint_floor(&floor, &square(10, 20, 3), 5, false);
                w.paint_floor(&floor, &square(11, 10, 3), 0, false);
                w.paint_floor(&floor, &square(11, 19, 3), 0, false);
            }
            "floor_vary" => w.paint_floor(&floor, &square(10, 10, 3), 0, true),
            "floor_plain" => {
                w.statics.insert((10, 10), vec![(1340, 0)]);
                w.paint_plain(&square(10, 10, 3), 1339, &[1339, 1340]);
            }
            other => panic!("case without a stroke: {other}"),
        }
        w
    }

    #[test]
    fn painting_matches_uoiddler_tile_for_tile() {
        let mut count = 0;
        for line in UOIDDLER.lines().filter(|l| !l.trim().is_empty()) {
            let (name, expected) = line.split_once(' ').unwrap_or((line, ""));
            assert_eq!(run(name).dump(), expected, "case {name}");
            count += 1;
        }
        assert_eq!(count, 18, "the fixture's cases");
    }

    /// Vary on the furrow read the pieces 0x000A-0x0015 from the art as whole (the texture is
    /// noisy) and rolled them in the core: grass in the middle of the path. A set's pieces
    /// never get into the draw; the whole tiles of the chosen set do.
    #[test]
    fn the_core_never_rolls_a_border_piece() {
        let furrow = transitions(&seeded("furrow-grass"));
        let pieces: HashSet<u16> = super::pieces(&furrow).collect();
        assert_eq!(pieces.len(), 12, "0x000A to 0x0015");
        let found = [9, 10, 11, 12, 13, 14, 15, 18, 19, 20, 21];
        assert_eq!(land_pool(9, true, &[9], &found, &pieces), [9]);
        assert_eq!(land_pool(9, false, &[9], &found, &pieces), [9]);
        // Grass has real variations, and none is a piece.
        assert_eq!(
            land_pool(3, true, &[], &[3, 4, 5, 6], &pieces),
            [3, 4, 5, 6]
        );
        // The cave-floor's whole tiles are the variations, even the centers with a black
        // corner (1595-1598).
        let cave = seeded("cave-floor");
        let cave_pieces: HashSet<u16> = super::pieces(&transitions(&cave)).collect();
        let pool = land_pool(584, true, cave.tiles(ALL), &[584, 586], &cave_pieces);
        assert_eq!(pool, [584, 582, 583, 581, 585, 1597, 1596, 1598, 1595]);
    }

    #[test]
    fn the_groups_cover_every_mask_once() {
        let mut all: Vec<u8> = GROUPS.iter().flat_map(|(_, m)| m.iter().copied()).collect();
        all.sort_unstable();
        assert_eq!(all, (1..=ALL).collect::<Vec<_>>());
        assert!(GROUPS.iter().all(|(key, _)| group_label(key) != *key));
    }

    #[test]
    fn a_saved_set_becomes_transitions_and_back() {
        let set = seeded("cave-floor");
        let t = transitions(&set);
        assert_eq!(t.pure[0], 584);
        assert_eq!(t.masks.len(), 12);
        assert_eq!(from_transitions(&t), set.masks);
        assert!(
            !transitions(&emptied()).masks.contains_key(&7),
            "empty does not count"
        );
    }

    #[test]
    fn the_fallback_prefers_the_nearest_mask_with_more_center() {
        let t = transitions(&emptied());
        // Without the inner corner 14, the most similar one with more center: the whole one.
        assert_eq!(terrain::pick(&t, ALL & !TOP, 0), Some(584));
        // The floor prefers the piece that covers the core corners.
        let floor = transitions(&seeded("piso-de-caverna"));
        assert_eq!(
            pick_floor_piece(&floor, BOTTOM, 0),
            Some(1353),
            "the exact one"
        );
        assert_eq!(
            pick_floor_piece(&floor, TOP | BOTTOM, 0),
            Some(1339),
            "only the whole one covers"
        );
        assert_eq!(
            pick_floor_piece(&floor, 0, 0),
            Some(1361),
            "the one with fewest corners"
        );
        assert_eq!(pick_floor_piece(&Transitions::default(), 3, 0), None);
    }

    #[test]
    fn the_outside_land_is_the_most_common_around() {
        let land = |x: i32, _y: i32| Some(if x > 10 { 5 } else { GRASS });
        assert_eq!(common_outside(&[(10, 10)], land, 0x0245), Some(GRASS));
        assert_eq!(
            common_outside(&[(10, 10)], |_, _| Some(0x0245), 0x0245),
            None
        );
    }

    #[test]
    fn the_preview_is_one_center_and_its_ring() {
        let t = transitions(&seeded("cave-floor"));
        let tiles = preview(&t, 584);
        assert_eq!(tiles.len(), 9);
        assert!(tiles.contains(&((1, 1), 584)));
        assert_eq!(preview(&Transitions::default(), 584), [((1, 1), 584)]);
    }
}
