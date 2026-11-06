//! The visual art picker: a grid window, like an inventory, with each entry's
//! art (statics from the art, terrain from the land art, hues from the
//! hues.mul ramps). Only what is in view is decoded and turned into a texture,
//! and the textures are kept by key. Search by tiledata name and by id
//! (`0x1F03` or `7939`), categories derived from the flags and the names,
//! per-user favorites and recents (saved in
//! `~/.config/uofiddler/picker_prefs.txt`), and a large preview of the
//! selected entry with its name, id, flags and height.
//!
//! A caller opens it with a caller name and the current value, and reads the
//! choice later: `picker.open("item-graphic", Kind::Static, g)` and, on a
//! later frame, `picker.take_result("item-graphic")`. The window is drawn by
//! `Picker::show`, which the owner calls every frame, after its own panels.
//! `Browse` and `Picker::browse` embed the same grid without a window (the map
//! editor's palette).

use super::{style, widgets};
use crate::map::assets::MapAssets;
use crate::map::iso::flags;
use eframe::egui::{self, Color32, Rect, RichText, Rounding, Sense, Stroke};
use egui_phosphor::regular as icon;
use std::collections::HashMap;
use std::path::PathBuf;
use uo_core::{color16_to_rgba, HuesManager};

/// What is being picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// The art of an item or map static.
    Static,
    /// A terrain tile.
    Land,
    /// A color from hues.mul.
    Hue,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Static, Kind::Land, Kind::Hue];

    /// The short name, in the favorites file and in `UOFIDDLER_PICKER`.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Static => "static",
            Kind::Land => "land",
            Kind::Hue => "hue",
        }
    }

    pub fn parse(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }

    fn title(self) -> &'static str {
        match self {
            Kind::Static => "Pick a graphic",
            Kind::Land => "Pick a terrain",
            Kind::Hue => "Pick a hue",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Kind::Static => icon::CUBE,
            Kind::Land => icon::MOUNTAINS,
            Kind::Hue => icon::PALETTE,
        }
    }

    /// The side of a grid cell.
    fn cell(self) -> f32 {
        match self {
            Kind::Static => 64.0,
            Kind::Land => 56.0,
            Kind::Hue => 30.0,
        }
    }

    /// The kind's own categories, after All, Favorites and Recent.
    fn categories(self) -> &'static [Category] {
        use Category::*;
        match self {
            Kind::Static => &[
                Wearable, Weapon, Armor, Container, Light, Wall, Floor, Roof, Door, Window, Stairs,
                Plant, Water, Animated,
            ],
            Kind::Land => &[
                Grass, Dirt, Sand, Rock, Snow, Cave, Lava, Swamp, Forest, Water,
            ],
            Kind::Hue => &[Gray, Red, Orange, Yellow, Green, Cyan, Blue, Purple, Pink],
        }
    }
}

/// A category of the left column. The first three apply to every kind; the
/// others come from the tiledata flags and names (or from the color).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    All,
    Favorites,
    Recent,
    // Statics.
    Wearable,
    Weapon,
    Armor,
    Container,
    Light,
    Wall,
    Floor,
    Roof,
    Door,
    Window,
    Stairs,
    Plant,
    Water,
    Animated,
    // Terrain (and water, above).
    Grass,
    Dirt,
    Sand,
    Rock,
    Snow,
    Cave,
    Lava,
    Swamp,
    Forest,
    // Hues.
    Gray,
    Red,
    Orange,
    Yellow,
    Green,
    Cyan,
    Blue,
    Purple,
    Pink,
}

impl Category {
    fn bit(self) -> u64 {
        1 << self as u8
    }

    fn label(self) -> &'static str {
        use Category::*;
        match self {
            All => "All",
            Favorites => "Favorites",
            Recent => "Recent",
            Wearable => "Wearable",
            Weapon => "Weapons",
            Armor => "Armor",
            Container => "Containers",
            Light => "Lights",
            Wall => "Walls",
            Floor => "Floors",
            Roof => "Roofs",
            Door => "Doors",
            Window => "Windows",
            Stairs => "Stairs",
            Plant => "Plants",
            Water => "Water",
            Animated => "Animated",
            Grass => "Grass",
            Dirt => "Dirt",
            Sand => "Sand",
            Rock => "Rock",
            Snow => "Snow",
            Cave => "Cave",
            Lava => "Lava",
            Swamp => "Swamp",
            Forest => "Forest",
            Gray => "Grays",
            Red => "Reds",
            Orange => "Oranges",
            Yellow => "Yellows",
            Green => "Greens",
            Cyan => "Cyans",
            Blue => "Blues",
            Purple => "Purples",
            Pink => "Pinks",
        }
    }

    fn icon(self) -> &'static str {
        use Category::*;
        match self {
            All => icon::SQUARES_FOUR,
            Favorites => icon::STAR,
            Recent => icon::CLOCK_COUNTER_CLOCKWISE,
            Wearable => icon::T_SHIRT,
            Weapon => icon::SWORD,
            Armor => icon::SHIELD,
            Container => icon::PACKAGE,
            Light => icon::LAMP,
            Wall => icon::WALL,
            Floor => icon::SQUARE,
            Roof => icon::HOUSE,
            Door => icon::DOOR,
            Window => icon::FRAME_CORNERS,
            Stairs => icon::STAIRS,
            Plant => icon::PLANT,
            Water | Swamp => icon::DROP,
            Animated => icon::FILM_STRIP,
            Grass => icon::LEAF,
            Dirt | Sand => icon::GRAINS,
            Rock | Cave => icon::MOUNTAINS,
            Snow => icon::SNOWFLAKE,
            Lava => icon::FIRE,
            Forest => icon::TREE,
            _ => icon::CIRCLE,
        }
    }
}

fn contains_any(name: &str, words: &[&str]) -> bool {
    words.iter().any(|w| name.contains(w))
}

/// The categories of a static, from its tiledata flags, its height and its
/// name (lowercase).
pub fn static_categories(tile_flags: u64, height: u8, name: &str) -> u64 {
    use Category::*;
    let mut out = 0;
    let mut add = |c: Category, yes: bool| {
        if yes {
            out |= c.bit();
        }
    };
    let has = |f: u64| tile_flags & f != 0;
    add(Wearable, has(flags::WEARABLE));
    add(Weapon, has(flags::WEAPON));
    add(Armor, has(flags::ARMOR));
    add(Container, has(flags::CONTAINER));
    add(Light, has(flags::LIGHT_SOURCE));
    add(Wall, has(flags::WALL));
    let stairs = has(flags::STAIR_BACK) || has(flags::STAIR_RIGHT) || name.contains("stair");
    add(Stairs, stairs);
    add(Roof, has(flags::ROOF));
    add(Door, has(flags::DOOR) || name.contains("door"));
    add(Window, has(flags::WINDOW) || name.contains("window"));
    // Floor: the name says so, or a shallow surface one walks on.
    let flat = has(flags::SURFACE)
        && height <= 1
        && !has(flags::IMPASSABLE | flags::WALL | flags::ROOF)
        && !stairs;
    add(
        Floor,
        flat || contains_any(name, &["floor", "flagstone", "cobblestone", "paving"]),
    );
    add(
        Plant,
        has(flags::FOLIAGE)
            || contains_any(
                name,
                &[
                    "tree", "plant", "flower", "bush", "fern", "vine", "grass", "leaves",
                    "mushroom", "cactus", "weed", "reed", "lily", "shrub", "sapling",
                ],
            ),
    );
    add(Water, has(flags::WET));
    add(Animated, has(flags::ANIMATION));
    out
}

/// The categories of a terrain tile, from its name (lowercase), and water
/// from the flag.
pub fn land_categories(tile_flags: u64, name: &str) -> u64 {
    use Category::*;
    let rules: [(Category, &[&str]); 9] = [
        (Grass, &["grass"]),
        (Dirt, &["dirt", "mud", "furrow"]),
        (Sand, &["sand", "beach"]),
        (Rock, &["rock", "mountain", "stone"]),
        (Snow, &["snow", "ice"]),
        (Cave, &["cave"]),
        (Lava, &["lava"]),
        (Swamp, &["swamp", "marsh", "bog"]),
        (Forest, &["forest", "jungle", "leaves", "tree"]),
    ];
    let mut out = 0;
    for (category, words) in rules {
        if contains_any(name, words) {
            out |= category.bit();
        }
    }
    if tile_flags & flags::WET != 0 || name.contains("water") {
        out |= Water.bit();
    }
    out
}

