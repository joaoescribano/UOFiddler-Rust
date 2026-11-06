//! The small shared types of the map editor: the rectangle of tiles (`Area`) and the border
//! sets (`BorderSet`), with the corner bits of a tile's diamond, as MacroOnline's rule
//! definitions have them.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A rectangle of tiles, edges included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Area {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Area {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        (self.x0..=self.x1).contains(&x) && (self.y0..=self.y1).contains(&y)
    }

    /// How many tiles.
    pub fn size(&self) -> i64 {
        (self.x1 - self.x0 + 1) as i64 * (self.y1 - self.y0 + 1) as i64
    }

    /// The corners in order: the first one at the top left.
    pub fn is_ordered(&self) -> bool {
        self.x0 <= self.x1 && self.y0 <= self.y1
    }

    /// What is left of this rectangle after taking `cut` out: up to four rectangles (the
    /// strip above, the one below, and the sides in the middle), without overlapping.
    pub fn minus(&self, cut: &Area) -> Vec<Area> {
        let (x0, y0) = (self.x0.max(cut.x0), self.y0.max(cut.y0));
        let (x1, y1) = (self.x1.min(cut.x1), self.y1.min(cut.y1));
        if x0 > x1 || y0 > y1 {
            return vec![*self];
        }
        let pieces = [
            Area {
                y1: y0 - 1,
                ..*self
            },
            Area {
                y0: y1 + 1,
                ..*self
            },
            Area {
                y0,
                y1,
                x1: x0 - 1,
                ..*self
            },
            Area {
                y0,
                y1,
                x0: x1 + 1,
                ..*self
            },
        ];
        pieces.into_iter().filter(Area::is_ordered).collect()
    }
}

/// What lies outside a border set: another land, or the black of the void around caves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BorderKind {
    #[default]
    Land,
    /// The border "with the black": the cave floor that ends in the void.
    Cave,
    /// Floors (statics): the borders go inside the stroke, like UOIddler's cave floor
    /// (0x053B).
    Floor,
}

/// The corners of a tile's diamond, as bits of the border masks.
pub mod corner {
    pub const TOP: u8 = 1;
    pub const RIGHT: u8 = 2;
    pub const BOTTOM: u8 = 4;
    pub const LEFT: u8 = 8;
    /// All four: the full center.
    pub const ALL: u8 = 15;
}

/// How many lands fit in one mask of a border set.
pub const MAX_BORDER_VARIANTS: usize = 16;

/// A border set of the map editor, built tile by tile: for each mask of center corners (1 to
/// 15; 15 is the full center), the lands that fit. More than one in a mask is variation.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BorderSet {
    pub name: String,
    #[serde(default)]
    pub kind: BorderKind,
    #[serde(default)]
    pub masks: BTreeMap<u8, Vec<u16>>,
}

impl BorderSet {
    /// The lands of a mask (empty when there are none).
    pub fn tiles(&self, mask: u8) -> &[u16] {
        self.masks.get(&mask).map_or(&[], |v| v.as_slice())
    }

    /// The mask of a land of the set: the full center's first.
    pub fn mask_of(&self, graphic: u16) -> Option<u8> {
        if self.tiles(corner::ALL).contains(&graphic) {
            return Some(corner::ALL);
        }
        self.masks
            .iter()
            .find(|(_, tiles)| tiles.contains(&graphic))
            .map(|(mask, _)| *mask)
    }

    /// Whether the set can be saved: a valid name, masks from 1 to 15 with up to
    /// `MAX_BORDER_VARIANTS` lands each, land graphics only up to 0x3FFF (floors take statics
    /// above it) and at least one land in the full center. These are the checks MacroOnline's
    /// server made on a saved border set, without the one against the other sets' names.
    pub fn check(&self) -> Result<(), String> {
        if !valid_name(&self.name) {
            return Err(format!(
                "invalid name '{}': letters, digits, - and _, up to 40",
                self.name
            ));
        }
        for (mask, tiles) in &self.masks {
            if !(1..=corner::ALL).contains(mask) {
                return Err(format!("border mask {mask}: from 1 to 15"));
            }
            if tiles.len() > MAX_BORDER_VARIANTS {
                return Err(format!(
                    "border mask {mask}: up to {MAX_BORDER_VARIANTS} lands"
                ));
            }
            if self.kind != BorderKind::Floor && tiles.iter().any(|g| *g >= 0x4000) {
                return Err("land graphics go up to 0x3FFF".into());
            }
        }
        if self.tiles(corner::ALL).is_empty() {
            return Err("a border set needs at least one land in the full center".into());
        }
        Ok(())
    }
}

