//! The isometric projection of the classic client: 44x44 diamonds, 4 pixels per unit of height,
//! and the draw order of land and statics.

pub const TILE_HALF: f32 = 22.0;
pub const TILE_SIZE: f32 = 44.0;
pub const Z_STEP: f32 = 4.0;

/// Tiledata flags used by the renderer and the editor.
pub mod flags {
    pub const BACKGROUND: u64 = 0x1;
    pub const WALL: u64 = 0x10;
    pub const IMPASSABLE: u64 = 0x40;
    pub const WET: u64 = 0x80;
    pub const SURFACE: u64 = 0x200;
    pub const BRIDGE: u64 = 0x400;
    pub const FOLIAGE: u64 = 0x20000;
    pub const PARTIAL_HUE: u64 = 0x40000;
    pub const ANIMATION: u64 = 0x100_0000;
    pub const ROOF: u64 = 0x1000_0000;
    pub const DOOR: u64 = 0x2000_0000;
    pub const STAIR_BACK: u64 = 0x4000_0000;
    pub const STAIR_RIGHT: u64 = 0x8000_0000;
    pub const MULTI_MOVABLE: u64 = 0x100 << 32;
}

/// Top-left corner of a tile's land art, in world pixels.
pub fn tile_origin(x: i32, y: i32) -> (f32, f32) {
    ((x - y) as f32 * TILE_HALF, (x + y) as f32 * TILE_HALF)
}

/// The tile under a point in world pixels, at height zero.
pub fn tile_at(px: f32, py: f32) -> (i32, i32) {
    let a = (px - TILE_HALF) / TILE_HALF;
    let b = (py - TILE_HALF) / TILE_HALF;
    (
        ((a + b) / 2.0).round() as i32,
        ((b - a) / 2.0).round() as i32,
    )
}

/// The sort key: the diagonal first, the priority breaking ties in hundredths.
pub fn depth(x: i32, y: i32, priority: i32) -> f32 {
    (x + y) as f32 + (127 + priority) as f32 * 0.01
}

/// The priority of a static on its tile, as in the client: its z, one less for background
/// pieces, one more for what has height and for what moves with a house.
pub fn static_priority(z: i32, tile_flags: u64, height: u8) -> i32 {
    let mut priority = z;
    if tile_flags & flags::BACKGROUND != 0 {
        priority -= 1;
    }
    if height != 0 {
        priority += 1;
    }
    if tile_flags & flags::MULTI_MOVABLE != 0 {
        priority += 1;
    }
    priority
}

pub fn static_depth(x: i32, y: i32, z: i32, tile_flags: u64, height: u8) -> f32 {
    depth(x, y, static_priority(z, tile_flags, height))
}

/// The height the client uses to sort a sloped land tile: the average of its corners, or the
/// lower diagonal pair when the tile folds.
pub fn average_land_z(corners: [i32; 4]) -> i32 {
    let [top, right, bottom, left] = corners;
    if (top - bottom).abs() <= (left - right).abs() {
        (top + bottom).div_euclid(2)
    } else {
        (left + right).div_euclid(2)
    }
}

/// Client markers that are never drawn.
pub fn is_drawable_static(graphic: u16, tile_flags: u64) -> bool {
    match graphic {
        0x0001 | 0x21BC | 0xA1FE..=0xA201 => false,
        0x2198..=0x21A4 => false,
        0x63D3 => true,
        0x9E4C | 0x9E64 | 0x9E65 | 0x9E7D => tile_flags & (flags::BACKGROUND | flags::SURFACE) == 0,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_at_inverts_origin() {
        for (x, y) in [(0, 0), (10, 3), (1480, 1620), (7, 7)] {
            let (px, py) = tile_origin(x, y);
            assert_eq!(tile_at(px + TILE_HALF, py + TILE_HALF), (x, y));
        }
    }

    #[test]
    fn depth_orders_diagonals_first() {
        assert!(depth(1, 1, 100) < depth(2, 2, -100) + 2.0);
        assert!(depth(1, 1, 0) < depth(1, 1, 1));
        assert!(depth(0, 1, 5) < depth(1, 1, -5));
    }
}