/// The family of a color, for the hue categories: gray when it has almost no
/// saturation or is too dark, and by hue angle otherwise.
pub fn hue_family(r: u8, g: u8, b: u8) -> Category {
    use Category::*;
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    if max < 0.12 || delta / max.max(1e-6) < 0.18 {
        return Gray;
    }
    let mut h = if max == r {
        60.0 * ((g - b) / delta)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    if h < 0.0 {
        h += 360.0;
    }
    match h {
        h if !(15.0..345.0).contains(&h) => Red,
        h if h < 40.0 => Orange,
        h if h < 70.0 => Yellow,
        h if h < 160.0 => Green,
        h if h < 200.0 => Cyan,
        h if h < 255.0 => Blue,
        h if h < 290.0 => Purple,
        _ => Pink,
    }
}

/// A typed id: `0x1F03` in hexadecimal or `7939` in decimal.
pub fn parse_id(text: &str) -> Option<u32> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return u32::from_str_radix(hex, 16).ok();
    }
    if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
        return text.parse().ok();
    }
    None
}

/// A grid entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: u32,
    pub name: String,
    /// The name in lowercase, for the search.
    lower: String,
    /// The categories, one bit per `Category`.
    cats: u64,
}

impl Entry {
    pub fn new(id: u32, name: &str, cats: u64) -> Self {
        Self {
            id,
            name: name.to_string(),
            lower: name.to_lowercase(),
            cats,
        }
    }
}

/// Everything a kind has, built once.
#[derive(Default)]
pub struct Catalog {
    entries: Vec<Entry>,
    by_id: HashMap<u32, usize>,
    counts: HashMap<Category, usize>,
}

impl Catalog {
    pub fn new(entries: Vec<Entry>) -> Self {
        let by_id = entries.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
        let mut counts = HashMap::new();
        for e in &entries {
            let mut bits = e.cats;
            while bits != 0 {
                let at = bits.trailing_zeros();
                bits &= bits - 1;
                if let Some(c) = CATEGORY_BY_BIT.get(at as usize) {
                    *counts.entry(*c).or_insert(0) += 1;
                }
            }
        }
        Self {
            entries,
            by_id,
            counts,
        }
    }

    fn get(&self, id: u32) -> Option<&Entry> {
        self.by_id.get(&id).map(|&i| &self.entries[i])
    }

    /// The entries that pass the search and the category, as indices. The
    /// typed id comes first; Favorites and Recent follow the list's order.
    pub fn filter(&self, query: &str, category: Category, listed: &[u32]) -> Vec<usize> {
        let query = query.trim().to_lowercase();
        let id = parse_id(&query);
        let keeps = |e: &Entry| query.is_empty() || Some(e.id) == id || e.lower.contains(&query);
        let mut out: Vec<usize> = match category {
            Category::Favorites | Category::Recent => listed
                .iter()
                .filter_map(|id| self.by_id.get(id).copied())
                .filter(|&i| keeps(&self.entries[i]))
                .collect(),
            Category::All => (0..self.entries.len())
                .filter(|&i| keeps(&self.entries[i]))
                .collect(),
            c => (0..self.entries.len())
                .filter(|&i| self.entries[i].cats & c.bit() != 0 && keeps(&self.entries[i]))
                .collect(),
        };
        if let Some(id) = id {
            out.sort_by_key(|&i| self.entries[i].id != id);
        }
        out
    }
}

/// The categories by bit number, in the enum's order.
const CATEGORY_BY_BIT: [Category; 35] = {
    use Category::*;
    [
        All, Favorites, Recent, Wearable, Weapon, Armor, Container, Light, Wall, Floor, Roof, Door,
        Window, Stairs, Plant, Water, Animated, Grass, Dirt, Sand, Rock, Snow, Cave, Lava, Swamp,
        Forest, Gray, Red, Orange, Yellow, Green, Cyan, Blue, Purple, Pink,
    ]
};

/// Where statics start in the art index.
const STATIC_OFFSET: usize = 0x4000;

/// Whether the static has art in the client files.
fn has_static(assets: &MapAssets, id: usize) -> bool {
    assets
        .art
        .is_some_and(|art| art.is_valid(STATIC_OFFSET + id))
}

/// Whether the terrain tile has art in the client files.
fn has_land(assets: &MapAssets, id: usize) -> bool {
    assets.art.is_some_and(|art| art.is_valid(id))
}

/// The statics with art, the terrain tiles with art, and the hues.
fn build(kind: Kind, assets: &MapAssets) -> Catalog {
    let entries = match kind {
        Kind::Static => assets
            .tiledata
            .map_or(&[][..], |data| data.item_tiles.as_slice())
            .iter()
            .enumerate()
            .filter(|(id, _)| has_static(assets, *id))
            .map(|(id, tile)| {
                let cats = static_categories(tile.flags, tile.height, &tile.name.to_lowercase());
                Entry::new(id as u32, &tile.name, cats)
            })
            .collect(),
        Kind::Land => assets
            .tiledata
            .map_or(&[][..], |data| data.land_tiles.as_slice())
            .iter()
            .enumerate()
            .filter(|(id, _)| has_land(assets, *id))
            .map(|(id, tile)| {
                let cats = land_categories(tile.flags, &tile.name.to_lowercase());
                Entry::new(id as u32, &tile.name, cats)
            })
            .collect(),
        Kind::Hue => (0..=assets.hues.map_or(0, |h| h.hues.len()) as u32)
            .map(|h| {
                let cats = hue_color(assets.hues, h as u16)
                    .map(|c| hue_family(c.r(), c.g(), c.b()).bit())
                    .unwrap_or(0);
                let name = if h == 0 { "No hue" } else { "" };
                Entry::new(h, name, cats)
            })
            .collect(),
    };
    Catalog::new(entries)
}

/// A client 16-bit color as an egui color.
fn color(c: u16) -> Color32 {
    let [r, g, b, a] = color16_to_rgba(c);
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// The shade of the ramp shown as a hue's swatch.
const SWATCH_SHADE: usize = 24;

/// The 32 colors of a hue's ramp; `None` for 0 (no hue) or past the table.
fn ramp(hues: Option<&HuesManager>, hue: u16) -> Option<&[u16; 32]> {
    if hue == 0 {
        return None;
    }
    hues?.hues.get(hue as usize - 1).map(|h| &h.colors)
}

/// The swatch color of a hue; `None` for 0 (no hue) or past the table.
pub fn hue_color(hues: Option<&HuesManager>, hue: u16) -> Option<Color32> {
    ramp(hues, hue).map(|r| color(r[SWATCH_SHADE]))
}

/// The 32 colors of a hue's ramp, from dark to light.
fn hue_ramp(hues: Option<&HuesManager>, hue: u16) -> Option<[Color32; 32]> {
    let ramp = ramp(hues, hue)?;
    Some(std::array::from_fn(|i| color(ramp[i])))
}

/// The user's favorites and recents, per kind.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Prefs {
    favorites: HashMap<Kind, Vec<u32>>,
    recents: HashMap<Kind, Vec<u32>>,
}

impl Prefs {
    /// How many recents are kept per kind.
    const RECENTS: usize = 32;

    pub fn favorites(&self, kind: Kind) -> &[u32] {
        self.favorites.get(&kind).map_or(&[], |v| v)
    }

    pub fn recents(&self, kind: Kind) -> &[u32] {
        self.recents.get(&kind).map_or(&[], |v| v)
    }

    pub fn is_favorite(&self, kind: Kind, id: u32) -> bool {
        self.favorites(kind).contains(&id)
    }

    /// Marks or unmarks; favorites stay in the order they were added.
    pub fn toggle_favorite(&mut self, kind: Kind, id: u32) {
        let list = self.favorites.entry(kind).or_default();
        match list.iter().position(|&f| f == id) {
            Some(at) => {
                list.remove(at);
            }
            None => list.push(id),
        }
    }

    /// The last pick goes to the front, without repeats, up to `RECENTS`.
    pub fn push_recent(&mut self, kind: Kind, id: u32) {
        let list = self.recents.entry(kind).or_default();
        list.retain(|&r| r != id);
        list.insert(0, id);
        list.truncate(Self::RECENTS);
    }

