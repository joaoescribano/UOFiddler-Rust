//! The land heights of a region, with a border, for the corner normals of sloped land and the
//! light falling on them.

/// Brightness of flat land: the vertical normal dotted with the light.
pub const FLAT_LIGHT: f32 = 0.853_553_4;
const BRIGHT_LIGHT: f32 = 1.5;
const LIGHT: [f32; 3] = [
    0.0,
    std::f32::consts::FRAC_1_SQRT_2,
    std::f32::consts::FRAC_1_SQRT_2,
];

pub struct HeightGrid {
    pub x0: i32,
    pub y0: i32,
    pub width: i32,
    pub height: i32,
    pub z: Vec<i8>,
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

impl HeightGrid {
    /// Heights of the rectangle `[x0, x1) x [y0, y1)`, read through `z_at`.
    pub fn new(x0: i32, y0: i32, x1: i32, y1: i32, mut z_at: impl FnMut(i32, i32) -> i8) -> Self {
        let (width, height) = ((x1 - x0).max(1), (y1 - y0).max(1));
        let mut z = Vec::with_capacity((width * height) as usize);
        for y in y0..y0 + height {
            for x in x0..x0 + width {
                z.push(z_at(x, y));
            }
        }
        Self {
            x0,
            y0,
            width,
            height,
            z,
        }
    }

    /// Height of the top corner of a tile, clamped to the grid.
    pub fn z(&self, x: i32, y: i32) -> i32 {
        let x = (x - self.x0).clamp(0, self.width - 1);
        let y = (y - self.y0).clamp(0, self.height - 1);
        self.z[(y * self.width + x) as usize] as i32
    }

    /// The normal at vertex `(x, y)`, or `None` when everything around is flat.
    pub fn normal(&self, x: i32, y: i32) -> Option<[f32; 3]> {
        let z = self.z(x, y);
        let (e, s, w, n) = (
            self.z(x + 1, y),
            self.z(x, y + 1),
            self.z(x - 1, y),
            self.z(x, y - 1),
        );
        if e == z && s == z && w == z && n == z {
            return None;
        }
        let dz = |h: i32| (h - z) as f32 * 4.0;
        let east = [22.0, 22.0, dz(e)];
        let south = [-22.0, 22.0, dz(s)];
        let west = [-22.0, -22.0, dz(w)];
        let north = [22.0, -22.0, dz(n)];
        let mut normal = add(
            add(cross(east, south), cross(south, west)),
            add(cross(west, north), cross(north, east)),
        );
        if normal[2] < 0.0 {
            normal = [-normal[0], -normal[1], -normal[2]];
        }
        let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if len == 0.0 {
            return None;
        }
        Some([normal[0] / len, normal[1] / len, normal[2] / len])
    }

    /// The light at a vertex, multiplying the texture: the flat brightness (0.85) on flat land,
    /// and relief brightens or darkens it with contrast 1.5. That is the client's formula, which
    /// is why textured land is a bit darker than flat art.
    pub fn light(&self, x: i32, y: i32) -> f32 {
        let n = self.normal(x, y).unwrap_or([0.0, 0.0, 1.0]);
        let dot = n[0] * LIGHT[0] + n[1] * LIGHT[1] + n[2] * LIGHT[2];
        let base = dot.max(0.0) / 2.0 + 0.5;
        let bright = base + (BRIGHT_LIGHT - 1.0) * (base - FLAT_LIGHT);
        bright.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(z: Vec<i8>, width: i32) -> HeightGrid {
        let height = z.len() as i32 / width;
        HeightGrid {
            x0: 0,
            y0: 0,
            width,
            height,
            z,
        }
    }

    #[test]
    fn flat_ground_has_no_normal_and_flat_light() {
        let g = grid(vec![3; 9], 3);
        assert!(g.normal(1, 1).is_none());
        assert!((g.light(1, 1) - FLAT_LIGHT).abs() < 1e-6);
    }

    #[test]
    fn slopes_facing_away_from_the_light_are_darker() {
        let toward = grid(vec![10, 10, 10, 0, 0, 0, -10, -10, -10], 3);
        let away = grid(vec![-10, -10, -10, 0, 0, 0, 10, 10, 10], 3);
        assert!(toward.light(1, 1) > FLAT_LIGHT && toward.light(1, 1) <= 1.0);
        assert!(away.light(1, 1) < FLAT_LIGHT);
    }
}
