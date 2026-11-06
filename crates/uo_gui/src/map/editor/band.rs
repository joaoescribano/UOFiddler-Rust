//! The height band visible in the map editor: only what lies between `from` and `to` is
//! drawn, to edit inside caves and under roofs. The handles move by floors (every 20, where
//! floors usually start) or by 1; the stamp and the cursor stay inside the band.
//!
//! On the rail: dragging and the arrow keys move by 1 (dragging with Shift, by floors); the
//! wheel over the rail moves `WHEEL_STEP` per notch (Shift: by 1). Values are always integers.

use crate::map::scene::Cut;

/// The lowest and the highest Z of the game: the full band hides nothing.
pub const LOWEST: i32 = -128;
pub const HIGHEST: i32 = 127;
/// The most common distance between floors.
pub const FLOOR: i32 = 20;
/// How far one wheel notch over the rail moves the handle.
pub const WHEEL_STEP: i32 = 5;

/// One of the two handles of the rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Handle {
    /// The end of the band (the highest Z).
    #[default]
    Top,
    /// The start (the lowest Z).
    Bottom,
}

/// How many wheel notches there are in an egui scroll (`per_notch` points per notch); a scroll
/// smaller than one notch still counts as one, in its direction.
pub fn notches(delta: f32, per_notch: f32) -> i32 {
    if delta == 0.0 {
        return 0;
    }
    match (delta / per_notch).round() as i32 {
        0 => delta.signum() as i32,
        n => n,
    }
}