    /// One line per entry: `fav static 7939`, `recent hue 33`.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for kind in Kind::ALL {
            for id in self.favorites(kind) {
                out.push_str(&format!("fav {} {id}\n", kind.name()));
            }
            for id in self.recents(kind) {
                out.push_str(&format!("recent {} {id}\n", kind.name()));
            }
        }
        out
    }

    /// Reads what `to_text` wrote; lines that don't parse are left out.
    pub fn from_text(text: &str) -> Prefs {
        let mut prefs = Prefs::default();
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let (Some(list), Some(kind), Some(id)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let (Some(kind), Ok(id)) = (Kind::parse(kind), id.parse::<u32>()) else {
                continue;
            };
            let target = match list {
                "fav" => &mut prefs.favorites,
                "recent" => &mut prefs.recents,
                _ => continue,
            };
            let ids = target.entry(kind).or_default();
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        prefs
    }
}

/// The favorites and recents file, next to the app's settings.json. The unit
/// tests never touch it.
fn prefs_path() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/uofiddler/picker_prefs.txt"))
}

fn load_prefs() -> Prefs {
    prefs_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| Prefs::from_text(&text))
        .unwrap_or_default()
}

fn save_prefs(prefs: &Prefs) {
    let Some(path) = prefs_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, prefs.to_text()) {
        eprintln!("could not save {}: {e}", path.display());
    }
}

/// `UOFIDDLER_PICKER=static/0x0F51/dagger`: the kind, the selected id and the
/// search (for hues, which have no name, the third field is the graphic shown
/// tinted in the preview), to open the picker right at startup (screenshots,
/// manual tests).
pub fn parse_spec(spec: &str) -> Option<(Kind, u32, String)> {
    let mut parts = spec.splitn(3, '/');
    let kind = Kind::parse(parts.next()?.trim())?;
    let id = parts.next().and_then(parse_id).unwrap_or(0);
    let query = parts.next().unwrap_or("").to_string();
    Some((kind, id, query))
}

/// A thumbnail: the texture, or nothing when the entry has no art.
type Thumb = Option<egui::TextureHandle>;

/// The thumbnails already decoded, by key (kind, id, hue), with the last
/// frame they showed up in, to drop the ones out of view when past the limit.
#[derive(Default)]
struct Thumbs {
    map: HashMap<(Kind, u32, u16), (Thumb, u64)>,
    frame: u64,
    /// How many more can be decoded this frame.
    budget: usize,
}

impl Thumbs {
    /// How many textures are kept before dropping the ones not in view.
    const KEEP: usize = 3000;

    fn next_frame(&mut self) {
        self.frame += 1;
        self.budget = 48;
        if self.map.len() > Self::KEEP {
            let recent = self.frame.saturating_sub(2);
            self.map.retain(|_, (_, seen)| *seen >= recent);
        }
    }

    /// The thumbnail, or `None` while it was not decoded yet because this
    /// frame already decoded enough. Asks for another frame then.
    fn get(
        &mut self,
        ctx: &egui::Context,
        assets: &MapAssets,
        kind: Kind,
        id: u32,
        hue: u16,
    ) -> Option<Thumb> {
        let key = (kind, id, hue);
        if let Some((thumb, seen)) = self.map.get_mut(&key) {
            *seen = self.frame;
            return Some(thumb.clone());
        }
        if self.budget == 0 {
            ctx.request_repaint();
            return None;
        }
        self.budget -= 1;
        Some(self.read(ctx, assets, kind, id, hue))
    }

    /// The thumbnail of a field outside the window: without the frame's
    /// budget (there are few), kept along with the grid's.
    fn fetch(
        &mut self,
        ctx: &egui::Context,
        assets: &MapAssets,
        kind: Kind,
        id: u32,
        hue: u16,
    ) -> Thumb {
        let key = (kind, id, hue);
        if let Some((thumb, seen)) = self.map.get_mut(&key) {
            *seen = self.frame;
            return thumb.clone();
        }
        self.read(ctx, assets, kind, id, hue)
    }

    /// Decodes the image, makes the texture and keeps it.
    fn read(
        &mut self,
        ctx: &egui::Context,
        assets: &MapAssets,
        kind: Kind,
        id: u32,
        hue: u16,
    ) -> Thumb {
        let thumb = load(assets, kind, id, hue).map(|(size, rgba)| {
            ctx.load_texture(
                format!("picker-{}-{id}-{hue}", kind.name()),
                egui::ColorImage::from_rgba_unmultiplied(size, &rgba),
                egui::TextureOptions::NEAREST,
            )
        });
        self.map
            .insert((kind, id, hue), (thumb.clone(), self.frame));
        thumb
    }
}

/// The image of an entry (size and RGBA); `None` when it has no art.
fn load(assets: &MapAssets, kind: Kind, id: u32, hue: u16) -> Option<([usize; 2], Vec<u8>)> {
    let image = match kind {
        Kind::Static => assets.static_image(id as u16, hue),
        Kind::Land => assets.land_image(id as u16),
        Kind::Hue => None,
    }?;
    (image.width > 0 && image.height > 0).then_some(([image.width, image.height], image.rgba))
}

/// The open window: for whom, what, and the search state.
struct Open {
    caller: String,
    kind: Kind,
    current: u32,
    selected: Option<u32>,
    query: String,
    category: Category,
    /// A graphic to show tinted in the hue preview.
    tint: Option<(Kind, u16)>,
    /// The search result, as catalog indices, and what produced it.
    shown: Vec<usize>,
    shown_for: Option<(String, Category, u64)>,
    /// Scroll to the selected entry on the next frame.
    scroll_to: bool,
}

impl Open {
    fn new(caller: &str, kind: Kind, current: u32) -> Self {
        Self {
            caller: caller.to_string(),
            kind,
            current,
            selected: Some(current),
            query: String::new(),
            category: Category::All,
            tint: None,
            shown: Vec::new(),
            shown_for: None,
            scroll_to: true,
        }
    }
}

/// An embedded grid, without a window (the map editor's palette): the kind,
/// the search, the category and the selection, kept by whoever shows it.
pub struct Browse {
    open: Open,
}

impl Browse {
    pub fn new(kind: Kind, current: u32) -> Self {
        Self {
            open: Open::new("", kind, current),
        }
    }

    /// What is in use now: marked and selected in the grid, which scrolls to it.
    pub fn set_current(&mut self, current: u32) {
        self.open.current = current;
        if self.open.selected != Some(current) {
            self.open.selected = Some(current);
            self.open.scroll_to = true;
        }
    }

    /// Scrolls the grid to the selected entry on the next frame.
    pub fn reveal(&mut self) {
        self.open.scroll_to = true;
    }
}

/// The picker, kept by its owner (the map editor).
#[derive(Default)]
pub struct Picker {
    open: Option<Open>,
    /// The choice made, for the caller to read.
    result: Option<(String, u32)>,
    catalogs: HashMap<Kind, Catalog>,
    prefs: Option<Prefs>,
    /// Goes up on every change of the favorites and recents, to redo the search.
    prefs_version: u64,
    thumbs: Thumbs,
    /// `UOFIDDLER_PICKER` was already read.
    param_read: bool,
}

impl Picker {
    /// Opens the picker of a kind for a caller, with the current value selected.
    pub fn open(&mut self, caller: &str, kind: Kind, current: u32) {
        self.open = Some(Open::new(caller, kind, current));
    }

    /// Opens the hue picker; the preview shows the graphic tinted.
    pub fn open_hue(&mut self, caller: &str, current: u16, tint: Option<(Kind, u16)>) {
        self.open(caller, Kind::Hue, current as u32);
        if let Some(open) = &mut self.open {
            open.tint = tint;
        }
    }

    /// Whether the window is open.
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The choice made for this caller, once.
    pub fn take_result(&mut self, caller: &str) -> Option<u32> {
        if self.result.as_ref().is_some_and(|(c, _)| c == caller) {
            return self.result.take().map(|(_, id)| id);
        }
        None
    }

    /// A choice made for a caller, as if the window had made it (for the
    /// callers' tests).
    #[cfg(test)]
    pub fn answer(&mut self, caller: &str, id: u32) {
        self.result = Some((caller.to_string(), id));
    }

