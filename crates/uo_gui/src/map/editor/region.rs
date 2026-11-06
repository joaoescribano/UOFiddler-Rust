//! The region selection of the map editor, in the part that does not need the map: what is
//! copied from a rectangle (the statics with their height over each tile's ground, and the
//! land when asked), the saved templates (with a name, in `~/.config/uofiddler`) and the tiles
//! a fill picks.

use super::terrain::roll;
use super::types::Area;
use std::path::PathBuf;

/// The templates file, next to the app's settings.
fn templates_file() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".config/uofiddler/map_templates.txt")
}

/// A copied static: where in the rectangle, the graphic, the height over the tile's ground
/// and the hue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub dx: i32,
    pub dy: i32,
    pub graphic: u16,
    pub dz: i32,
    pub hue: u16,
}

/// What was copied from a region: the size, the statics and, when asked, the land of each
/// tile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Clip {
    pub w: i32,
    pub h: i32,
    pub pieces: Vec<Piece>,
    pub lands: Vec<(i32, i32, u16)>,
}

impl Clip {
    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty() && self.lands.is_empty()
    }

    /// The tiles the clip covers with its top corner at `x, y`.
    pub fn area_at(&self, x: i32, y: i32) -> Area {
        Area {
            x0: x,
            y0: y,
            x1: x + self.w.max(1) - 1,
            y1: y + self.h.max(1) - 1,
        }
    }
}

/// A template: a clip with a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub name: String,
    pub clip: Clip,
}

/// The templates as text: one block per template, one line per piece.
///
/// ```text
/// template small house
/// size 5 4
/// s 1 2 0x0B7C 0 0
/// l 0 0 0x0071
/// end
/// ```
pub fn to_text(templates: &[Template]) -> String {
    let mut out = String::new();
    for template in templates {
        out.push_str(&format!("template {}\n", template.name.trim()));
        out.push_str(&format!("size {} {}\n", template.clip.w, template.clip.h));
        for p in &template.clip.pieces {
            out.push_str(&format!(
                "s {} {} 0x{:04X} {} {}\n",
                p.dx, p.dy, p.graphic, p.dz, p.hue
            ));
        }
        for (dx, dy, graphic) in &template.clip.lands {
            out.push_str(&format!("l {dx} {dy} 0x{graphic:04X}\n"));
        }
        out.push_str("end\n");
    }
    out
}

/// The templates in a text; lines that do not parse are left out.
pub fn from_text(text: &str) -> Vec<Template> {
    let hex = |s: &str| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok();
    let mut out = Vec::new();
    let mut current: Option<Template> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("template ") {
            current = Some(Template {
                name: name.trim().to_string(),
                clip: Clip::default(),
            });
            continue;
        }
        if line == "end" {
            out.extend(current.take());
            continue;
        }
        let Some(template) = &mut current else {
            continue;
        };
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts.as_slice() {
            ["size", w, h] => {
                template.clip.w = w.parse().unwrap_or(1);
                template.clip.h = h.parse().unwrap_or(1);
            }
            ["s", dx, dy, graphic, dz, hue] => {
                if let (Ok(dx), Ok(dy), Some(graphic), Ok(dz), Ok(hue)) = (
                    dx.parse(),
                    dy.parse(),
                    hex(graphic),
                    dz.parse(),
                    hue.parse(),
                ) {
                    template.clip.pieces.push(Piece {
                        dx,
                        dy,
                        graphic,
                        dz,
                        hue,
                    });
                }
            }
            ["l", dx, dy, graphic] => {
                if let (Ok(dx), Ok(dy), Some(graphic)) = (dx.parse(), dy.parse(), hex(graphic)) {
                    template.clip.lands.push((dx, dy, graphic));
                }
            }
            _ => {}
        }
    }
    out
}

/// The saved templates.
pub fn load_templates() -> Vec<Template> {
    std::fs::read_to_string(templates_file())
        .map(|text| from_text(&text))
        .unwrap_or_default()
}

/// Saves the templates.
pub fn save_templates(templates: &[Template]) {
    let path = templates_file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, to_text(templates)) {
        eprintln!("could not save {}: {e}", path.display());
    }
}

/// The tiles of a rectangle that a fill takes: each one with the chance `density` (1 to 100),
/// by a stable hash of the tile and of the turn (`seed`), and only those `keep` accepts (the
/// empty ones, when asked).
pub fn fill_targets(
    area: Area,
    density: u8,
    seed: u32,
    keep: impl Fn(i32, i32) -> bool,
) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for y in area.y0..=area.y1 {
        for x in area.x0..=area.x1 {
            let chance = roll(x ^ seed as i32, y.wrapping_add(seed as i32)) % 100;
            if chance < density as usize && keep(x, y) {
                out.push((x, y));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Template {
        Template {
            name: "small house".into(),
            clip: Clip {
                w: 5,
                h: 4,
                pieces: vec![
                    Piece {
                        dx: 1,
                        dy: 2,
                        graphic: 0x0B7C,
                        dz: 0,
                        hue: 0,
                    },
                    Piece {
                        dx: 4,
                        dy: 3,
                        graphic: 0x0E76,
                        dz: -6,
                        hue: 1153,
                    },
                ],
                lands: vec![(0, 0, 0x0071)],
            },
        }
    }

    #[test]
    fn templates_survive_the_file() {
        let saved = vec![
            sample(),
            Template {
                name: "empty".into(),
                clip: Clip {
                    w: 1,
                    h: 1,
                    ..Default::default()
                },
            },
        ];
        assert_eq!(from_text(&to_text(&saved)), saved);
        // Garbage and a block without an end are left out.
        assert!(from_text("s 1 2 3\ntemplate x\nsize 1 1\n").is_empty());
    }

    #[test]
    fn a_clip_covers_its_size_from_the_corner() {
        let clip = sample().clip;
        assert_eq!(
            clip.area_at(10, 20),
            Area {
                x0: 10,
                y0: 20,
                x1: 14,
                y1: 23
            }
        );
        assert!(!clip.is_empty() && Clip::default().is_empty());
    }

    #[test]
    fn fills_follow_the_density_and_skip_what_is_taken() {
        let area = Area {
            x0: 0,
            y0: 0,
            x1: 19,
            y1: 19,
        };
        assert_eq!(fill_targets(area, 100, 0, |_, _| true).len(), 400);
        let half = fill_targets(area, 50, 0, |_, _| true).len();
        assert!((140..260).contains(&half), "{half}");
        assert_ne!(
            fill_targets(area, 50, 1, |_, _| true),
            fill_targets(area, 50, 2, |_, _| true),
            "another turn, another draw"
        );
        assert!(fill_targets(area, 100, 0, |x, _| x > 9)
            .iter()
            .all(|t| t.0 > 9));
    }
}
