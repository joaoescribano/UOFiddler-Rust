//! The map editor's brush: the size and the shape, the tiles it covers around the cursor, the
//! line between two points of a drag (so a fast gesture does not skip tiles), the outline of a
//! set of tiles and the tile under the cursor given the ground height. All pure, without the
//! map: the caller passes the height of each tile.

use crate::map::iso::{tile_at, Z_STEP};
use std::collections::HashSet;

/// The brush sizes, in tiles per side.
pub const SIZES: [u8; 7] = [1, 3, 5, 7, 9, 11, 15];

/// The brush shape, in map axes (on screen, the square is a diamond).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    #[default]
    Square,
    Circle,
    Diamond,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Brush {
    /// The side, odd: the cursor sits in the middle.
    pub size: u8,
    pub shape: Shape,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            size: 1,
            shape: Shape::Square,
        }
    }
}

impl Brush {
    /// The tiles covered with the center at `x, y`, row by row.
    pub fn footprint(&self, x: i32, y: i32) -> Vec<(i32, i32)> {
        let r = (self.size.max(1) / 2) as i32;
        let mut out = Vec::new();
        for dy in -r..=r {
            for dx in -r..=r {
                let inside = match self.shape {
                    Shape::Square => true,
                    // Half a tile of slack: the edge of the circle stays full, without loose tips.
                    Shape::Circle => dx * dx + dy * dy <= r * r + r,
                    Shape::Diamond => dx.abs() + dy.abs() <= r,
                };
                if inside {
                    out.push((x + dx, y + dy));
                }
            }
        }
        out
    }
}

/// The tiles from `a` to `b`, both included, without holes (Bresenham).
pub fn line(a: (i32, i32), b: (i32, i32)) -> Vec<(i32, i32)> {
    let (mut x, mut y) = a;
    let dx = (b.0 - a.0).abs();
    let dy = -(b.1 - a.1).abs();
    let sx = if a.0 < b.0 { 1 } else { -1 };
    let sy = if a.1 < b.1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut out = vec![(x, y)];
    while (x, y) != b {
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
        out.push((x, y));
    }
    out
}

/// The sides of a tile's diamond, as bits: the upper right one (shared with `x, y-1`), the
/// lower right (`x+1, y`), the lower left (`x, y+1`) and the upper left (`x-1, y`).
pub const NE: u8 = 1;
pub const SE: u8 = 2;
pub const SW: u8 = 4;
pub const NW: u8 = 8;

/// The sides of each tile that lie on the edge of the set (the neighbour on that side is not
/// in it): the outline to draw.
pub fn outline(tiles: &[(i32, i32)]) -> Vec<((i32, i32), u8)> {
    let set: HashSet<(i32, i32)> = tiles.iter().copied().collect();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for &(x, y) in tiles {
        if !seen.insert((x, y)) {
            continue;
        }
        let mut sides = 0;
        for (bit, n) in [
            (NE, (x, y - 1)),
            (SE, (x + 1, y)),
            (SW, (x, y + 1)),
            (NW, (x - 1, y)),
        ] {
            if !set.contains(&n) {
                sides |= bit;
            }
        }
        if sides != 0 {
            out.push(((x, y), sides));
        }
    }
    out
}

/// The tile under a point in world pixels. The ground has height: start from a guessed height
/// and correct by the height of the ground found, until it settles.
pub fn pick(wx: f32, wy: f32, guess_z: i32, land_z: impl Fn(i32, i32) -> Option<i8>) -> (i32, i32) {
    let mut z = guess_z;
    let mut at = tile_at(wx, wy + z as f32 * Z_STEP);
    for _ in 0..4 {
        let Some(found) = land_z(at.0, at.1).map(i32::from) else {
            break;
        };
        if found == z {
            break;
        }
        z = found;
        at = tile_at(wx, wy + z as f32 * Z_STEP);
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::iso::{tile_origin, TILE_HALF};

    #[test]
    fn footprints_follow_size_and_shape() {
        let square = Brush {
            size: 3,
            shape: Shape::Square,
        };
        assert_eq!(square.footprint(10, 10).len(), 9);
        assert_eq!(Brush::default().footprint(4, 5), [(4, 5)]);
        let diamond = Brush {
            size: 5,
            shape: Shape::Diamond,
        };
        // 1 + 4 + 8: the diamond of radius 2.
        assert_eq!(diamond.footprint(0, 0).len(), 13);
        assert!(!diamond.footprint(0, 0).contains(&(2, 1)));
        let circle = Brush {
            size: 7,
            shape: Shape::Circle,
        };
        let disc = circle.footprint(0, 0);
        assert!(disc.contains(&(3, 0)) && disc.contains(&(2, 2)));
        assert!(!disc.contains(&(3, 3)), "the corners stay out");
        assert!(disc.len() < 49 && disc.len() > 25);
    }

    #[test]
    fn a_fast_drag_leaves_no_holes() {
        let path = line((0, 0), (5, 2));
        assert_eq!(path.first(), Some(&(0, 0)));
        assert_eq!(path.last(), Some(&(5, 2)));
        for pair in path.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            assert!((a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1);
        }
        assert_eq!(line((3, 3), (3, 3)), [(3, 3)]);
        assert_eq!(line((2, 0), (0, 0)), [(2, 0), (1, 0), (0, 0)]);
    }

    #[test]
    fn the_outline_keeps_only_the_outer_sides() {
        assert_eq!(outline(&[(0, 0)]), [((0, 0), NE | SE | SW | NW)]);
        let block = Brush {
            size: 3,
            shape: Shape::Square,
        }
        .footprint(0, 0);
        let edges = outline(&block);
        assert_eq!(edges.len(), 8, "the middle has no outer side");
        let corner = edges.iter().find(|(t, _)| *t == (-1, -1)).unwrap();
        assert_eq!(corner.1, NE | NW);
    }

    #[test]
    fn picking_corrects_for_the_ground_height() {
        // The point in the middle of the diamond of tile 10,10, with the ground at 20.
        let (ox, oy) = tile_origin(10, 10);
        let (wx, wy) = (ox + TILE_HALF, oy + TILE_HALF - 20.0 * Z_STEP);
        assert_eq!(pick(wx, wy, 20, |_, _| Some(20)), (10, 10));
        // The guess is the ground at 0: the first try misses, the height corrects it.
        assert_eq!(pick(wx, wy, 0, |_, _| Some(20)), (10, 10));
        // Without the block loaded, the first try stays.
        assert_ne!(pick(wx, wy, 0, |_, _| None), (10, 10));
    }
}