    /// Drops the catalogs and the thumbnails, for when the client files were
    /// (re)loaded: they are rebuilt from the new files when next shown.
    pub fn reload(&mut self) {
        self.catalogs.clear();
        self.thumbs = Thumbs::default();
        if let Some(open) = &mut self.open {
            open.shown_for = None;
        }
    }

    /// The thumbnail of an entry, for the fields: `None` when it has no art.
    pub fn thumb(
        &mut self,
        ctx: &egui::Context,
        assets: &MapAssets,
        kind: Kind,
        id: u32,
        hue: u16,
    ) -> Thumb {
        self.thumbs.fetch(ctx, assets, kind, id, hue)
    }

    /// `UOFIDDLER_PICKER=kind[/id[/search]]` opens the picker once, at
    /// startup; `hue/1153/0x1F03` shows hue 1153 on a robe.
    pub fn open_from_param(&mut self) {
        if std::mem::replace(&mut self.param_read, true) {
            return;
        }
        let Some((kind, id, query)) = std::env::var("UOFIDDLER_PICKER")
            .ok()
            .and_then(|spec| parse_spec(&spec))
        else {
            return;
        };
        match (kind, parse_id(&query)) {
            (Kind::Hue, Some(graphic)) => {
                self.open_hue("debug", id as u16, Some((Kind::Static, graphic as u16)))
            }
            _ => {
                self.open("debug", kind, id);
                if let Some(open) = &mut self.open {
                    open.query = query;
                }
            }
        }
    }

    /// The kind's catalog (built the first time) and the search result,
    /// redone when the search, the category or the favorites change.
    fn refresh(&mut self, open: &mut Open, assets: &MapAssets) {
        let prefs = self.prefs.get_or_insert_with(load_prefs);
        let catalog = self
            .catalogs
            .entry(open.kind)
            .or_insert_with(|| build(open.kind, assets));
        let key = (open.query.clone(), open.category, self.prefs_version);
        if open.shown_for.as_ref() != Some(&key) {
            let listed = match open.category {
                Category::Favorites => prefs.favorites(open.kind),
                Category::Recent => prefs.recents(open.kind),
                _ => &[],
            };
            open.shown = catalog.filter(&open.query, open.category, listed);
            open.shown_for = Some(key);
        }
    }

    /// The embedded grid: the categories on the left and the grid with the
    /// search. A click picks (and goes into the recents); right-click marks a
    /// favorite. Returns the entry picked this frame.
    pub fn browse(
        &mut self,
        ui: &mut egui::Ui,
        browse: &mut Browse,
        assets: &MapAssets,
    ) -> Option<u32> {
        let open = &mut browse.open;
        self.thumbs.next_frame();
        self.refresh(open, assets);
        let before = open.selected;
        let height = ui.available_height();
        let side = (ui.available_width() * 0.3).clamp(120.0, 170.0);
        let middle = (ui.available_width() - side - 2.0 * style::GAP).max(120.0);
        let mut action = None;
        ui.horizontal_top(|ui| {
            column(ui, side, height, |ui| self.categories(ui, open));
            ui.separator();
            column(ui, middle, height, |ui| {
                action = self.grid(ui, open, assets)
            });
        });
        let prefs = self.prefs.get_or_insert_with(Prefs::default);
        let chosen = match action {
            Some(Action::Favorite(id)) => {
                prefs.toggle_favorite(open.kind, id);
                save_prefs(prefs);
                self.prefs_version += 1;
                None
            }
            Some(Action::Use(id)) => Some(id),
            _ => open.selected.filter(|s| before != Some(*s)),
        };
        if let Some(id) = chosen {
            open.selected = Some(id);
            open.current = id;
            prefs.push_recent(open.kind, id);
            save_prefs(prefs);
            self.prefs_version += 1;
        }
        chosen
    }

    /// Draws the window, if open, over everything else. Call it every frame,
    /// after the owner's panels.
    pub fn show(&mut self, ctx: &egui::Context, assets: &MapAssets) {
        let Some(mut open) = self.open.take() else {
            return;
        };
        self.thumbs.next_frame();
        self.refresh(&mut open, assets);
        let screen = ctx.screen_rect();
        let size = egui::vec2(
            (screen.width() * 0.85).clamp(560.0, 1080.0),
            (screen.height() * 0.85).clamp(380.0, 720.0),
        );
        let mut action = Action::None;
        let frame = egui::Frame::none()
            .fill(style::color::BG)
            .stroke(Stroke::new(1.0_f32, style::color::BORDER))
            .rounding(Rounding::same(style::RADIUS + 2.0))
            .inner_margin(egui::Margin::same(14.0));
        let should_close = modal(ctx, egui::Id::new("admin-picker"), frame, |ui| {
            ui.set_min_size(size);
            ui.set_max_size(size);
            action = self.window(ui, &mut open, assets, size);
        });
        if should_close && action == Action::None {
            action = Action::Close;
        }
        let prefs = self.prefs.get_or_insert_with(Prefs::default);
        match action {
            Action::None => self.open = Some(open),
            Action::Close => {}
            Action::Use(id) => {
                prefs.push_recent(open.kind, id);
                save_prefs(prefs);
                self.prefs_version += 1;
                self.result = Some((open.caller, id));
            }
            Action::Favorite(id) => {
                prefs.toggle_favorite(open.kind, id);
                save_prefs(prefs);
                self.prefs_version += 1;
                self.open = Some(open);
            }
        }
    }