/// How far the wheel over the rail moves: `WHEEL_STEP` per notch, or 1 with Shift.
pub fn wheel_step(notches: i32, fine: bool) -> i32 {
    notches * if fine { 1 } else { WHEEL_STEP }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Band {
    pub from: i32,
    pub to: i32,
}

impl Default for Band {
    fn default() -> Self {
        Self::FULL
    }
}

impl Band {
    pub const FULL: Band = Band {
        from: LOWEST,
        to: HIGHEST,
    };

    pub fn is_full(&self) -> bool {
        self.from <= LOWEST && self.to >= HIGHEST
    }

    pub fn contains(&self, z: i32) -> bool {
        (self.from..=self.to).contains(&z)
    }

    /// A height brought inside the band.
    pub fn clamp(&self, z: i32) -> i32 {
        z.clamp(self.from, self.to)
    }

    /// The scene's cut: nothing above `to` nor below `from`.
    pub fn cutaway(&self) -> Cut {
        Cut {
            hide_roofs: false,
            cut: (self.to < HIGHEST).then_some(self.to),
            floor: (self.from > LOWEST).then_some(self.from),
        }
    }

    /// Changes the start: by floors, or by 1 (`fine`). Never goes past the end.
    pub fn with_from(self, value: i32, fine: bool) -> Band {
        let value = value.clamp(LOWEST, HIGHEST);
        let from = if fine || value <= LOWEST {
            value
        } else {
            (value as f32 / FLOOR as f32).round() as i32 * FLOOR
        };
        Band {
            from: from.clamp(LOWEST, self.to),
            to: self.to,
        }
    }

    /// Changes the end: by floors (just below the next floor, to hide it), or by 1. Never
    /// comes before the start.
    pub fn with_to(self, value: i32, fine: bool) -> Band {
        let value = value.clamp(LOWEST, HIGHEST);
        let to = if fine || value >= HIGHEST {
            value
        } else {
            ((value + 1) as f32 / FLOOR as f32).round() as i32 * FLOOR - 1
        };
        Band {
            from: self.from,
            to: to.clamp(self.from, HIGHEST),
        }
    }

    /// The height of a handle.
    pub fn at(&self, handle: Handle) -> i32 {
        match handle {
            Handle::Top => self.to,
            Handle::Bottom => self.from,
        }
    }

    /// Puts a handle at a height: by 1, or by floors with `snap`. One end never passes the
    /// other.
    pub fn with(self, handle: Handle, value: i32, snap: bool) -> Band {
        match handle {
            Handle::Top => self.with_to(value, !snap),
            Handle::Bottom => self.with_from(value, !snap),
        }
    }

    /// Moves a handle `by` (the arrow keys, the wheel), without passing the other nor leaving
    /// the game.
    pub fn nudge(self, handle: Handle, by: i32) -> Band {
        self.with(handle, self.at(handle) + by, false)
    }

    /// The handle nearest to a height (the top one, on a tie).
    pub fn nearest(&self, z: i32) -> Handle {
        if (z - self.to).abs() <= (z - self.from).abs() {
            Handle::Top
        } else {
            Handle::Bottom
        }
    }

    /// The whole band goes up or down, same size, without leaving the game.
    pub fn moved(self, by: i32) -> Band {
        let by = by.clamp(LOWEST - self.from, HIGHEST - self.to);
        Band {
            from: self.from + by,
            to: self.to + by,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_full_band_hides_nothing() {
        let full = Band::default();
        assert!(full.is_full());
        assert_eq!(full.cutaway(), Cut::default());
        assert!(full.contains(-128) && full.contains(127));
    }

    #[test]
    fn handles_snap_to_floors_unless_fine() {
        let band = Band::FULL.with_to(25, false);
        assert_eq!(band.to, 19, "just below the floor above");
        let band = band.with_from(-7, false);
        assert_eq!(band.from, 0);
        assert_eq!(band.with_from(-7, true).from, -7);
        assert_eq!(
            band.with_from(60, false).from,
            19,
            "the start does not pass the end"
        );
        assert_eq!(
            band.with_to(-50, false).to,
            0,
            "the end does not come before the start"
        );
        assert_eq!(
            band.cutaway(),
            Cut {
                hide_roofs: false,
                cut: Some(19),
                floor: Some(0),
            }
        );
        assert_eq!(band.clamp(40), 19);
    }

    #[test]
    fn handles_move_by_one_and_the_wheel_by_five() {
        let band = Band { from: 0, to: 19 };
        // Arrow keys and dragging: by 1.
        assert_eq!(band.nudge(Handle::Top, 1), Band { from: 0, to: 20 });
        assert_eq!(band.nudge(Handle::Bottom, -1), Band { from: -1, to: 19 });
        assert_eq!(band.with(Handle::Top, 23, false).to, 23);
        assert_eq!(band.with(Handle::Top, 23, true).to, 19, "Shift: by floors");
        // The wheel: 5 per notch, 1 with Shift, both ways.
        assert_eq!(wheel_step(1, false), 5);
        assert_eq!(wheel_step(-2, false), -10);
        assert_eq!(wheel_step(3, true), 3);
        assert_eq!(band.nudge(Handle::Top, wheel_step(1, false)).to, 24);
        // One end does not pass the other, and nothing leaves the game.
        assert_eq!(band.nudge(Handle::Bottom, 50), Band { from: 19, to: 19 });
        assert_eq!(band.nudge(Handle::Top, -50), Band { from: 0, to: 0 });
        assert_eq!(Band::FULL.nudge(Handle::Top, 5), Band::FULL);
        assert_eq!(Band::FULL.nudge(Handle::Bottom, -5), Band::FULL);
        assert_eq!(band.nearest(15), Handle::Top);
        assert_eq!(band.nearest(2), Handle::Bottom);
        assert_eq!(band.at(Handle::Bottom), 0);
        // The wheel notches in an egui scroll (50 points per notch).
        assert_eq!(notches(50.0, 50.0), 1);
        assert_eq!(notches(-100.0, 50.0), -2);
        assert_eq!(notches(8.0, 50.0), 1, "less than a notch still moves");
        assert_eq!(notches(-3.0, 50.0), -1);
        assert_eq!(notches(0.0, 50.0), 0);
    }

    #[test]
    fn moving_keeps_the_size_inside_the_game() {
        let band = Band { from: 0, to: 19 };
        assert_eq!(band.moved(20), Band { from: 20, to: 39 });
        assert_eq!(band.moved(500), Band { from: 108, to: 127 });
        assert_eq!(
            band.moved(-500),
            Band {
                from: -128,
                to: -109
            }
        );
    }
}