/// The border sets that come with the editor: UOIddler's three, with the same tiles, in the
/// same order, as its database. `cave-floor` is the "Cave in the black", `furrow-grass` the
/// "Furrow Grass" and `cave-floor-statics` the "Cave floor" (floor statics, with the borders
/// inside the stroke).
pub fn seed_border_sets() -> Vec<BorderSet> {
    let set = |name: &str, kind: BorderKind, masks: &[(u8, &[u16])]| BorderSet {
        name: name.to_string(),
        kind,
        masks: masks.iter().map(|(m, t)| (*m, t.to_vec())).collect(),
    };
    vec![
        set(
            "cave-floor",
            BorderKind::Land,
            &[
                (1, &[586]),
                (2, &[587]),
                (3, &[590, 591]),
                (4, &[589]),
                (6, &[592, 593, 594]),
                (7, &[611]),
                (8, &[588]),
                (9, &[597, 598, 596]),
                (11, &[610]),
                (12, &[599, 600, 601]),
                (13, &[612]),
                (14, &[613]),
                (15, &[584, 582, 583, 581, 585, 1597, 1596, 1598, 1595]),
            ],
        ),
        set(
            "furrow-grass",
            BorderKind::Land,
            &[
                (1, &[16]),
                (2, &[15]),
                (3, &[11]),
                (4, &[17]),
                (6, &[12]),
                (7, &[21]),
                (8, &[18]),
                (9, &[10]),
                (11, &[14]),
                (12, &[13]),
                (13, &[20]),
                (14, &[19]),
                (15, &[9]),
            ],
        ),
        set(
            "cave-floor-statics",
            BorderKind::Floor,
            &[
                (1, &[1361]),
                (2, &[1351, 1362]),
                (3, &[1345, 1346, 1347]),
                (4, &[1353]),
                (6, &[1354, 1355, 1356]),
                (8, &[1352, 1363]),
                (9, &[1348, 1349]),
                (12, &[1358, 1359, 1357]),
                (15, &[1339, 1340, 1341, 1342, 1343]),
            ],
        ),
    ]
}

/// A definition name: letters (upper or lower case), digits, `-` and `_`, up to 40. The name
/// is also the key, with the letters as they were typed.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 40
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: i32, y0: i32, x1: i32, y1: i32) -> Area {
        Area { x0, y0, x1, y1 }
    }

    #[test]
    fn cutting_a_rectangle_out_leaves_the_pieces_around() {
        let ring = rect(0, 0, 4, 4).minus(&rect(1, 1, 3, 3));
        assert_eq!(ring.iter().map(Area::size).sum::<i64>(), 25 - 9);
        assert!(!ring.iter().any(|r| r.contains(2, 2)));
        assert!(ring.iter().any(|r| r.contains(0, 2)) && ring.iter().any(|r| r.contains(4, 4)));
        assert!(rect(0, 0, 4, 4).minus(&rect(-5, -5, 10, 10)).is_empty());
        assert_eq!(
            rect(0, 0, 4, 4).minus(&rect(8, 8, 9, 9)),
            [rect(0, 0, 4, 4)]
        );
        assert!(rect(0, 0, 0, 0).is_ordered() && !rect(1, 0, 0, 0).is_ordered());
    }

    #[test]
    fn names_take_letters_digits_dashes_and_underscores() {
        assert!(valid_name("Sword_of-Light2"));
        for bad in ["", "Bad Name", "ação", &"a".repeat(41)] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn border_sets_need_a_center_and_valid_masks() {
        let mut cave = BorderSet {
            name: "cave".into(),
            kind: BorderKind::Cave,
            masks: BTreeMap::from([(corner::TOP, vec![0x0262]), (corner::LEFT, vec![0x024C])]),
        };
        assert!(cave.check().is_err(), "no full center");
        cave.masks.insert(corner::ALL, vec![0x0245, 0x0246]);
        assert_eq!(cave.check(), Ok(()));
        cave.masks.insert(16, vec![0x0245]);
        assert!(cave.check().is_err(), "mask outside 1 to 15");
        cave.masks.remove(&16);
        cave.masks.insert(corner::RIGHT, vec![0x4000]);
        assert!(cave.check().is_err(), "not a land");
        cave.masks
            .insert(corner::RIGHT, vec![0x024B; MAX_BORDER_VARIANTS + 1]);
        assert!(cave.check().is_err(), "too many variations");
        cave.masks.insert(corner::RIGHT, vec![0x024B]);
        assert_eq!(cave.mask_of(0x0246), Some(corner::ALL));
        assert_eq!(cave.mask_of(0x024B), Some(corner::RIGHT));
        assert_eq!(cave.mask_of(0x0003), None);
    }

    #[test]
    fn the_seed_brings_the_uoiddler_border_sets() {
        let seed = seed_border_sets();
        let get = |name: &str| seed.iter().find(|s| s.name == name).unwrap();
        let cave = get("cave-floor");
        assert_eq!(cave.kind, BorderKind::Land);
        assert_eq!(cave.tiles(corner::ALL)[0], 584, "UOIddler's order");
        assert_eq!(cave.mask_of(1595), Some(corner::ALL));
        assert_eq!(cave.tiles(corner::ALL & !corner::TOP), [613]);
        assert_eq!(get("furrow-grass").tiles(corner::ALL), [9]);
        let floor = get("cave-floor-statics");
        assert_eq!(floor.kind, BorderKind::Floor);
        assert_eq!(floor.tiles(corner::ALL)[0], 0x053B);
        assert_eq!(floor.masks.len(), 9);
        for set in &seed {
            assert!(set.check().is_ok(), "{}", set.name);
        }
        // A floor may have static graphics above 0x3FFF; a land may not.
        let mut big = floor.clone();
        big.masks.insert(1, vec![0x4100]);
        assert!(big.check().is_ok());
        big.kind = BorderKind::Land;
        assert!(big.check().is_err());
    }
}