    /// The content: the title, and the three columns (categories, grid, preview).
    fn window(
        &mut self,
        ui: &mut egui::Ui,
        open: &mut Open,
        assets: &MapAssets,
        size: egui::Vec2,
    ) -> Action {
        let mut action = Action::None;
        ui.horizontal(|ui| {
            ui.label(RichText::new(open.kind.icon()).font(egui::FontId::proportional(22.0)));
            ui.label(RichText::new(open.kind.title()).font(style::title()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, icon::X, "Cancel").clicked() {
                    action = Action::Close;
                }
                ui.label(
                    RichText::new(
                        "Double-click uses; right-click marks a favorite; Enter uses the first result.",
                    )
                    .font(style::small())
                    .color(style::color::TEXT_DIM),
                );
            });
        });
        ui.add_space(4.0);
        ui.separator();
        let height = ui.available_height();
        let side = 170.0;
        let preview = 260.0;
        let middle = (size.x - side - preview - 2.0 * 2.0 * style::GAP).max(160.0);
        ui.horizontal_top(|ui| {
            column(ui, side, height, |ui| self.categories(ui, open));
            ui.separator();
            column(ui, middle, height, |ui| {
                if let Some(a) = self.grid(ui, open, assets) {
                    action = a;
                }
            });
            ui.separator();
            column(ui, preview, height, |ui| {
                if let Some(a) = self.preview(ui, open, assets) {
                    action = a;
                }
            });
        });
        action
    }

    /// The categories column, with how many entries each has.
    fn categories(&self, ui: &mut egui::Ui, open: &mut Open) {
        let catalog = &self.catalogs[&open.kind];
        let prefs = self.prefs.as_ref();
        egui::ScrollArea::vertical()
            .id_salt("picker-categories")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let fixed = [Category::All, Category::Favorites, Category::Recent];
                for category in fixed.iter().chain(open.kind.categories()) {
                    let count = match category {
                        Category::All => catalog.entries.len(),
                        Category::Favorites => prefs.map_or(0, |p| p.favorites(open.kind).len()),
                        Category::Recent => prefs.map_or(0, |p| p.recents(open.kind).len()),
                        c => catalog.counts.get(c).copied().unwrap_or(0),
                    };
                    if count == 0 && !fixed.contains(category) {
                        continue;
                    }
                    let label = format!("{}  ({count})", category.label());
                    let selected = open.category == *category;
                    if widgets::nav_item(ui, category.icon(), &label, selected).clicked() {
                        open.category = *category;
                        open.scroll_to = true;
                    }
                    if *category == Category::Recent {
                        ui.separator();
                    }
                }
            });
    }

    /// The search, the count, and the grid: only the rows in view are drawn.
    fn grid(&mut self, ui: &mut egui::Ui, open: &mut Open, assets: &MapAssets) -> Option<Action> {
        let mut action = None;
        let search = widgets::search(ui, &mut open.query, "Name, or id (0x1F03 or 7939)");
        if search.changed() {
            open.scroll_to = true;
        }
        if search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let first = open
                .shown
                .first()
                .map(|&i| self.catalogs[&open.kind].entries[i].id);
            // Enter uses the selected entry if it is in the search, otherwise
            // the first one.
            let selected = open.selected.filter(|s| {
                open.shown
                    .iter()
                    .any(|&i| self.catalogs[&open.kind].entries[i].id == *s)
            });
            if let Some(id) = selected.or(first) {
                action = Some(Action::Use(id));
            }
        }
        let catalog = &self.catalogs[&open.kind];
        ui.label(
            RichText::new(format!("{} of {}", open.shown.len(), catalog.entries.len()))
                .font(style::small())
                .color(style::color::TEXT_DIM),
        );
        if open.shown.is_empty() {
            widgets::empty(ui, icon::MAGNIFYING_GLASS, "Nothing here.");
            return action;
        }
        let gap = 6.0;
        let width = ui.available_width();
        let cols = (((width + gap) / (open.kind.cell() + gap)).floor() as usize).max(1);
        // The cells grow a little so the grid takes the whole width.
        let cell = (width - gap * (cols - 1) as f32) / cols as f32 - 0.5;
        let rows = open.shown.len().div_ceil(cols);
        let step = cell + gap;
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt(("picker-grid", open.kind))
            .auto_shrink([false, false]);
        if std::mem::take(&mut open.scroll_to) {
            let at = open
                .selected
                .and_then(|s| open.shown.iter().position(|&i| catalog.entries[i].id == s))
                .unwrap_or(0);
            // The selected entry lands on the second row, with a bit of
            // context above.
            let row = (at / cols).saturating_sub(1);
            scroll = scroll.vertical_scroll_offset(row as f32 * step);
        }
        let prefs = self.prefs.as_ref().expect("prefs loaded");
        let hues = assets.hues;
        let thumbs = &mut self.thumbs;
        scroll.show_viewport(ui, |ui, viewport| {
            ui.set_height(rows as f32 * step);
            let origin = ui.max_rect().min;
            let first = (viewport.min.y / step).floor().max(0.0) as usize;
            let last = ((viewport.max.y / step).ceil() as usize + 1).min(rows);
            for row in first..last {
                for col in 0..cols {
                    let Some(&index) = open.shown.get(row * cols + col) else {
                        break;
                    };
                    let entry = &catalog.entries[index];
                    let rect = Rect::from_min_size(
                        origin + egui::vec2(col as f32 * step, row as f32 * step),
                        egui::vec2(cell, cell),
                    );
                    let response = ui
                        .interact(rect, ui.id().with(("cell", entry.id)), Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    let selected = open.selected == Some(entry.id);
                    let painter = ui.painter_at(rect.expand(1.0));
                    let fill = if selected {
                        style::color::ACCENT_BG
                    } else if response.hovered() {
                        style::color::BORDER
                    } else {
                        style::color::SURFACE_2
                    };
                    painter.rect_filled(rect, Rounding::same(4.0), fill);
                    match open.kind {
                        Kind::Hue => match hue_color(hues, entry.id as u16) {
                            Some(c) => {
                                painter.rect_filled(rect.shrink(3.0), Rounding::same(3.0), c);
                            }
                            None => {
                                painter.text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    icon::PROHIBIT,
                                    style::body(),
                                    style::color::TEXT_DIM,
                                );
                            }
                        },
                        kind => match thumbs.get(ui.ctx(), assets, kind, entry.id, 0) {
                            Some(Some(texture)) => fit(&painter, rect.shrink(4.0), &texture, 1.0),
                            Some(None) => {
                                painter.text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    icon::IMAGE_BROKEN,
                                    style::body(),
                                    style::color::BORDER,
                                );
                            }
                            None => {
                                painter.text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    icon::HOURGLASS,
                                    style::small(),
                                    style::color::BORDER,
                                );
                            }
                        },
                    }
                    if selected {
                        // Shrunk by half the stroke width: egui 0.29 centers
                        // strokes on the edge, the source drew them inside.
                        painter.rect_stroke(
                            rect.shrink(1.0),
                            Rounding::same(4.0),
                            Stroke::new(2.0_f32, style::color::ACCENT),
                        );
                    }
                    if open.kind != Kind::Hue && cell >= 48.0 {
                        if prefs.is_favorite(open.kind, entry.id) {
                            painter.text(
                                rect.right_top() + egui::vec2(-3.0, 2.0),
                                egui::Align2::RIGHT_TOP,
                                icon::STAR,
                                style::small(),
                                style::color::ACCENT,
                            );
                        }
                        if entry.id == open.current {
                            painter.circle_filled(
                                rect.left_top() + egui::vec2(6.0, 6.0),
                                3.0,
                                style::color::SUCCESS,
                            );
                        }
                    }
                    let response = response.on_hover_text(label(open.kind, entry));
                    if response.clicked() {
                        open.selected = Some(entry.id);
                    }
                    if response.double_clicked() {
                        action = Some(Action::Use(entry.id));
                    }
                    if response.secondary_clicked() {
                        action = Some(Action::Favorite(entry.id));
                    }
                }
            }
        });
        action
    }

    /// The large preview of the selected entry, its data and the buttons.
    fn preview(
        &mut self,
        ui: &mut egui::Ui,
        open: &mut Open,
        assets: &MapAssets,
    ) -> Option<Action> {
        let mut action = None;
        let ctx = ui.ctx().clone();
        egui::TopBottomPanel::bottom("picker-actions")
            .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(0.0, 6.0)))
            .show_separator_line(true)
            .show_inside(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let use_it = ui.add_enabled_ui(open.selected.is_some(), |ui| {
                        widgets::action(ui, icon::CHECK, "Use", widgets::Kind::Primary)
                    });
                    if let (true, Some(id)) = (use_it.inner.clicked(), open.selected) {
                        action = Some(Action::Use(id));
                    }
                    if widgets::action(ui, icon::X, "Cancel", widgets::Kind::Normal).clicked() {
                        action = Some(Action::Close);
                    }
                });
            });
        let Some(id) = open.selected else {
            widgets::empty(ui, open.kind.icon(), "Pick something in the grid.");
            return action;
        };
        let catalog = &self.catalogs[&open.kind];
        let entry = catalog.get(id);
        // The art, in a fixed frame.
        widgets::card(ui, |ui| {
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 200.0), Sense::hover());
            let painter = ui.painter_at(rect);
            match open.kind {
                Kind::Hue => hue_preview(
                    ui,
                    &painter,
                    rect,
                    assets,
                    id as u16,
                    open,
                    &mut self.thumbs,
                ),
                kind => match self.thumbs.get(&ctx, assets, kind, id, 0) {
                    Some(Some(texture)) => fit(&painter, rect.shrink(6.0), &texture, 3.0),
                    _ => {
                        painter.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            icon::IMAGE_BROKEN,
                            egui::FontId::proportional(32.0),
                            style::color::BORDER,
                        );
                    }
                },
            }
        });
        ui.add_space(6.0);
        let name = entry.map(|e| e.name.as_str()).unwrap_or("");
        let title = if name.is_empty() {
            label(open.kind, entry.unwrap_or(&Entry::new(id, "", 0)))
        } else {
            name.to_string()
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).font(style::heading()).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let favorite = self
                    .prefs
                    .as_ref()
                    .is_some_and(|p| p.is_favorite(open.kind, id));
                let (glyph, tip) = if favorite {
                    (
                        RichText::new(icon::STAR).color(style::color::ACCENT),
                        "Remove from favorites",
                    )
                } else {
                    (
                        RichText::new(icon::STAR).color(style::color::TEXT_DIM),
                        "Add to favorites",
                    )
                };
                if ui
                    .add(egui::Button::new(glyph.font(style::icon())).frame(false))
                    .on_hover_text(tip)
                    .clicked()
                {
                    action = Some(Action::Favorite(id));
                }
            });
        });
        egui::ScrollArea::vertical()
            .id_salt("picker-details")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                widgets::fields(ui, "picker-details-grid", |ui| {
                    widgets::label(ui, "Id");
                    ui.label(format!("0x{id:04X}  ({id})"));
                    ui.end_row();
                    if id == open.current {
                        widgets::label(ui, "");
                        ui.label(RichText::new("the current value").color(style::color::SUCCESS));
                        ui.end_row();
                    }
                    details(ui, open.kind, id as u16, assets);
                });
            });
        action
    }
}

