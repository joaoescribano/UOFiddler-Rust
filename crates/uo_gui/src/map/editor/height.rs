//! The ground brush: raise, lower, smooth and flatten the tiles of a stroke. Pure: the caller
//! gives the current height of each tile (and of the neighbours, for smoothing) and gets back
//! the new height of the ones that change.

/// What the ground brush does to each tile of the stroke, once per stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Raise,
    Lower,
    /// Each tile goes to the average of itself and its eight neighbours.
    Smooth,
    /// All go to one height: the first tile's of the stroke, or the working Z.
    Flatten,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Raise, Mode::Lower, Mode::Smooth, Mode::Flatten];

    /// The translation key of the mode's name, as in MacroOnline.
    pub fn key(self) -> &'static str {
        match self {
            Mode::Raise => "admin.map.raise",
            Mode::Lower => "admin.map.lower",
            Mode::Smooth => "admin.map.smooth",
            Mode::Flatten => "admin.map.flatten",
        }
    }

    /// The mode's name in English (what MacroOnline's English table gives for `key`).
    pub fn label(self) -> &'static str {
        match self {
            Mode::Raise => "Raise",
            Mode::Lower => "Lower",
            Mode::Smooth => "Smooth",
            Mode::Flatten => "Flatten",
        }
    }
}

/// The new height of the stroke's tiles that change. `step` applies to raising and lowering;
/// `level` is the height to flatten to. Heights stay within what an `i8` holds, and smoothing
/// reads the heights from before the stroke, not the ones already changed.
pub fn apply(
    mode: Mode,
    step: i8,
    level: i8,
    tiles: &[(i32, i32)],
    z_of: impl Fn(i32, i32) -> Option<i8>,
) -> Vec<((i32, i32), i8)> {
    let mut out = Vec::new();
    for &(x, y) in tiles {
        let Some(z) = z_of(x, y) else {
            continue;
        };
        let new = match mode {
            Mode::Raise => z.saturating_add(step),
            Mode::Lower => z.saturating_sub(step),
            Mode::Flatten => level,
            Mode::Smooth => {
                let around: Vec<i32> = (-1..=1)
                    .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                    .filter_map(|(dx, dy)| z_of(x + dx, y + dy).map(i32::from))
                    .collect();
                let sum: i32 = around.iter().sum();
                let n = around.len().max(1) as i32;
                // Rounds to the nearest, below zero too.
                ((2 * sum + n).div_euclid(2 * n)).clamp(-128, 127) as i8
            }
        };
        if new != z {
            out.push(((x, y), new));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ground drawn as text: each number is the height of a tile.
    fn ground<'a>(rows: &'a [&'a [i8]]) -> impl Fn(i32, i32) -> Option<i8> + 'a {
        move |x, y| {
            rows.get(usize::try_from(y).ok()?)?
                .get(usize::try_from(x).ok()?)
                .copied()
        }
    }

    #[test]
    fn raising_and_lowering_move_each_tile_once_and_saturate() {
        let z = ground(&[&[0, 120, -125]]);
        let tiles = [(0, 0), (1, 0), (2, 0)];
        assert_eq!(
            apply(Mode::Raise, 5, 0, &tiles, &z),
            [((0, 0), 5), ((1, 0), 125), ((2, 0), -120)]
        );
        assert_eq!(
            apply(Mode::Lower, 10, 0, &tiles, &z),
            [((0, 0), -10), ((1, 0), 110), ((2, 0), -128)]
        );
        assert!(
            apply(Mode::Raise, 5, 0, &[(9, 9)], &z).is_empty(),
            "off the map"
        );
    }

    #[test]
    fn smoothing_averages_with_the_neighbours_and_flattening_levels() {
        let z = ground(&[&[0, 0, 0], &[0, 18, 0], &[0, 0, 0]]);
        // 18 / 9 = 2: the peak goes down; the neighbours go up a little (18 / 4 or / 6).
        let smoothed = apply(Mode::Smooth, 0, 0, &[(1, 1), (0, 0), (1, 0)], &z);
        assert_eq!(smoothed, [((1, 1), 2), ((0, 0), 5), ((1, 0), 3)]);
        let flat = apply(Mode::Flatten, 0, 0, &[(1, 1), (0, 0)], &z);
        assert_eq!(
            flat,
            [((1, 1), 0)],
            "what is already at the height does not change"
        );
        let low = ground(&[&[-3, -6]]);
        assert_eq!(apply(Mode::Smooth, 0, 0, &[(0, 0)], &low), [((0, 0), -4)]);
    }
}