/// What the window asked for this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    None,
    Close,
    Use(u32),
    Favorite(u32),
}

/// A modal dialog in the center of the screen over a dimmed backdrop, as
/// egui 0.31's `Modal` (egui 0.29 lacks it). Returns whether it should close:
/// a click on the backdrop, or Escape when no popup is open.
fn modal(
    ctx: &egui::Context,
    id: egui::Id,
    frame: egui::Frame,
    content: impl FnOnce(&mut egui::Ui),
) -> bool {
    let any_popup_open = ctx.memory(|m| m.any_popup_open());
    let shown = egui::Area::new(id)
        .sense(Sense::hover())
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Foreground)
        .interactable(true)
        .show(ctx, |ui| {
            // The picker keeps the panel's look whatever the app's theme is.
            style::apply(ui.style_mut());
            let screen = ui.ctx().screen_rect();
            let mut backdrop = ui.new_child(
                egui::UiBuilder::new()
                    .sense(Sense::click_and_drag())
                    .max_rect(screen),
            );
            backdrop.set_min_size(screen.size());
            ui.painter()
                .rect_filled(screen, 0.0, Color32::from_black_alpha(100));
            let backdrop = backdrop.response();
            // The extra scope senses clicks so they don't reach the backdrop.
            ui.scope_builder(
                egui::UiBuilder::new().sense(Sense::click_and_drag()),
                |ui| frame.show(ui, content),
            );
            backdrop
        });
    let escape = || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    shown.inner.clicked() || (!any_popup_open && escape())
}

/// A column of fixed width and height.
fn column(ui: &mut egui::Ui, width: f32, height: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_size(egui::vec2(width, height));
            ui.set_max_width(width);
            add(ui)
        },
    );
}

/// The texture centered in the rect, scaled up to `max_scale` at most.
pub fn fit(painter: &egui::Painter, rect: Rect, texture: &egui::TextureHandle, max_scale: f32) {
    let size = texture.size_vec2();
    let scale = (rect.width() / size.x)
        .min(rect.height() / size.y)
        .min(max_scale);
    let shown = Rect::from_center_size(rect.center(), size * scale);
    painter.image(
        texture.id(),
        shown,
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
}

/// The name of an entry in the tooltip and the preview: the name, or the
/// kind and the id.
fn label(kind: Kind, entry: &Entry) -> String {
    let id = format!("0x{:04X}", entry.id);
    match kind {
        Kind::Hue if entry.id != 0 => format!("Hue {}", entry.id),
        _ if entry.name.is_empty() => format!("(no name)  {id}"),
        _ => format!("{}  {id}", entry.name),
    }
}

/// The hue's ramp and, if any, the caller's graphic tinted by it.
fn hue_preview(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rect: Rect,
    assets: &MapAssets,
    hue: u16,
    open: &Open,
    thumbs: &mut Thumbs,
) {
    let ramp_rect = Rect::from_min_size(rect.min, egui::vec2(rect.width(), 36.0));
    match hue_ramp(assets.hues, hue) {
        Some(ramp) => {
            let band = ramp_rect.width() / ramp.len() as f32;
            for (i, c) in ramp.iter().enumerate() {
                let r = Rect::from_min_size(
                    ramp_rect.min + egui::vec2(i as f32 * band, 0.0),
                    egui::vec2(band + 0.5, ramp_rect.height()),
                );
                painter.rect_filled(r, Rounding::ZERO, *c);
            }
        }
        None => {
            painter.text(
                ramp_rect.center(),
                egui::Align2::CENTER_CENTER,
                "No hue",
                style::body(),
                style::color::TEXT_DIM,
            );
        }
    }
    let rest = Rect::from_min_max(egui::pos2(rect.min.x, ramp_rect.max.y + 8.0), rect.max);
    match open.tint {
        Some((kind, id)) => {
            if let Some(Some(texture)) = thumbs.get(ui.ctx(), assets, kind, id as u32, hue) {
                fit(painter, rest.shrink(4.0), &texture, 3.0);
            }
        }
        None => {
            if let Some(c) = hue_color(assets.hues, hue) {
                painter.rect_filled(rest.shrink(20.0), Rounding::same(style::RADIUS), c);
            }
        }
    }
}

/// The flags the preview shows, by name.
const FLAG_NAMES: [(u64, &str); 27] = [
    (flags::BACKGROUND, "background"),
    (flags::WEAPON, "weapon"),
    (flags::TRANSLUCENT, "translucent"),
    (flags::WALL, "wall"),
    (flags::DAMAGING, "damaging"),
    (flags::IMPASSABLE, "impassable"),
    (flags::WET, "wet"),
    (flags::SURFACE, "surface"),
    (flags::BRIDGE, "bridge"),
    (flags::GENERIC, "stackable"),
    (flags::WINDOW, "window"),
    (flags::NO_SHOOT, "no-shoot"),
    (flags::INTERNAL, "internal"),
    (flags::FOLIAGE, "foliage"),
    (flags::PARTIAL_HUE, "partial-hue"),
    (flags::NO_HOUSE, "no-house"),
    (flags::MAP, "map"),
    (flags::CONTAINER, "container"),
    (flags::WEARABLE, "wearable"),
    (flags::LIGHT_SOURCE, "light"),
    (flags::ANIMATION, "animation"),
    (flags::NO_DIAGONAL, "no-diagonal"),
    (flags::ARMOR, "armor"),
    (flags::ROOF, "roof"),
    (flags::DOOR, "door"),
    (flags::STAIR_BACK, "stair-back"),
    (flags::STAIR_RIGHT, "stair-right"),
];

/// The names of the flags the tile has.
pub fn flag_names(tile_flags: u64) -> Vec<&'static str> {
    FLAG_NAMES
        .iter()
        .filter(|(f, _)| tile_flags & f != 0)
        .map(|(_, n)| *n)
        .collect()
}

/// The preview rows that depend on the kind: flags, height, weight, layer...
fn details(ui: &mut egui::Ui, kind: Kind, id: u16, assets: &MapAssets) {
    let flags_row = |ui: &mut egui::Ui, tile_flags: u64| {
        widgets::label(ui, "Flags");
        let names = flag_names(tile_flags);
        let text = if names.is_empty() {
            "-".to_string()
        } else {
            names.join(", ")
        };
        ui.add(egui::Label::new(text).wrap());
        ui.end_row();
    };
    match kind {
        Kind::Static => {
            let Some(tile) = assets.item(id) else {
                return;
            };
            widgets::label(ui, "Height");
            ui.label(tile.height.to_string());
            ui.end_row();
            widgets::label(ui, "Weight");
            ui.label(tile.weight.to_string());
            ui.end_row();
            if tile.flags & flags::WEARABLE != 0 {
                // For wearables the tiledata's quality byte is the layer.
                widgets::label(ui, "Layer");
                ui.label(tile.quality.to_string());
                ui.end_row();
                widgets::label(ui, "Animation");
                ui.label(format!("0x{:04X}", tile.anim_id));
                ui.end_row();
            }
            flags_row(ui, tile.flags);
        }
        Kind::Land => {
            let Some(tile) = assets.tiledata.and_then(|t| t.land_tiles.get(id as usize)) else {
                return;
            };
            widgets::label(ui, "Texture");
            ui.label(format!("0x{:04X}", tile.texture_id));
            ui.end_row();
            flags_row(ui, tile.flags);
        }
        Kind::Hue => {
            if let Some(c) = hue_color(assets.hues, id) {
                widgets::label(ui, "Color");
                ui.label(format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b()));
                ui.end_row();
                widgets::label(ui, "Group");
                ui.label(hue_family(c.r(), c.g(), c.b()).label());
                ui.end_row();
            }
        }
    }
}

/// The button that opens the picker, next to a graphic field.
pub fn button(ui: &mut egui::Ui, kind: Kind) -> egui::Response {
    widgets::icon_button(ui, icon::SQUARES_FOUR, kind.title())
}

/// A clickable swatch of the hue, next to a hue field: opens the picker.
pub fn hue_button(ui: &mut egui::Ui, hues: Option<&HuesManager>, hue: u16) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(26.0, 20.0), Sense::click());
    let painter = ui.painter();
    match hue_color(hues, hue) {
        Some(c) => {
            painter.rect_filled(rect, Rounding::same(4.0), c);
        }
        None => {
            painter.rect_filled(rect, Rounding::same(4.0), style::color::SURFACE_2);
            painter.line_segment(
                [rect.left_bottom(), rect.right_top()],
                Stroke::new(1.0_f32, style::color::DANGER),
            );
        }
    }
    let stroke = if response.hovered() {
        style::color::ACCENT
    } else {
        style::color::BORDER
    };
    painter.rect_stroke(
        rect.shrink(0.5),
        Rounding::same(4.0),
        Stroke::new(1.0_f32, stroke),
    );
    response
        .on_hover_text(Kind::Hue.title())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The side of the thumbnail in the fields.
const FIELD_THUMB: f32 = 40.0;

/// The clickable thumbnail of a field: the selected entry's art in a frame;
/// the border lights up under the mouse, and a click opens the picker.
fn thumb_button(
    ui: &mut egui::Ui,
    texture: Option<&egui::TextureHandle>,
    tip: &str,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(FIELD_THUMB, FIELD_THUMB), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        Rounding::same(style::RADIUS),
        ui.visuals().extreme_bg_color,
    );
    match texture {
        Some(texture) => fit(painter, rect.shrink(3.0), texture, 2.0),
        None => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                icon::IMAGE_BROKEN,
                style::icon(),
                style::color::BORDER,
            );
        }
    }
    let stroke = if response.hovered() {
        style::color::ACCENT
    } else {
        style::color::BORDER
    };
    painter.rect_stroke(
        rect.shrink(0.5),
        Rounding::same(style::RADIUS),
        Stroke::new(1.0_f32, stroke),
    );
    response
        .on_hover_text(tip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A graphic field: the thumbnail of the selected entry (tinted by `hue`),
/// the number, which can still be typed in hexadecimal, and the grid button.
/// The thumbnail and the button open the picker for `caller`; the choice
/// comes back through `take_result(caller)`.
pub fn graphic_field(
    ui: &mut egui::Ui,
    picker: &mut Picker,
    assets: &MapAssets,
    caller: &str,
    kind: Kind,
    value: &mut u16,
    hue: u16,
) {
    ui.horizontal(|ui| {
        let texture = picker.thumb(ui.ctx(), assets, kind, *value as u32, hue);
        let mut open = thumb_button(ui, texture.as_ref(), kind.title()).clicked();
        ui.add(egui::DragValue::new(value).hexadecimal(4, false, true));
        open |= button(ui, kind).clicked();
        if open {
            picker.open(caller, kind, *value as u32);
        }
    });
}

/// A hue field: the swatch (which opens the picker for `caller`) and the
/// number, which can still be typed. `tint` (a graphic) shows up tinted in
/// the hue preview.
pub fn hue_field(
    ui: &mut egui::Ui,
    picker: &mut Picker,
    hues: Option<&HuesManager>,
    caller: &str,
    hue: &mut u16,
    tint: Option<(Kind, u16)>,
) {
    ui.horizontal(|ui| {
        if hue_button(ui, hues, *hue).clicked() {
            picker.open_hue(caller, *hue, tint);
        }
        ui.add(egui::DragValue::new(hue).range(0..=3000));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_parse_in_hex_and_decimal() {
        assert_eq!(parse_id("0x1F03"), Some(0x1f03));
        assert_eq!(parse_id(" 0x0f51 "), Some(0x0f51));
        assert_eq!(parse_id("7939"), Some(7939));
        assert_eq!(parse_id("dagger"), None);
        assert_eq!(parse_id("0xzz"), None);
        assert_eq!(parse_id(""), None);
        assert_eq!(
            parse_spec("static/0x0F51/dag"),
            Some((Kind::Static, 0x0f51, "dag".to_string()))
        );
        assert_eq!(parse_spec("hue"), Some((Kind::Hue, 0, String::new())));
        assert_eq!(parse_spec("nope/1"), None);
    }

    #[test]
    fn the_result_goes_to_its_caller_once() {
        let mut picker = Picker::default();
        picker.open_hue("item-hue", 33, Some((Kind::Static, 0x1F03)));
        assert!(picker.open.as_ref().is_some_and(|o| o.kind == Kind::Hue
            && o.selected == Some(33)
            && o.tint == Some((Kind::Static, 0x1F03))));
        picker.result = Some(("item-graphic".into(), 0x0F51));
        assert_eq!(picker.take_result("item-hue"), None);
        assert_eq!(picker.take_result("item-graphic"), Some(0x0F51));
        assert_eq!(picker.take_result("item-graphic"), None);
    }

    #[test]
    fn categories_come_from_flags_and_names() {
        let sword = static_categories(flags::WEAPON | flags::WEARABLE, 1, "longsword");
        assert!(sword & Category::Weapon.bit() != 0);
        assert!(sword & Category::Wearable.bit() != 0);
        assert!(sword & Category::Wall.bit() == 0);
        let wall = static_categories(flags::WALL | flags::IMPASSABLE, 20, "stone wall");
        assert_eq!(wall, Category::Wall.bit());
        let floor = static_categories(flags::SURFACE, 0, "wooden boards");
        assert_eq!(floor, Category::Floor.bit());
        let tree = static_categories(flags::FOLIAGE, 20, "leaves");
        assert!(tree & Category::Plant.bit() != 0);
        let stairs = static_categories(flags::SURFACE | flags::STAIR_BACK, 5, "stairs");
        assert_eq!(stairs, Category::Stairs.bit());
        let water = land_categories(flags::WET, "water");
        assert_eq!(water, Category::Water.bit());
        let grass = land_categories(0, "grass");
        assert_eq!(grass, Category::Grass.bit());
    }

    #[test]
    fn hue_families_by_color() {
        assert_eq!(hue_family(200, 30, 30), Category::Red);
        assert_eq!(hue_family(30, 200, 30), Category::Green);
        assert_eq!(hue_family(30, 30, 200), Category::Blue);
        assert_eq!(hue_family(230, 220, 40), Category::Yellow);
        assert_eq!(hue_family(120, 120, 125), Category::Gray);
        assert_eq!(hue_family(5, 5, 20), Category::Gray, "too dark");
        assert_eq!(hue_family(140, 40, 200), Category::Purple);
    }

    #[test]
    fn search_by_name_id_and_category() {
        let catalog = Catalog::new(vec![
            Entry::new(0x0F51, "dagger", Category::Weapon.bit()),
            Entry::new(0x1F03, "robe", Category::Wearable.bit()),
            Entry::new(0x0050, "stone wall", Category::Wall.bit()),
            Entry::new(0x1F04, "Robe", Category::Wearable.bit()),
        ]);
        let ids = |shown: Vec<usize>| -> Vec<u32> {
            shown.iter().map(|&i| catalog.entries[i].id).collect()
        };
        assert_eq!(ids(catalog.filter("", Category::All, &[])).len(), 4);
        assert_eq!(
            ids(catalog.filter("ROBE", Category::All, &[])),
            [0x1F03, 0x1F04]
        );
        assert_eq!(ids(catalog.filter("0x1f04", Category::All, &[])), [0x1F04]);
        assert_eq!(ids(catalog.filter("80", Category::All, &[])), [0x50]);
        assert_eq!(
            ids(catalog.filter("", Category::Wearable, &[])),
            [0x1F03, 0x1F04]
        );
        assert_eq!(catalog.counts[&Category::Wearable], 2);
        // Favorites and recents in the list's order; ids without an entry are
        // left out.
        assert_eq!(
            ids(catalog.filter("", Category::Recent, &[0x50, 0x9999, 0x0F51])),
            [0x50, 0x0F51]
        );
        assert_eq!(
            ids(catalog.filter("wall", Category::Favorites, &[0x50, 0x0F51])),
            [0x50]
        );
    }

    #[test]
    fn favorites_and_recents_round_trip() {
        let mut prefs = Prefs::default();
        prefs.toggle_favorite(Kind::Static, 7939);
        prefs.toggle_favorite(Kind::Static, 10);
        prefs.toggle_favorite(Kind::Static, 7939);
        assert_eq!(prefs.favorites(Kind::Static), [10]);
        for id in 0..40 {
            prefs.push_recent(Kind::Hue, id);
        }
        prefs.push_recent(Kind::Hue, 5);
        let recents = prefs.recents(Kind::Hue);
        assert_eq!(recents.len(), Prefs::RECENTS);
        assert_eq!(recents[..3], [5, 39, 38]);
        assert_eq!(recents.iter().filter(|&&r| r == 5).count(), 1);
        let text = prefs.to_text();
        assert_eq!(Prefs::from_text(&text), prefs);
        let messy =
            Prefs::from_text("fav static 1\nfav nope 2\njunk\nrecent land x\nfav static 1\n");
        assert_eq!(messy.favorites(Kind::Static), [1]);
        assert!(messy.recents(Kind::Land).is_empty());
        // The dropped body kind of the source is skipped, not misread.
        assert_eq!(Prefs::from_text("fav body 400\n"), Prefs::default());
    }

    #[test]
    fn every_label_has_a_text() {
        for kind in Kind::ALL {
            assert_ne!(kind.title(), "");
            assert_eq!(Kind::parse(kind.name()), Some(kind));
            for c in kind.categories() {
                assert_ne!(c.label(), "", "{c:?}");
            }
        }
        for c in [Category::All, Category::Favorites, Category::Recent] {
            assert_ne!(c.label(), "");
        }
        for (bit, c) in CATEGORY_BY_BIT.iter().enumerate() {
            assert_eq!(*c as usize, bit, "{c:?}");
        }
        assert_eq!(CATEGORY_BY_BIT[Category::Pink as usize], Category::Pink);
        assert_eq!(CATEGORY_BY_BIT[Category::Grass as usize], Category::Grass);
        assert_eq!(flag_names(flags::WET | flags::WALL), ["wall", "wet"]);
    }

    #[test]
    fn a_new_current_is_picked_and_scrolled_to() {
        let mut browse = Browse::new(Kind::Static, 10);
        browse.open.scroll_to = false;
        browse.set_current(10);
        assert!(!browse.open.scroll_to, "the same one does not scroll again");
        browse.set_current(42);
        assert_eq!(browse.open.selected, Some(42));
        assert!(browse.open.scroll_to);
    }

    #[test]
    fn hue_swatches_come_from_the_ramp() {
        let mut colors = [0u16; 32];
        colors[SWATCH_SHADE] = 0x7C00; // pure red in 1555
        colors[0] = 0x001F; // pure blue
        let hues = HuesManager {
            hues: vec![uo_core::Hue {
                index: 0,
                colors,
                table_start: 0,
                table_end: 0,
                name: String::new(),
            }],
        };
        assert_eq!(hue_color(Some(&hues), 0), None, "0 is no hue");
        assert_eq!(hue_color(Some(&hues), 2), None, "past the table");
        assert_eq!(hue_color(None, 1), None);
        let red = hue_color(Some(&hues), 1).unwrap();
        assert_eq!((red.r(), red.g(), red.b()), (255, 0, 0));
        assert_eq!(hue_family(red.r(), red.g(), red.b()), Category::Red);
        let ramp = hue_ramp(Some(&hues), 1).unwrap();
        assert_eq!((ramp[0].r(), ramp[0].b()), (0, 255));
    }

    /// One frame of a headless context, with the given input events.
    fn frame(ctx: &egui::Context, events: Vec<egui::Event>, run: impl FnMut(&egui::Context)) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, run);
    }

    fn click(at: egui::Pos2) -> Vec<Vec<egui::Event>> {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        vec![
            vec![egui::Event::PointerMoved(at)],
            vec![button(true)],
            vec![button(false)],
        ]
    }

    /// Clicks on the backdrop close the modal; clicks on the dialog don't.
    #[test]
    fn the_modal_closes_on_the_backdrop_only() {
        let ctx = egui::Context::default();
        let closes = |at: egui::Pos2| {
            let mut closed = false;
            for events in std::iter::once(Vec::new()).chain(click(at)) {
                frame(&ctx, events, |ctx| {
                    closed |= modal(
                        ctx,
                        egui::Id::new("test-modal"),
                        egui::Frame::none(),
                        |ui| {
                            ui.set_min_size(egui::vec2(200.0, 100.0));
                        },
                    );
                });
            }
            closed
        };
        assert!(!closes(egui::pos2(400.0, 300.0)), "inside the dialog");
        assert!(closes(egui::pos2(20.0, 20.0)), "on the backdrop");
    }

    /// The window and the embedded grid draw every kind without client files
    /// (no art, no tiledata, a single hue), and Escape closes the window
    /// without a result.
    #[test]
    fn window_and_grid_draw_without_files() {
        let maps = uo_core::MapManager::new();
        let edits = uo_core::MapEdits::new();
        let hues = HuesManager {
            hues: vec![uo_core::Hue {
                index: 0,
                colors: [0x7C00; 32],
                table_start: 0,
                table_end: 0,
                name: String::new(),
            }],
        };
        let assets = MapAssets {
            maps: &maps,
            map_id: 0,
            edits: &edits,
            art: None,
            textures: None,
            tiledata: None,
            hues: Some(&hues),
            cache: Default::default(),
        };
        let ctx = egui::Context::default();
        let mut picker = Picker::default();
        let mut browse = Browse::new(Kind::Static, 0);
        for kind in Kind::ALL {
            picker.open("test", kind, 1);
            for _ in 0..3 {
                frame(&ctx, Vec::new(), |ctx| {
                    picker.show(ctx, &assets);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        picker.browse(ui, &mut browse, &assets);
                    });
                });
            }
            assert!(picker.is_open(), "{kind:?}");
        }
        assert_eq!(picker.catalogs[&Kind::Hue].entries.len(), 2);
        let escape = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&ctx, vec![escape], |ctx| picker.show(ctx, &assets));
        assert!(!picker.is_open());
        assert_eq!(picker.take_result("test"), None);
    }

    /// The real client's tiledata and art build catalogs with entries.
    #[test]
    fn catalogs_from_the_client_files() {
        let Some(dir) = std::env::var_os("UOFIDDLER_UO_DIR") else {
            return;
        };
        let dir = PathBuf::from(dir);
        let tiledata =
            uo_core::TileDataManager::from_file(dir.join("tiledata.mul")).expect("tiledata.mul");
        let art = uo_core::FileIndex::from_uop(dir.join("artLegacyMUL.uop"), ".tga", 0x14000)
            .or_else(|_| {
                uo_core::FileIndex::from_mul_idx(
                    dir.join("artidx.mul"),
                    dir.join("art.mul"),
                    0x14000,
                )
            })
            .expect("art");
        let hues = uo_core::HuesManager::from_file(dir.join("hues.mul")).ok();
        let maps = uo_core::MapManager::new();
        let edits = uo_core::MapEdits::new();
        let assets = MapAssets {
            maps: &maps,
            map_id: 0,
            edits: &edits,
            art: Some(&art),
            textures: None,
            tiledata: Some(&tiledata),
            hues: hues.as_ref(),
            cache: Default::default(),
        };
        let statics = build(Kind::Static, &assets);
        assert!(statics.entries.len() > 1000, "{}", statics.entries.len());
        assert!(statics.counts.get(&Category::Wall).copied().unwrap_or(0) > 0);
        let dagger = statics.filter("0x0F51", Category::All, &[]);
        assert_eq!(statics.entries[dagger[0]].id, 0x0F51);
        assert!(load(&assets, Kind::Static, 0x0F51, 0).is_some());
        let lands = build(Kind::Land, &assets);
        assert!(lands.counts.get(&Category::Grass).copied().unwrap_or(0) > 0);
        assert!(load(&assets, Kind::Land, 3, 0).is_some());
        if hues.is_some() {
            let hues = build(Kind::Hue, &assets);
            assert!(hues.entries.len() > 1000);
        }
        // The window decodes only what is in view, within the frame's budget.
        let ctx = egui::Context::default();
        let mut picker = Picker::default();
        picker.open("test", Kind::Static, 0x0F51);
        frame(&ctx, Vec::new(), |ctx| picker.show(ctx, &assets));
        let decoded = picker.thumbs.map.len();
        assert!(decoded > 0 && decoded <= 48, "{decoded}");
        assert!(picker.thumbs.map.values().any(|(t, _)| t.is_some()));
    }
}
