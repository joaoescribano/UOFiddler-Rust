//! The map editor: a **full-screen mode** over the isometric view. Entering it (the "Edit"
//! button of the map tab) hides the app's tabs and the map fills the window; the camera moves
//! freely (WASD, arrows, drag with the middle or right button, the wheel zooms). Around it sit the
//! top bar (tools, palette, undo, brush, Z, hue, grid, go to), the floating palette, the tile
//! inspector on the right, the status bar at the bottom and the height band (`chrome`).
//!
//! The state and the rules live here. Tools act with the cursor: Select and Region by click;
//! Stamp, Terrain, Height and Erase by **brush** (size and shape, an outline following the cursor
//! with x, y, z, drag to paint with a preview of the stroke). Each action becomes the whole new
//! state of every tile it changed, kept in the pending edits of the facet at once, and goes into
//! the history (undo and redo). Nothing reaches the client files until Save. The stamp shows the
//! **ghost** of the static where it will land: on top of what can be stacked on in the tile
//! (tables, boxes, floors) inside the height band, on the ground with Alt, or at the fixed
//! working Z.

use super::band::{self, Band};
use super::history::{Action, History};
use super::text::{t, tf};
use super::types::{Area, BorderKind, BorderSet};
use super::{border_dialog, borders, brush, height, region, storage, terrain};
use crate::map::assets::MapAssets;
use crate::map::iso::{flags, tile_at, tile_origin, TILE_HALF, Z_STEP};
use crate::map::ui::picker::{Browse, Kind};
use crate::map::view::IsoView;
use eframe::egui::{self, Color32};
use std::collections::{BTreeMap, HashMap, HashSet};
use uo_core::{MapEdits, MapTile, PlacedStatic, RemovedStatic};

/// The cave floor, the brush of "use cave floor".
pub(super) const CAVE_FLOOR: u16 = 0x053B;

/// How many tiles of the stroke have their ground above the height band (the `roofTiles`):
/// the rock covering a cave, which painting would open.
fn roof_tiles(stroke: &[(i32, i32)], band: Band, z: impl Fn(i32, i32) -> Option<i8>) -> usize {
    stroke
        .iter()
        .filter(|&&(x, y)| z(x, y).is_some_and(|z| z as i32 > band.to))
        .count()
}

/// How many statics the editor places on one tile at most.
pub(super) const MAX_PLACED: usize = 32;

/// Without a height band, how far above the ground a static still serves as a base to stack on:
/// the upper floor of a house is left out.
const STACK_REACH: i32 = 15;

/// The changes an action makes: the whole new state of each tile it touched.
pub type Changes = Vec<MapTile>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Select,
    Stamp,
    Terrain,
    Height,
    Erase,
    Region,
}

impl Tool {
    pub const ALL: [Tool; 6] = [
        Tool::Select,
        Tool::Stamp,
        Tool::Terrain,
        Tool::Height,
        Tool::Erase,
        Tool::Region,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Tool::Select => "map.select",
            Tool::Stamp => "map.stamp",
            Tool::Terrain => "map.terrain",
            Tool::Height => "map.height",
            Tool::Erase => "map.erase",
            Tool::Region => "map.region",
        }
    }

    /// The short name, in `UOFIDDLER_EDITOR=<tool>`.
    pub fn name(self) -> &'static str {
        self.key().trim_start_matches("map.")
    }

    /// The tool's key.
    pub fn shortcut(self) -> &'static str {
        match self {
            Tool::Select => "V",
            Tool::Stamp => "B",
            Tool::Terrain => "T",
            Tool::Height => "U",
            Tool::Erase => "E",
            Tool::Region => "R",
        }
    }

    /// What the tool does, in the button tip and the status bar.
    pub fn hint(self) -> &'static str {
        t(match self {
            Tool::Select => "map.select_hint",
            Tool::Stamp => "map.stamp_hint",
            Tool::Terrain => "map.terrain_hint",
            Tool::Height => "map.height_hint",
            Tool::Erase => "map.erase_hint",
            Tool::Region => "map.region_hint",
        })
    }

    pub fn icon(self) -> &'static str {
        use egui_phosphor::regular as icon;
        match self {
            Tool::Select => icon::CURSOR,
            Tool::Stamp => icon::STAMP,
            Tool::Terrain => icon::PAINT_BRUSH_BROAD,
            Tool::Height => icon::MOUNTAINS,
            Tool::Erase => icon::ERASER,
            Tool::Region => icon::SELECTION,
        }
    }

    /// Paints with the brush (dragging); the others act by click.
    pub fn brushes(self) -> bool {
        matches!(self, Tool::Stamp | Tool::Terrain | Tool::Height | Tool::Erase)
    }
}

/// A static on a tile, as the map has it now.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct StaticNow {
    pub graphic: u16,
    pub name: String,
    pub z: i8,
    pub hue: u16,
    /// The height of the top: `z` plus the tiledata height.
    pub top: i32,
    /// Serves as a base to stack another one on.
    pub base: bool,
    /// Is a floor (surface in tiledata): can be the visible ground of a storey.
    pub floor: bool,
}

/// What a tile has now (with the edits): the ground and the statics.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TileNow {
    pub x: i32,
    pub y: i32,
    pub land: (u16, String, i8),
    /// Bottom to top, by their top.
    pub statics: Vec<StaticNow>,
}

/// The saved set that is the default of a graphic: the one holding it as its whole core.
fn default_set(sets: &[BorderSet], graphic: u16, floor: bool) -> Option<String> {
    sets.iter()
        .filter(|s| (s.kind == BorderKind::Floor) == floor)
        .find(|s| s.tiles(terrain::ALL).contains(&graphic))
        .map(|s| s.name.clone())
}

/// Can be stacked on: what is a surface (tables, floors, counters) and not a roof, or a low
/// thing that is not a wall, door, window, plant or water (boxes, barrels, anvils).
fn stacks_on(tile_flags: u64, height: u8) -> bool {
    if tile_flags & flags::ROOF != 0 {
        return false;
    }
    if tile_flags & flags::SURFACE != 0 {
        return true;
    }
    let never = flags::WALL | flags::DOOR | flags::WINDOW | flags::FOLIAGE | flags::WET;
    height <= 12 && tile_flags & never == 0
}

/// The ground of a tile whose land the height band hides: the working Z when fixed; with the land
/// above the band (the rock covering a cave), the floor of the band's top storey
/// (`to - FLOOR + 1`, the 0 of band 0..19), never its top nor the mountain's; with it below, the
/// start of the band.
fn hidden_floor(ground: i32, band: Band, fixed: Option<i8>) -> i32 {
    match fixed {
        Some(z) => band.clamp(z as i32),
        None if ground > band.to => band.clamp(band.to - band::FLOOR + 1),
        None => band.from,
    }
}

/// Where a static placed on the tile lands: on the highest top of what serves as a base inside
/// the height band (without a band, up to `STACK_REACH` above the ground); without a base, on
/// the ground, or on the hidden ground (`hidden_floor`) when the land is outside the band.
pub(super) fn landing(tile: &TileNow, band: Band) -> i32 {
    let ground = tile.land.2 as i32;
    let floor = if band.contains(ground) {
        ground
    } else {
        hidden_floor(ground, band, None)
    };
    let ceiling = if band.to < band::HIGHEST {
        band.to
    } else {
        ground + STACK_REACH
    };
    tile.statics
        .iter()
        .filter(|s| s.base && s.top <= ceiling && band.contains(s.z as i32))
        .map(|s| s.top)
        .fold(floor, i32::max)
}

/// The height of the visible ground of a tile inside the height band: the land, when inside it;
/// else the lowest floor inside it (that of the visible storey; floors only: the nameless,
/// flagless statics the map keeps under the rock, at -40 and -60 in Minoc, are not ground);
/// else the hidden ground (`hidden_floor`: the fixed Z, or the floor of the band's storey). The
/// cursor finds the tile and the outline is drawn by it: what the band hides does not count,
/// neither statics nor the rock over a cave.
pub(super) fn surface(tile: &TileNow, band: Band, fixed: Option<i8>) -> i32 {
    let ground = tile.land.2 as i32;
    if band.contains(ground) {
        return ground;
    }
    tile.statics
        .iter()
        .filter(|s| s.floor && band.contains(s.z as i32))
        .map(|s| s.z as i32)
        .min()
        .unwrap_or_else(|| hidden_floor(ground, band, fixed))
}

/// The statics of a tile the height band leaves visible, bottom to top.
pub(super) fn visible(tile: &TileNow, band: Band) -> impl DoubleEndedIterator<Item = &StaticNow> {
    tile.statics
        .iter()
        .filter(move |s| band.contains(s.z as i32))
}

/// The height of the visible ground of a tile, inside a band (`surface`).
fn surface_in(assets: &MapAssets, band: Band, fixed: Option<i8>, x: i32, y: i32) -> Option<i32> {
    read_tile(assets, x, y).map(|t| surface(&t, band, fixed))
}

/// The corner heights of a tile's diamond as the editor draws it: the land's when it is inside
/// the band; else a flat diamond at the height of the visible ground.
fn tile_corners_in(
    assets: &MapAssets,
    band: Band,
    fixed: Option<i8>,
    x: i32,
    y: i32,
) -> [i8; 4] {
    match assets.land_z(x, y) {
        Some(z) if band.contains(z as i32) => corners(assets, x, y),
        _ => {
            let z = surface_in(assets, band, fixed, x, y).unwrap_or(0);
            [z.clamp(-128, 127) as i8; 4]
        }
    }
}

/// The heights of the four points of a tile rectangle's outline, in a band: the top corner of
/// `x0,y0`, the right one of `x1,y0`, the bottom one of `x1,y1` and the left one of `x0,y1`.
fn rect_corners_in(assets: &MapAssets, band: Band, area: Area) -> [i8; 4] {
    [
        tile_corners_in(assets, band, None, area.x0, area.y0)[0],
        tile_corners_in(assets, band, None, area.x1, area.y0)[1],
        tile_corners_in(assets, band, None, area.x1, area.y1)[2],
        tile_corners_in(assets, band, None, area.x0, area.y1)[3],
    ]
}

/// A height as a diamond corner.
fn flat(z: i32) -> i8 {
    z.clamp(-128, 127) as i8
}

/// The tile as the map has it now.
pub(super) fn read_tile(assets: &MapAssets, x: i32, y: i32) -> Option<TileNow> {
    if x < 0 || y < 0 {
        return None;
    }
    let block = assets.block(x / 8, y / 8)?;
    let land = block.cell(x, y);
    let mut statics: Vec<StaticNow> = block
        .statics_at(x, y)
        .map(|s| {
            let tile = assets.item(s.graphic);
            let height = tile.map_or(0, |t| t.height);
            let tile_flags = tile.map_or(0, |t| t.flags);
            StaticNow {
                graphic: s.graphic,
                name: tile.map(|t| t.name.clone()).unwrap_or_default(),
                z: s.z,
                hue: s.hue,
                top: s.z as i32 + height as i32,
                base: tile.is_some_and(|t| stacks_on(t.flags, t.height)),
                floor: tile_flags & flags::SURFACE != 0 && tile_flags & flags::ROOF == 0,
            }
        })
        .collect();
    statics.sort_by_key(|s| (s.top, s.z));
    Some(TileNow {
        x,
        y,
        land: (land.graphic, assets.land_name(land.graphic).to_string(), land.z),
        statics,
    })
}

/// The heights of the four corners of a tile's diamond (top, right, bottom, left), as land is
/// drawn: each corner has the height of the tile it is the top corner of.
pub fn corners(assets: &MapAssets, x: i32, y: i32) -> [i8; 4] {
    let z = |x: i32, y: i32| assets.land_z(x, y).unwrap_or(0);
    [z(x, y), z(x + 1, y), z(x + 1, y + 1), z(x, y + 1)]
}

/// Takes a static out of a change: if the editor placed it, it leaves the placed list; if it is
/// from the original map, it goes into the removed one.
pub(super) fn take_static(tile: &mut MapTile, graphic: u16, z: i8) {
    if let Some(at) = tile
        .add
        .iter()
        .rposition(|s| s.graphic == graphic && s.z == z)
    {
        tile.add.remove(at);
    } else {
        tile.remove.push(RemovedStatic { graphic, z });
    }
}

/// The cursor over the map this frame, with the editor open.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Pointer {
    /// The tile under the cursor; nothing when the cursor is over the interface.
    pub tile: Option<(i32, i32)>,
    /// The left button went down now (outside the interface).
    pub pressed: bool,
    /// The left button is down.
    pub down: bool,
    /// Alt: the static goes to the ground, without stacking.
    pub alt: bool,
    /// Ctrl.
    pub ctrl: bool,
}

/// A static not placed yet, drawn see-through where it will land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ghost {
    pub graphic: u16,
    pub hue: u16,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// What to draw over the map: the grid, the brush outline, the stroke preview, the ghosts, the
/// chosen tile and the label with x, y, z.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlay {
    /// The tile, its corner heights and the diamond sides to stroke.
    pub outline: Vec<((i32, i32), [i8; 4], u8)>,
    /// The tiles of the stroke in progress, filled.
    pub fill: Vec<((i32, i32), [i8; 4])>,
    pub fill_color: Option<Color32>,
    pub ghosts: Vec<Ghost>,
    /// The tile under the cursor, its corners and the text.
    pub label: Option<((i32, i32), [i8; 4], String)>,
    /// The tile chosen in the inspector.
    pub selected: Option<((i32, i32), [i8; 4])>,
    /// The rectangle of a region being drawn, or of a clip being pasted, with the heights of the
    /// four points of its outline.
    pub rect: Option<(Area, [i8; 4])>,
    /// The chosen region.
    pub region: Option<(Area, [i8; 4])>,
    /// The grid of the visible tiles, inside the height band.
    pub grid: Option<(i32, i32)>,
}

/// The stroke in progress: the tiles in the order the brush went over them, without repeating,
/// and where the cursor was last frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Stroke {
    pub tiles: Vec<(i32, i32)>,
    pub seen: HashSet<(i32, i32)>,
    pub last: (i32, i32),
    /// The levelling height: the ground where the stroke started, or the working Z.
    pub level: i8,
    /// Where the stamp's floor goes (the working Z, or where the stamp would place on the tile
    /// where the stroke started): the whole stroke stays at that height.
    pub ground: i8,
}

impl Stroke {
    fn paint(&mut self, tiles: impl IntoIterator<Item = (i32, i32)>) {
        for tile in tiles {
            if tile.0 >= 0 && tile.1 >= 0 && self.seen.insert(tile) {
                self.tiles.push(tile);
            }
        }
    }
}

/// What the palette shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum PaletteMode {
    #[default]
    Objects,
    Lands,
    Templates,
}

pub struct MapEditor {
    /// The pending edits of the facet being edited: the editor's copy of the panel's, kept in
    /// step through the changes each action returns.
    pub(super) tiles: MapEdits,
    /// The full-screen mode is open.
    pub editing: bool,
    pub(super) tool: Tool,
    pub(super) brush: brush::Brush,
    /// The stamp's static and its hue.
    pub(super) graphic: u16,
    pub(super) hue: u16,
    /// The land of the terrain brush.
    pub(super) land: u16,
    /// The fixed working Z: the stamp places at it and levelling goes to it.
    pub(super) working_z: Option<i8>,
    /// What the height brush does, and by how much it raises or lowers.
    pub(super) ground: height::Mode,
    pub(super) step: i8,
    /// The eraser takes every static of the tile, not just the top one.
    pub(super) erase_all: bool,
    /// Land varies among similar whole tiles, and gets borders with the land outside.
    pub(super) vary: bool,
    pub(super) auto_border: bool,
    /// Where the automatic border takes its pieces from, and the saved sets.
    pub(super) border_source: borders::Source,
    pub(super) border_sets: Vec<BorderSet>,
    pub(super) border_dialog: border_dialog::Dialog,
    /// A terrain stroke that went over the height band (a cave's rock), waiting for the answer:
    /// use the cave floor or paint anyway. The number of tiles above the band.
    pub(super) roof: Option<(Stroke, usize)>,
    pub(super) terrain: terrain::Cache,
    pub(super) band: Band,
    pub(super) grid: bool,
    pub(super) palette: bool,
    pub(super) palette_mode: PaletteMode,
    pub(super) statics_browse: Browse,
    pub(super) lands_browse: Browse,
    pub(super) help: bool,
    /// The text of "go to x, y".
    pub(super) goto: String,
    /// The camera should go to this tile next frame.
    pub(super) look_at: Option<(i32, i32)>,
    pub(super) hover: Option<(i32, i32)>,
    /// The tile chosen with Select, and the static chosen on it (graphic and height).
    pub(super) selected: Option<(i32, i32)>,
    pub(super) picked: Option<(u16, i8)>,
    /// The chosen region, the copied clip, and whether the clip follows the cursor to be pasted
    /// (moving: taken from where it was, on paste).
    pub(super) region: Option<Area>,
    /// The height the chosen region was dragged at: it draws flat at it. Nothing when it came
    /// another way (paste, `sel`).
    pub(super) region_z: Option<i32>,
    pub(super) clip: Option<region::Clip>,
    pub(super) pasting: bool,
    pub(super) moving: Option<Area>,
    /// Copying takes the land along.
    pub(super) with_land: bool,
    /// Fill: the chance of each tile (1 to 100), empty ones only, and the round.
    pub(super) density: u8,
    pub(super) empty_only: bool,
    pub(super) fill_seed: u32,
    /// The saved templates (read the first time) and the name of the next one.
    pub(super) templates: Option<Vec<region::Template>>,
    pub(super) template_name: String,
    /// The corner where the region drag started, and the height of its plane: the visible
    /// ground where it started, or the fixed working Z.
    pub(super) corner: Option<(i32, i32)>,
    pub(super) corner_z: i32,
    pub(super) stroke: Option<Stroke>,
    pub(super) history: History,
    /// The status bar message, and whether it is an error.
    pub(super) message: Option<(String, bool)>,
    pub(super) overlay: Option<Overlay>,
    /// `UOFIDDLER_EDITOR=<tool>/x,y`: the cursor stays on that tile (captures).
    pub(super) pinned: Option<(i32, i32)>,
    /// Where the cursor is on screen, over the map: the brush thumbnail follows it.
    pub cursor_screen: Option<egui::Pos2>,
    /// The keyboard belongs to a text field of the interface.
    pub(super) typing: bool,
    /// `.../click`: one click on the cursor's tile.
    pub(super) test_click: bool,
    /// `.../hold`: the click stays down (the stroke preview in view).
    pub(super) test_hold: bool,
    /// `.../move`: move the chosen region before the click.
    pub(super) test_move: bool,
    /// `.../drag2647,552;2655,552`: a stroke dragged through the given tiles, one per frame,
    /// released on the last (captures of a real stroke).
    pub(super) test_drag: std::collections::VecDeque<(i32, i32)>,
    /// The art of statics (by hue) and of lands, as textures.
    pub(super) art: HashMap<(u16, u16), Option<egui::TextureHandle>>,
    pub(super) land_art: HashMap<u16, Option<egui::TextureHandle>>,
}

impl Default for MapEditor {
    fn default() -> Self {
        Self {
            tiles: MapEdits::new(),
            editing: false,
            tool: Tool::Select,
            brush: brush::Brush::default(),
            graphic: 0x0E76,
            hue: 0,
            land: 0x0003,
            working_z: None,
            ground: height::Mode::Raise,
            step: 5,
            erase_all: false,
            vary: true,
            auto_border: true,
            border_source: borders::Source::Detect,
            border_sets: storage::load_border_sets(),
            border_dialog: border_dialog::Dialog::default(),
            roof: None,
            terrain: terrain::Cache::default(),
            band: Band::FULL,
            grid: true,
            palette: false,
            palette_mode: PaletteMode::Objects,
            statics_browse: Browse::new(Kind::Static, 0x0E76),
            lands_browse: Browse::new(Kind::Land, 0x0003),
            help: false,
            goto: String::new(),
            look_at: None,
            hover: None,
            selected: None,
            picked: None,
            region: None,
            region_z: None,
            clip: None,
            pasting: false,
            moving: None,
            with_land: false,
            density: 30,
            empty_only: true,
            fill_seed: 0,
            templates: None,
            template_name: String::new(),
            corner: None,
            corner_z: 0,
            stroke: None,
            history: History::default(),
            message: None,
            overlay: None,
            pinned: None,
            cursor_screen: None,
            typing: false,
            test_click: false,
            test_hold: false,
            test_move: false,
            test_drag: Default::default(),
            art: HashMap::new(),
            land_art: HashMap::new(),
        }
    }
}

/// The rectangle between two corners, in any order.
pub(super) fn rect(a: (i32, i32), b: (i32, i32)) -> Area {
    Area {
        x0: a.0.min(b.0),
        y0: a.1.min(b.1),
        x1: a.0.max(b.0),
        y1: a.1.max(b.1),
    }
}

/// "x, y" (or "x y") as two numbers.
pub(super) fn parse_xy(text: &str) -> Option<(i32, i32)> {
    let mut parts = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|p| !p.is_empty());
    let x = parts.next()?.parse().ok()?;
    let y = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some((x, y))
}

impl MapEditor {
    /// Enters the full-screen mode (or leaves it).
    pub fn set_editing(&mut self, on: bool) {
        self.editing = on;
        self.stroke = None;
        self.overlay = None;
        self.corner = None;
    }

    /// Starts over on a facet's pending edits (opening the editor, switching facets, loading or
    /// saving): the history goes, since it was made against other files.
    pub fn reset(&mut self, edits: &MapEdits) {
        self.tiles = edits.clone();
        self.history = History::default();
        self.stroke = None;
        self.roof = None;
        self.selected = None;
        self.picked = None;
        self.region = None;
        self.pasting = false;
        self.moving = None;
    }

    /// Puts the camera over a tile next frame.
    pub fn look_at_tile(&mut self, x: i32, y: i32) {
        self.look_at = Some((x, y));
    }

    /// The message of the status bar.
    pub fn say(&mut self, text: impl Into<String>, error: bool) {
        self.message = Some((text.into(), error));
    }

    /// `UOFIDDLER_EDITOR=<tool>[/x,y][/palette|lands|templates|help|click|hold|move|drag…|novary
    /// |z0..39|wz20|b5|l0071|s053b|sel1,2,3,4|borders|cave|draft|void|set=name|circle|diamond]`:
    /// opens the editor on the tool; with the tile, puts the cursor on it (and selects it, with
    /// `select`); the rest opens the palette (on lands or templates) or the help, clicks once on
    /// the tile, or sets the height band, the working Z (`wz20`), the brush size, the land or the
    /// stamp; `borders` opens the border sets dialog (`cave`: a cave set; `draft`: the draft read
    /// from the art), `void` and `set=name` choose where borders come from, and `circle` and
    /// `diamond` the brush shape. For captures: `select/1480,1620/palette`.
    pub fn focus(&mut self, spec: &str) {
        let mut parts = spec.split('/');
        let name = parts.next().unwrap_or_default();
        if let Some(tool) = Tool::ALL.into_iter().find(|tool| tool.name() == name) {
            self.set_editing(true);
            self.choose(tool);
        }
        for part in parts {
            match part {
                "palette" => self.palette = true,
                "lands" => {
                    self.palette = true;
                    self.palette_mode = PaletteMode::Lands;
                }
                "templates" => {
                    self.palette = true;
                    self.palette_mode = PaletteMode::Templates;
                }
                "help" => self.help = true,
                "circle" => self.brush.shape = brush::Shape::Circle,
                "diamond" => self.brush.shape = brush::Shape::Diamond,
                // The stamp's static, in hexadecimal (`s053b`).
                stamp
                    if stamp.starts_with('s')
                        && stamp.len() > 1
                        && u16::from_str_radix(&stamp[1..], 16).is_ok() =>
                {
                    self.graphic = u16::from_str_radix(&stamp[1..], 16).unwrap_or(self.graphic);
                }
                "borders" => self.border_dialog.open = true,
                "cave" => self.border_dialog.editing.kind = BorderKind::Cave,
                "draft" => self.border_dialog.test_draft = true,
                "void" => self.border_source = borders::Source::Void,
                set if set.starts_with("set=") => {
                    self.border_source = borders::Source::Set(set[4..].to_string());
                }
                // One click on the cursor's tile, on the editor's first frame.
                "click" => self.test_click = true,
                // The click on the cursor's tile without releasing: the stroke stays in progress.
                "hold" => {
                    self.test_click = true;
                    self.test_hold = true;
                }
                // A dragged stroke: the cursor goes through the tiles, one per frame.
                // `draghold`: the same, without releasing at the end.
                drag if drag.starts_with("drag") => {
                    let path = drag.strip_prefix("draghold").unwrap_or(&drag[4..]);
                    self.test_hold = drag.starts_with("draghold");
                    self.test_drag = path.split(';').filter_map(parse_xy).collect();
                    if let Some(&first) = self.test_drag.front() {
                        self.pinned = Some(first);
                        self.look_at = Some(first);
                    }
                }
                "novary" => self.vary = false,
                // Move the chosen region (`sel1,2,3,4`) to the cursor's tile.
                "move" => self.test_move = true,
                sel if sel.starts_with("sel") => {
                    let n: Vec<i32> = sel[3..].split(',').filter_map(|v| v.parse().ok()).collect();
                    if let [x0, y0, x1, y1] = n[..] {
                        self.region = Some(rect((x0, y0), (x1, y1)));
                        self.region_z = None;
                    }
                }
                // The brush (`b5`) and the land (`l0071`).
                size if size.starts_with('b') && size[1..].parse::<u8>().is_ok() => {
                    self.brush.size = size[1..].parse().unwrap_or(1);
                }
                land if land.starts_with('l') && u16::from_str_radix(&land[1..], 16).is_ok() => {
                    self.land = u16::from_str_radix(&land[1..], 16).unwrap_or(3);
                }
                // The fixed working Z (`wz20`).
                wz if wz.starts_with("wz") && wz[2..].parse::<i8>().is_ok() => {
                    self.working_z = wz[2..].parse().ok();
                }
                band if band.starts_with('z') => {
                    if let Some((from, to)) = band[1..].split_once("..") {
                        if let (Ok(from), Ok(to)) = (from.parse(), to.parse()) {
                            self.band = Band { from, to };
                        }
                    }
                }
                xy => {
                    if let Some((x, y)) = parse_xy(xy) {
                        self.pinned = Some((x, y));
                        self.look_at = Some((x, y));
                        if self.tool == Tool::Select {
                            self.selected = Some((x, y));
                        }
                    }
                }
            }
        }
    }

    /// What to draw over the map.
    pub fn overlay(&self) -> Option<&Overlay> {
        self.overlay.as_ref()
    }

    /// The height of the ground under the cursor last frame (inside the band): the guess to find
    /// the cursor's tile in this one.
    pub fn hover_z(&self, assets: &MapAssets) -> i32 {
        self.hover
            .and_then(|(x, y)| self.surface_z(assets, x, y))
            .unwrap_or(0)
    }

    /// The height of the visible ground of a tile, inside the band (`surface`).
    pub(super) fn surface_z(&self, assets: &MapAssets, x: i32, y: i32) -> Option<i32> {
        surface_in(assets, self.band, self.working_z, x, y)
    }

    /// The corner heights of a tile's diamond, in the editor's band.
    pub(super) fn tile_corners(&self, assets: &MapAssets, x: i32, y: i32) -> [i8; 4] {
        tile_corners_in(assets, self.band, self.working_z, x, y)
    }

    /// The heights of the outline of a tile rectangle, in the editor's band.
    fn rect_corners(&self, assets: &MapAssets, area: Area) -> [i8; 4] {
        rect_corners_in(assets, self.band, area)
    }

    /// The cut of the scene by the height band.
    pub fn cutaway(&self) -> crate::map::scene::Cut {
        self.band.cutaway()
    }

    /// The tile under a point of the world (in pixels): starts from the given height and corrects
    /// it by that of the ground visible inside the band (`surface`), not counting what it hides.
    pub fn pick(&self, assets: &MapAssets, wx: f32, wy: f32, guess_z: i32) -> (i32, i32) {
        // Dragging a rectangle, the opposite corner goes on the plane of the start: the
        // rectangle does not follow the Z of the ground under the mouse.
        if let Some(z) = self.drag_plane() {
            return tile_at(wx, wy + z as f32 * Z_STEP);
        }
        let band = self.band;
        brush::pick(wx, wy, band.clamp(guess_z), |x, y| {
            self.surface_z(assets, x, y)
                .map(|z| band.clamp(z).clamp(-128, 127) as i8)
        })
    }

    /// The height of the plane of the rectangle Region is dragging.
    fn drag_plane(&self) -> Option<i32> {
        let dragging = self.tool == Tool::Region && !self.pasting;
        self.corner.filter(|_| dragging).map(|_| self.corner_z)
    }

    /// Starts dragging a rectangle at `at`: its plane is the fixed working Z, or the visible
    /// ground on that tile.
    fn start_rect(&mut self, assets: &MapAssets, at: (i32, i32)) {
        self.corner = Some(at);
        self.corner_z = match self.working_z {
            Some(z) => z as i32,
            None => self.surface_z(assets, at.0, at.1).unwrap_or(0),
        };
    }

    /// The editor's free camera: WASD (and the arrows, without a chosen static) move (Shift:
    /// faster), Ctrl + wheel moves the height band (Shift: by 1). The view itself drags with the
    /// middle or right button and zooms with the wheel. `over_ui`: the mouse is over the
    /// interface.
    pub fn camera(&mut self, ctx: &egui::Context, view: &mut IsoView, over_ui: bool) {
        if let Some((x, y)) = self.look_at.take() {
            let (ox, oy) = tile_origin(x, y);
            view.camera = egui::pos2(ox + TILE_HALF, oy + TILE_HALF);
        }
        let (shift, ctrl, dt, wheel) = ctx.input(|i| {
            (
                i.modifiers.shift,
                i.modifiers.command,
                i.stable_dt.min(0.1),
                i.raw_scroll_delta.y,
            )
        });
        if !self.typing && !ctx.wants_keyboard_input() {
            let arrows = self.picked.is_none()
                && !(self.tool == Tool::Region && self.region.is_some());
            let keys = [
                (egui::Key::W, egui::Key::ArrowUp, egui::vec2(0.0, -1.0)),
                (egui::Key::S, egui::Key::ArrowDown, egui::vec2(0.0, 1.0)),
                (egui::Key::A, egui::Key::ArrowLeft, egui::vec2(-1.0, 0.0)),
                (egui::Key::D, egui::Key::ArrowRight, egui::vec2(1.0, 0.0)),
            ];
            let mut pan = egui::Vec2::ZERO;
            ctx.input(|i| {
                for (key, arrow, dir) in keys {
                    if (i.key_down(key) && !i.modifiers.command) || (arrows && i.key_down(arrow)) {
                        pan += dir;
                    }
                }
            });
            if pan != egui::Vec2::ZERO {
                let speed = if shift { 1800.0 } else { 700.0 };
                view.camera += pan.normalized() * speed * dt / view.zoom;
                ctx.request_repaint();
            }
        }
        if wheel != 0.0 && !over_ui && ctrl {
            let by = if shift { 1 } else { band::FLOOR };
            self.band = self.band.moved(if wheel > 0.0 { by } else { -by });
        }
    }

    /// The change of a tile as it is stored, or a new empty one.
    pub(super) fn edit_of(&self, x: i32, y: i32) -> MapTile {
        self.tiles
            .get(x, y)
            .cloned()
            .unwrap_or_else(|| MapTile::fresh(x, y))
    }

    /// Puts the new state of tiles in the editor's copy (the next stroke already starts from
    /// it) and returns the before and after of those that changed.
    fn apply(&mut self, changed: Vec<MapTile>) -> Action {
        let mut action = Action::default();
        for tile in changed {
            let before = self.edit_of(tile.x, tile.y);
            if before == tile {
                continue;
            }
            self.tiles.set(tile.clone());
            action.before.push(before);
            action.after.push(tile);
        }
        action
    }

    /// Stores the new state of tiles at once, and keeps the action in the history. What changes
    /// nothing is left out. Returns the changes for the facet's pending edits.
    pub(super) fn commit(&mut self, changed: Vec<MapTile>) -> Changes {
        let action = self.apply(changed);
        if action.after.is_empty() {
            return Vec::new();
        }
        let tiles = action.after.clone();
        self.message = Some((tf("map.changed", &[&tiles.len()]), false));
        self.history.push(action);
        tiles
    }

    /// Undoes the last action: the tiles go back to what they were.
    pub(super) fn undo(&mut self) -> Changes {
        let Some(action) = self.history.undo() else {
            return Vec::new();
        };
        let applied = self.apply(action.before);
        self.message = Some((t("map.undone").into(), false));
        applied.after
    }

    /// Redoes the last undone action.
    pub(super) fn redo(&mut self) -> Changes {
        let Some(action) = self.history.redo() else {
            return Vec::new();
        };
        let applied = self.apply(action.after);
        self.message = Some((t("map.redone").into(), false));
        applied.after
    }

    /// Where the stamp places the static on this tile.
    fn stamp_z(&self, tile: &TileNow, alt: bool) -> i8 {
        let z = match self.working_z {
            Some(z) => z as i32,
            None if alt => surface(tile, self.band, None),
            None => landing(tile, self.band),
        };
        z.clamp(-128, 127) as i8
    }

    /// The cursor over the map, each frame with the editor open: the brush stroke starts, grows
    /// and is stored on release; the other tools act on click. Returns the changes.
    pub fn pointer(&mut self, assets: &MapAssets, mut p: Pointer) -> Changes {
        if !self.test_drag.is_empty() {
            self.drag_step(assets, &mut p);
        }
        let tile = self.pinned.or(p.tile);
        if self.test_move && self.region.is_some() {
            self.test_move = false;
            self.start_move(assets);
        }
        // The border set asked for (`set=name`) has to exist.
        let set_ready = match &self.border_source {
            borders::Source::Set(name) => self.border_set(name).is_some(),
            _ => true,
        };
        if self.test_click
            && set_ready
            && tile.is_some_and(|(x, y)| assets.land_z(x, y).is_some())
        {
            self.test_click = false;
            p.pressed = true;
            p.down = self.test_hold;
        } else if self.test_hold {
            p.down = true;
        }
        self.hover = tile;
        let mut out = Vec::new();
        if self.tool.brushes() {
            if let Some(at) = tile.filter(|_| p.pressed) {
                let level = self
                    .working_z
                    .or_else(|| assets.land_z(at.0, at.1))
                    .unwrap_or(0);
                let ground = read_tile(assets, at.0, at.1)
                    .map(|t| self.stamp_z(&t, false))
                    .unwrap_or(level);
                let mut stroke = Stroke {
                    last: at,
                    level,
                    ground,
                    ..Default::default()
                };
                stroke.paint(self.brush.footprint(at.0, at.1));
                self.stroke = Some(stroke);
            } else if let (Some(stroke), Some(at)) = (&mut self.stroke, tile) {
                if at != stroke.last {
                    for step in brush::line(stroke.last, at).into_iter().skip(1) {
                        stroke.paint(self.brush.footprint(step.0, step.1));
                    }
                    stroke.last = at;
                }
            }
            if !p.down {
                if let Some(stroke) = self.stroke.take() {
                    out = self.finish(assets, stroke, p);
                }
            }
        } else {
            out = self.click(assets, tile, p);
        }
        self.overlay = Some(self.draw(assets, tile, p));
        out
    }

    /// One step of the test stroke (`drag`): presses on the first tile once the map and the
    /// border set are there, takes the cursor to one tile per frame and releases on the last.
    fn drag_step(&mut self, assets: &MapAssets, p: &mut Pointer) {
        let set_ready = match &self.border_source {
            borders::Source::Set(name) => self.border_set(name).is_some(),
            _ => true,
        };
        let Some(&at) = self.test_drag.front() else {
            return;
        };
        // The brush stroke, or Region's rectangle.
        if self.stroke.is_none() && self.corner.is_none() {
            if set_ready && assets.land_z(at.0, at.1).is_some() {
                self.pinned = Some(at);
                p.pressed = true;
                p.down = true;
            }
            return;
        }
        self.test_drag.pop_front();
        match self.test_drag.front() {
            Some(&next) => {
                self.pinned = Some(next);
                p.down = true;
            }
            None => p.down = self.test_hold,
        }
    }

    /// The click tools: Select chooses the tile (and its top static); Region draws a rectangle
    /// by dragging, or pastes the clip.
    fn click(&mut self, assets: &MapAssets, tile: Option<(i32, i32)>, p: Pointer) -> Changes {
        match self.tool {
            Tool::Select => {
                if let Some(at) = tile.filter(|_| p.pressed) {
                    self.selected = Some(at);
                    // The top one among those the band leaves visible.
                    self.picked = read_tile(assets, at.0, at.1).and_then(|t| {
                        visible(&t, self.band)
                            .next_back()
                            .map(|s| (s.graphic, s.z))
                    });
                }
            }
            Tool::Region if self.pasting => {
                if let Some(at) = tile.filter(|_| p.pressed) {
                    return self.paste(assets, at);
                }
            }
            Tool::Region => {
                if let Some(at) = tile.filter(|_| p.pressed) {
                    self.start_rect(assets, at);
                }
                if !p.down {
                    if let (Some(start), Some(end)) = (self.corner.take(), tile) {
                        self.region = Some(rect(start, end));
                        self.region_z = Some(self.corner_z);
                    }
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// The stroke is over: the new state of each of its tiles, by the tool.
    fn finish(&mut self, assets: &MapAssets, mut stroke: Stroke, p: Pointer) -> Changes {
        let mut changed = Vec::new();
        // Height does not touch the ground the band hides. Terrain asks (`roof_tiles`), as in
        // UOIddler.
        if self.tool == Tool::Height {
            let band = self.band;
            stroke.tiles.retain(|&(x, y)| {
                assets
                    .land_z(x, y)
                    .is_some_and(|z| band.contains(z as i32))
            });
        }
        match self.tool {
            Tool::Stamp if self.is_floor(assets, self.graphic) => {
                changed = self.paint_floor(assets, &stroke);
            }
            Tool::Stamp => {
                for &(x, y) in &stroke.tiles {
                    let Some(now) = read_tile(assets, x, y) else {
                        continue;
                    };
                    let mut tile = self.edit_of(x, y);
                    if tile.add.len() >= MAX_PLACED {
                        continue;
                    }
                    tile.add.push(PlacedStatic {
                        graphic: self.graphic,
                        z: self.stamp_z(&now, p.alt),
                        hue: self.hue,
                    });
                    changed.push(tile);
                }
            }
            Tool::Terrain => {
                let roof = roof_tiles(&stroke.tiles, self.band, |x, y| assets.land_z(x, y));
                if roof > 0 {
                    self.roof = Some((stroke, roof));
                    return Vec::new();
                }
                changed = self.paint_land(assets, &stroke.tiles);
            }
            Tool::Erase => {
                for &(x, y) in &stroke.tiles {
                    let Some(now) = read_tile(assets, x, y) else {
                        continue;
                    };
                    let mut gone: Vec<&StaticNow> = visible(&now, self.band).collect();
                    if !self.erase_all {
                        gone = gone.pop().into_iter().collect();
                    }
                    if gone.is_empty() {
                        continue;
                    }
                    let mut tile = self.edit_of(x, y);
                    for s in gone {
                        take_static(&mut tile, s.graphic, s.z);
                    }
                    changed.push(tile);
                }
            }
            Tool::Height => {
                let heights = height::apply(
                    self.ground,
                    self.step,
                    stroke.level,
                    &stroke.tiles,
                    |x, y| assets.land_z(x, y),
                );
                for ((x, y), z) in heights {
                    let mut tile = self.edit_of(x, y);
                    tile.land_z = Some(z);
                    changed.push(tile);
                }
            }
            _ => {}
        }
        self.commit(changed)
    }

    /// Opens the palette (or closes it), on the tool's brush and with the chosen one in view.
    pub(super) fn toggle_palette(&mut self) {
        self.palette = !self.palette;
        if !self.palette {
            return;
        }
        match self.tool {
            Tool::Terrain => self.palette_mode = PaletteMode::Lands,
            Tool::Stamp => self.palette_mode = PaletteMode::Objects,
            _ => {}
        }
        self.statics_browse.set_current(self.graphic as u32);
        self.lands_browse.set_current(self.land as u32);
    }

    /// The tile pinned by `UOFIDDLER_EDITOR` (captures).
    pub fn pinned(&self) -> Option<(i32, i32)> {
        self.pinned
    }

    /// The borders the automatic border would use now, for the preview: the chosen set, the ones
    /// with black, or the ones with the land under the cursor.
    pub(super) fn border_preview(
        &mut self,
        assets: &MapAssets,
    ) -> Option<(terrain::Transitions, u16)> {
        match self.border_source.clone() {
            borders::Source::Set(name) => {
                let set = self.border_set(&name)?;
                let pure = set.tiles(terrain::ALL);
                let core = if pure.contains(&self.land) || pure.is_empty() {
                    self.land
                } else {
                    pure[0]
                };
                Some((borders::transitions(set), core))
            }
            borders::Source::Void => {
                let set = self.terrain.void_transitions(assets, self.land)?;
                Some((set, self.land))
            }
            borders::Source::Detect => {
                let (x, y) = self.hover?;
                let outside = read_tile(assets, x, y)?.land.0;
                let set = self.terrain.transitions(assets, self.land, outside)?;
                Some((set, self.land))
            }
        }
    }

    /// Changes the terrain brush's land. A saved set holding that land as its core becomes the
    /// source of the borders: it is that ground's default (`cave-floor` for the cave ground).
    pub(super) fn pick_land(&mut self, land: u16) {
        self.land = land;
        if let Some(name) = default_set(&self.border_sets, land, false) {
            self.border_source = borders::Source::Set(name);
        }
    }

    /// Changes the stamp's static. A floor with a floor set holding it as its core puts that set
    /// in the automatic border.
    pub(super) fn pick_static(&mut self, graphic: u16) {
        self.graphic = graphic;
        if let Some(name) = default_set(&self.border_sets, graphic, true) {
            self.border_source = borders::Source::Set(name);
        }
    }

    /// The chosen border set, by name.
    pub(super) fn border_set(&self, name: &str) -> Option<&BorderSet> {
        self.border_sets.iter().find(|s| s.name == name)
    }

    /// Chooses where borders come from. A set brings the brush's land to one of its whole tiles,
    /// when the brush is not one.
    pub(super) fn choose_border_source(&mut self, source: borders::Source) {
        if let borders::Source::Set(name) = &source {
            let (pure, floor) = self
                .border_set(name)
                .map(|s| (s.tiles(terrain::ALL).to_vec(), s.kind == BorderKind::Floor))
                .unwrap_or_default();
            if floor && !pure.is_empty() && !pure.contains(&self.graphic) {
                self.graphic = pure[0];
                self.statics_browse.set_current(self.graphic as u32);
            } else if !floor && !pure.is_empty() && !pure.contains(&self.land) {
                self.land = pure[0];
                self.lands_browse.set_current(self.land as u32);
            }
        }
        self.border_source = source;
    }

    /// The stamp's brush is a floor: a surface static without height (the `floor` of UOIddler's
    /// catalog). A floor paints by dragging, at one height, and with the automatic border gets
    /// its borders inside.
    fn is_floor(&self, assets: &MapAssets, graphic: u16) -> bool {
        assets
            .item(graphic)
            .is_some_and(|t| t.flags & flags::SURFACE != 0 && t.height == 0)
    }

    /// The land borders: the chosen (land) set, or the ones read from the art, with the most
    /// common land around the stroke or with black. A chosen floor set does not serve land:
    /// black it is.
    fn land_transitions(&mut self, assets: &MapAssets, stroke: &[(i32, i32)]) -> terrain::Transitions {
        let land = |x: i32, y: i32| assets.land(x, y).map(|l| l.0);
        let found = match self.border_source.clone() {
            borders::Source::Set(name) => match self.border_set(&name) {
                Some(set) if set.kind != BorderKind::Floor => Some(borders::transitions(set)),
                Some(_) => self.terrain.void_transitions(assets, self.land),
                None => {
                    self.message = Some((tf("borders.missing", &[&name]), true));
                    self.border_source = borders::Source::Detect;
                    None
                }
            },
            borders::Source::Void => self.terrain.void_transitions(assets, self.land),
            borders::Source::Detect => match borders::common_outside(stroke, land, self.land) {
                Some(outside) => self.terrain.transitions(assets, self.land, outside),
                None => self.terrain.void_transitions(assets, self.land),
            },
        };
        found.unwrap_or_default()
    }

    /// Paints land on the stroke (UOIddler's `paintLandTiles`): the brush's draw in the core and,
    /// with the automatic border, the pieces in the ring.
    fn paint_land(&mut self, assets: &MapAssets, stroke: &[(i32, i32)]) -> Vec<MapTile> {
        let set = self
            .auto_border
            .then(|| self.land_transitions(assets, stroke));
        let pool = self.land_pool(assets, set.as_ref());
        let core = |x: i32, y: i32| pool[terrain::roll(x, y) % pool.len()];
        let plan = match &set {
            Some(set) => {
                let inside: HashSet<u16> = std::iter::once(self.land)
                    .chain(set.pure.iter().copied())
                    .chain(pool.iter().copied())
                    .collect();
                let land = |x: i32, y: i32| assets.land(x, y).map(|l| l.0);
                borders::plan(stroke, set, &inside, land, core, terrain::roll)
            }
            None => stroke.iter().map(|&(x, y)| ((x, y), core(x, y))).collect(),
        };
        self.painted(assets, &plan)
    }

    /// The draw for the land core (`borders::land_pool`): without the border pieces of the set in
    /// use nor those of any saved land set, which the admins already said are borders.
    fn land_pool(&mut self, assets: &MapAssets, set: Option<&terrain::Transitions>) -> Vec<u16> {
        let found = if self.vary {
            self.terrain.vary(assets, self.land)
        } else {
            Vec::new()
        };
        let mut pieces: HashSet<u16> = self
            .border_sets
            .iter()
            .filter(|s| s.kind != BorderKind::Floor)
            .flat_map(|s| borders::pieces(&borders::transitions(s)).collect::<Vec<_>>())
            .collect();
        pieces.extend(set.into_iter().flat_map(borders::pieces));
        let chosen = match &self.border_source {
            borders::Source::Set(name) => self
                .border_set(name)
                .filter(|s| s.kind != BorderKind::Floor)
                .map(|s| s.tiles(terrain::ALL).to_vec())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        borders::land_pool(self.land, self.vary, &chosen, &found, &pieces)
    }

    /// Paints floor with the stamp (UOIddler's `paintStatics`): with the automatic border, the
    /// borders inside the stroke (`borders::floor_plan`); without it, the floor only where there
    /// is none yet (`borders::static_plan`). All at the height where the stroke started.
    fn paint_floor(&mut self, assets: &MapAssets, stroke: &Stroke) -> Vec<MapTile> {
        let z = stroke.ground;
        let statics = |x: i32, y: i32| -> Vec<u16> {
            read_tile(assets, x, y)
                .map(|t| {
                    t.statics
                        .iter()
                        .filter(|s| s.z == z)
                        .map(|s| s.graphic)
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut work: BTreeMap<(i32, i32), MapTile> = BTreeMap::new();
        if self.auto_border {
            let chosen = match &self.border_source {
                borders::Source::Set(name) => self
                    .border_set(name)
                    .filter(|s| s.kind == BorderKind::Floor)
                    .map(borders::transitions),
                _ => None,
            };
            let set = match chosen {
                Some(set) => set,
                None => self
                    .terrain
                    .floor_transitions(assets, self.graphic)
                    .unwrap_or_default(),
            };
            let edits = borders::floor_plan(
                &stroke.tiles,
                &set,
                self.graphic,
                self.vary,
                statics,
                terrain::roll,
            );
            for edit in edits {
                let tile = self.work(&mut work, edit.at.0, edit.at.1);
                for gone in edit.remove {
                    take_static(tile, gone, z);
                }
                if tile.add.len() < MAX_PLACED {
                    tile.add.push(PlacedStatic {
                        graphic: edit.add,
                        z,
                        hue: self.hue,
                    });
                }
            }
        } else {
            let mut variants = vec![self.graphic];
            if self.vary {
                if let Some(found) = self.terrain.floor_transitions(assets, self.graphic) {
                    variants.extend(found.pure.iter().filter(|g| **g != self.graphic));
                }
            }
            let plan = borders::static_plan(
                &stroke.tiles,
                self.graphic,
                &variants,
                statics,
                terrain::roll,
            );
            for ((x, y), graphic) in plan {
                let tile = self.work(&mut work, x, y);
                if tile.add.len() < MAX_PLACED {
                    tile.add.push(PlacedStatic {
                        graphic,
                        z,
                        hue: self.hue,
                    });
                }
            }
        }
        work.into_values().collect()
    }

    /// Paints anyway the terrain stroke that went past the height band.
    pub(super) fn roof_anyway(&mut self, assets: &MapAssets) -> Changes {
        let Some((stroke, _)) = self.roof.take() else {
            return Vec::new();
        };
        let changed = self.paint_land(assets, &stroke.tiles);
        self.commit(changed)
    }

    /// Switches to the stamp with the cave floor (0x053B), the right way to enlarge a cave
    /// (UOIddler's `#useCaveFloor`), with its floor set when there is one.
    pub(super) fn use_cave_floor(&mut self) {
        self.roof = None;
        self.graphic = CAVE_FLOOR;
        self.hue = 0;
        self.statics_browse.set_current(CAVE_FLOOR as u32);
        self.choose(Tool::Stamp);
        if let Some(name) = default_set(&self.border_sets, CAVE_FLOOR, true) {
            self.border_source = borders::Source::Set(name);
        }
        self.message = Some((t("map.cave_stamp").into(), false));
    }

    /// The new state of the tiles of a terrain stroke: the land of the plan and, with the fixed
    /// working Z, the ground height (`terrain::painted`).
    fn painted(&self, assets: &MapAssets, plan: &[((i32, i32), u16)]) -> Vec<MapTile> {
        terrain::painted(
            plan,
            self.working_z,
            |x, y| self.edit_of(x, y),
            |x, y| assets.land_z(x, y),
        )
    }

    /// The drawing over the map this frame: the brush outline around the cursor, the stroke in
    /// progress, the stamp's ghosts and the label.
    fn draw(&self, assets: &MapAssets, tile: Option<(i32, i32)>, p: Pointer) -> Overlay {
        let _ = p.ctrl;
        let mut overlay = Overlay {
            grid: self.grid.then_some((self.band.from, self.band.to)),
            // The dragged rectangle, flat on the plane of the start.
            rect: match (self.corner, tile, &self.clip) {
                (Some(a), Some(b), _) if self.drag_plane().is_some() => {
                    Some((rect(a, b), [flat(self.corner_z); 4]))
                }
                (None, Some((x, y)), Some(clip)) if self.pasting && self.tool == Tool::Region => {
                    let area = clip.area_at(x, y);
                    Some((area, self.rect_corners(assets, area)))
                }
                _ => None,
            },
            region: self
                .region
                .filter(|_| self.tool == Tool::Region)
                .map(|area| match self.region_z {
                    Some(z) => (area, [flat(z); 4]),
                    None => (area, self.rect_corners(assets, area)),
                }),
            selected: self
                .selected
                .filter(|_| self.tool == Tool::Select)
                .map(|(x, y)| ((x, y), self.tile_corners(assets, x, y))),
            ..Default::default()
        };
        let Some((x, y)) = tile else {
            return overlay;
        };
        let footprint = if self.tool.brushes() {
            self.brush.footprint(x, y)
        } else {
            vec![(x, y)]
        };
        overlay.outline = brush::outline(&footprint)
            .into_iter()
            .map(|(t, sides)| (t, self.tile_corners(assets, t.0, t.1), sides))
            .collect();
        let painting = self.stroke.as_ref().map(|s| s.tiles.as_slice());
        let tint = |r: f32, g: f32, b: f32| {
            Color32::from_rgba_unmultiplied((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, 89)
        };
        overlay.fill_color = match self.tool {
            Tool::Erase => Some(tint(0.85, 0.36, 0.31)),
            Tool::Height => Some(tint(0.85, 0.66, 0.31)),
            Tool::Terrain => Some(tint(0.42, 0.72, 0.42)),
            _ => None,
        };
        // Terrain with the fixed Z: the stroke draws at the height it will have.
        let fixed = self
            .working_z
            .filter(|_| self.tool == Tool::Terrain)
            .zip(self.stroke.as_ref());
        if let Some((z, stroke)) = fixed {
            let after = terrain::preview_z(&stroke.seen, self.auto_border, z, |x, y| {
                assets.land_z(x, y).unwrap_or(0)
            });
            overlay.fill = stroke
                .tiles
                .iter()
                .map(|&(x, y)| {
                    let corners = [
                        after(x, y),
                        after(x + 1, y),
                        after(x + 1, y + 1),
                        after(x, y + 1),
                    ];
                    ((x, y), corners)
                })
                .collect();
        } else if overlay.fill_color.is_some() {
            overlay.fill = painting
                .unwrap_or_default()
                .iter()
                .map(|&(x, y)| ((x, y), self.tile_corners(assets, x, y)))
                .collect();
        }
        let mut z = self.surface_z(assets, x, y).unwrap_or(0);
        // On terrain, the label tells the height the ground will have: the fixed Z, on the tiles
        // the brush paints (the visible ground, inside the band).
        if let (Tool::Terrain, Some(fixed)) = (self.tool, self.working_z) {
            if assets
                .land_z(x, y)
                .is_some_and(|g| self.band.contains(g as i32))
            {
                z = fixed as i32;
            }
        }
        // Pasting: the ghosts of the clip, with its top corner on the cursor.
        if let (true, Tool::Region, Some(clip)) = (self.pasting, self.tool, &self.clip) {
            for piece in clip.pieces.iter().take(600) {
                let (gx, gy) = (x + piece.dx, y + piece.dy);
                let ground = self.surface_z(assets, gx, gy).unwrap_or(0);
                overlay.ghosts.push(Ghost {
                    graphic: piece.graphic,
                    hue: piece.hue,
                    x: gx,
                    y: gy,
                    z: (ground + piece.dz).clamp(-128, 127),
                });
            }
        }
        if self.tool == Tool::Stamp {
            // The ghosts: on the whole stroke while painting; else, on the brush.
            let tiles = painting.unwrap_or(&footprint);
            for &(gx, gy) in tiles.iter().take(400) {
                let Some(now) = read_tile(assets, gx, gy) else {
                    continue;
                };
                let gz = self.stamp_z(&now, p.alt) as i32;
                if (gx, gy) == (x, y) {
                    z = gz;
                }
                overlay.ghosts.push(Ghost {
                    graphic: self.graphic,
                    hue: self.hue,
                    x: gx,
                    y: gy,
                    z: gz,
                });
            }
        }
        // Dragging a rectangle, the label stays on its plane.
        let mut label_corners = self.tile_corners(assets, x, y);
        if let Some(plane) = self.drag_plane() {
            z = plane;
            label_corners = [flat(plane); 4];
        }
        let mut text = format!("{x}, {y}, {z}");
        if let Some(stroke) = &self.stroke {
            text = format!("{text}  ({})", stroke.tiles.len());
        }
        overlay.label = Some(((x, y), label_corners, text));
        overlay
    }

    /// A change of several tiles in progress: the new state of each, starting from the stored one.
    fn work<'w>(
        &self,
        work: &'w mut BTreeMap<(i32, i32), MapTile>,
        x: i32,
        y: i32,
    ) -> &'w mut MapTile {
        work.entry((x, y)).or_insert_with(|| self.edit_of(x, y))
    }

    /// What a region holds (inside the height band): the statics with their height over the
    /// ground of each tile and, with "take the land", the land.
    fn copy_region(&self, assets: &MapAssets, area: Area) -> region::Clip {
        let mut clip = region::Clip {
            w: area.x1 - area.x0 + 1,
            h: area.y1 - area.y0 + 1,
            ..Default::default()
        };
        for y in area.y0..=area.y1 {
            for x in area.x0..=area.x1 {
                let Some(now) = read_tile(assets, x, y) else {
                    continue;
                };
                let ground = surface(&now, self.band, None);
                let (dx, dy) = (x - area.x0, y - area.y0);
                for s in visible(&now, self.band) {
                    clip.pieces.push(region::Piece {
                        dx,
                        dy,
                        graphic: s.graphic,
                        dz: s.z as i32 - ground,
                        hue: s.hue,
                    });
                }
                if self.with_land {
                    clip.lands.push((dx, dy, now.land.0));
                }
            }
        }
        clip
    }

    /// Takes the statics out of a region (inside the band), in the change in progress.
    fn clear_into(
        &self,
        assets: &MapAssets,
        area: Area,
        work: &mut BTreeMap<(i32, i32), MapTile>,
    ) {
        for y in area.y0..=area.y1 {
            for x in area.x0..=area.x1 {
                let Some(now) = read_tile(assets, x, y) else {
                    continue;
                };
                let gone: Vec<&StaticNow> = visible(&now, self.band).collect();
                if gone.is_empty() {
                    continue;
                }
                let tile = self.work(work, x, y);
                for s in gone {
                    take_static(tile, s.graphic, s.z);
                }
            }
        }
    }

    /// Puts a clip with its top corner at `at`, in the change in progress: each static at the
    /// height of the target tile's ground plus its own.
    fn paste_into(
        &self,
        assets: &MapAssets,
        clip: &region::Clip,
        at: (i32, i32),
        work: &mut BTreeMap<(i32, i32), MapTile>,
    ) {
        for piece in &clip.pieces {
            let (x, y) = (at.0 + piece.dx, at.1 + piece.dy);
            let Some(ground) = self.surface_z(assets, x, y) else {
                continue;
            };
            let tile = self.work(work, x, y);
            if tile.add.len() < MAX_PLACED {
                tile.add.push(PlacedStatic {
                    graphic: piece.graphic,
                    z: (ground + piece.dz).clamp(-128, 127) as i8,
                    hue: piece.hue,
                });
            }
        }
        for &(dx, dy, land) in &clip.lands {
            if assets.land_z(at.0 + dx, at.1 + dy).is_some() {
                self.work(work, at.0 + dx, at.1 + dy).land = Some(land);
            }
        }
    }

    /// Deletes what the chosen region holds (Del).
    pub(super) fn delete_region(&mut self, assets: &MapAssets) -> Changes {
        let Some(area) = self.region else {
            return Vec::new();
        };
        let mut work = BTreeMap::new();
        self.clear_into(assets, area, &mut work);
        self.commit(work.into_values().collect())
    }

    /// Copies the chosen region (Ctrl+C).
    pub(super) fn copy(&mut self, assets: &MapAssets) {
        let Some(area) = self.region else {
            self.message = Some((t("map.nothing_selected").into(), true));
            return;
        };
        let clip = self.copy_region(assets, area);
        self.message = Some((tf("map.copied", &[&clip.pieces.len()]), false));
        self.clip = Some(clip);
    }

    /// Cuts the chosen region (Ctrl+X): copies and deletes.
    pub(super) fn cut(&mut self, assets: &MapAssets) -> Changes {
        self.copy(assets);
        self.delete_region(assets)
    }

    /// Starts pasting the clip (Ctrl+V): it follows the cursor until the click.
    pub(super) fn start_paste(&mut self) {
        if self.clip.as_ref().is_none_or(|c| c.is_empty()) {
            self.message = Some((t("map.nothing_copied").into(), true));
            return;
        }
        self.choose(Tool::Region);
        self.pasting = true;
        self.moving = None;
    }

    /// Starts moving the chosen region: it follows the cursor and leaves where it was on the
    /// click, in one action.
    pub(super) fn start_move(&mut self, assets: &MapAssets) {
        let Some(area) = self.region else {
            return;
        };
        self.clip = Some(self.copy_region(assets, area));
        self.start_paste();
        if self.pasting {
            self.moving = Some(area);
        }
    }

    /// Pastes the clip with its top corner at `at` (moving: takes it from the source in the same
    /// action). The region becomes the pasted one.
    fn paste(&mut self, assets: &MapAssets, at: (i32, i32)) -> Changes {
        let Some(clip) = self.clip.clone() else {
            return Vec::new();
        };
        let mut work = BTreeMap::new();
        if let Some(source) = self.moving.take() {
            self.clear_into(assets, source, &mut work);
            self.pasting = false;
        }
        self.paste_into(assets, &clip, at, &mut work);
        self.region = Some(clip.area_at(at.0, at.1));
        self.region_z = None;
        self.commit(work.into_values().collect())
    }

    /// Moves the chosen region and what it holds (the arrows).
    pub(super) fn shift_region(&mut self, assets: &MapAssets, dx: i32, dy: i32) -> Changes {
        let Some(area) = self.region else {
            return Vec::new();
        };
        let clip = self.copy_region(assets, area);
        let mut work = BTreeMap::new();
        self.clear_into(assets, area, &mut work);
        self.paste_into(assets, &clip, (area.x0 + dx, area.y0 + dy), &mut work);
        self.region = Some(clip.area_at(area.x0 + dx, area.y0 + dy));
        self.commit(work.into_values().collect())
    }

    /// Raises or lowers everything the region holds (PgUp/PgDn).
    pub(super) fn lift_region(&mut self, assets: &MapAssets, by: i32) -> Changes {
        let Some(area) = self.region else {
            return Vec::new();
        };
        let mut work = BTreeMap::new();
        for y in area.y0..=area.y1 {
            for x in area.x0..=area.x1 {
                let Some(now) = read_tile(assets, x, y) else {
                    continue;
                };
                for s in visible(&now, self.band) {
                    let tile = self.work(&mut work, x, y);
                    take_static(tile, s.graphic, s.z);
                    tile.add.push(PlacedStatic {
                        graphic: s.graphic,
                        z: (s.z as i32 + by).clamp(-128, 127) as i8,
                        hue: s.hue,
                    });
                }
            }
        }
        self.commit(work.into_values().collect())
    }

    /// Fills the region with the stamp's static: each tile with the density's chance, empty ones
    /// only when asked, where the stamp would place.
    pub(super) fn fill_region(&mut self, assets: &MapAssets) -> Changes {
        let Some(area) = self.region else {
            return Vec::new();
        };
        self.fill_seed = self.fill_seed.wrapping_add(1);
        let band = self.band;
        let empty_only = self.empty_only;
        let targets = region::fill_targets(area, self.density, self.fill_seed, |x, y| {
            !empty_only
                || read_tile(assets, x, y).is_some_and(|t| visible(&t, band).next().is_none())
        });
        let mut changed = Vec::new();
        for (x, y) in targets {
            let Some(now) = read_tile(assets, x, y) else {
                continue;
            };
            let mut tile = self.edit_of(x, y);
            if tile.add.len() >= MAX_PLACED {
                continue;
            }
            tile.add.push(PlacedStatic {
                graphic: self.graphic,
                z: self.stamp_z(&now, false),
                hue: self.hue,
            });
            changed.push(tile);
        }
        self.commit(changed)
    }

    /// The saved templates, read the first time.
    pub(super) fn templates(&mut self) -> &mut Vec<region::Template> {
        self.templates.get_or_insert_with(region::load_templates)
    }

    /// Saves the chosen region as a template, with the given name.
    pub(super) fn save_template(&mut self, assets: &MapAssets) {
        let name = self.template_name.trim().to_string();
        let Some(area) = self.region.filter(|_| !name.is_empty()) else {
            self.message = Some((t("map.template_needs").into(), true));
            return;
        };
        let clip = self.copy_region(assets, area);
        let templates = self.templates();
        templates.retain(|t| t.name != name);
        templates.push(region::Template {
            name: name.clone(),
            clip,
        });
        templates.sort_by(|a, b| a.name.cmp(&b.name));
        region::save_templates(templates);
        self.template_name.clear();
        self.message = Some((tf("map.template_saved", &[&name]), false));
    }

    /// A template of the palette becomes the clip, ready to paste.
    pub(super) fn use_template(&mut self, name: &str) {
        let clip = self
            .templates()
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.clip.clone());
        if let Some(clip) = clip {
            self.clip = Some(clip);
            self.start_paste();
        }
    }

    /// Deletes a template.
    pub(super) fn delete_template(&mut self, name: &str) {
        let templates = self.templates();
        templates.retain(|t| t.name != name);
        region::save_templates(templates);
    }

    /// The static chosen in the inspector, as the map has it now.
    pub(super) fn picked_static(&self, assets: &MapAssets) -> Option<StaticNow> {
        let (x, y) = self.selected?;
        let (graphic, z) = self.picked?;
        read_tile(assets, x, y)?
            .statics
            .into_iter()
            .rev()
            .find(|s| s.graphic == graphic && s.z == z)
    }

    /// Changes the chosen static: another tile (the arrows), another height (PgUp/PgDn), another
    /// hue or another graphic. Both tiles change in one action, and the choice follows the static.
    pub(super) fn change_picked(
        &mut self,
        assets: &MapAssets,
        to: (i32, i32),
        z: i8,
        hue: u16,
        graphic: u16,
    ) -> Changes {
        let (Some(from), Some(s)) = (self.selected, self.picked_static(assets)) else {
            return Vec::new();
        };
        let mut source = self.edit_of(from.0, from.1);
        take_static(&mut source, s.graphic, s.z);
        let placed = PlacedStatic { graphic, z, hue };
        let changed = if to == from {
            source.add.push(placed);
            vec![source]
        } else {
            let mut target = self.edit_of(to.0, to.1);
            target.add.push(placed);
            vec![source, target]
        };
        self.selected = Some(to);
        self.picked = Some((graphic, z));
        self.commit(changed)
    }

    /// Deletes the chosen static (Del).
    pub(super) fn delete_picked(&mut self, assets: &MapAssets) -> Changes {
        let (Some((x, y)), Some(s)) = (self.selected, self.picked_static(assets)) else {
            return Vec::new();
        };
        let mut tile = self.edit_of(x, y);
        take_static(&mut tile, s.graphic, s.z);
        self.picked = None;
        self.commit(vec![tile])
    }

    /// Moves the chosen static one tile (the arrows, on the screen's axes).
    pub(super) fn nudge_picked(&mut self, assets: &MapAssets, dx: i32, dy: i32) -> Changes {
        let (Some((x, y)), Some(s)) = (self.selected, self.picked_static(assets)) else {
            return Vec::new();
        };
        self.change_picked(assets, (x + dx, y + dy), s.z, s.hue, s.graphic)
    }

    /// Raises or lowers the chosen static (PgUp/PgDn); without one, the working Z.
    pub(super) fn lift(&mut self, assets: &MapAssets, by: i32) -> Changes {
        match (self.selected, self.picked_static(assets)) {
            (Some(at), Some(s)) => {
                let z = (s.z as i32 + by).clamp(-128, 127) as i8;
                self.change_picked(assets, at, z, s.hue, s.graphic)
            }
            _ => {
                let base = self
                    .working_z
                    .map(i32::from)
                    .or_else(|| self.hover.and_then(|(x, y)| self.surface_z(assets, x, y)))
                    .unwrap_or(0);
                self.working_z = Some((base + by).clamp(-128, 127) as i8);
                Vec::new()
            }
        }
    }

    /// The chosen static becomes the stamp's (graphic and hue).
    pub(super) fn use_as_brush(&mut self, assets: &MapAssets) {
        if let Some(s) = self.picked_static(assets) {
            self.graphic = s.graphic;
            self.hue = s.hue;
            self.statics_browse.set_current(s.graphic as u32);
            self.choose(Tool::Stamp);
        }
    }

    /// Chooses the tool.
    pub(super) fn choose(&mut self, tool: Tool) {
        // The palette shows the tool's brush: objects on the stamp, lands on terrain.
        match tool {
            Tool::Stamp if self.palette_mode != PaletteMode::Objects => {
                self.palette_mode = PaletteMode::Objects;
                self.statics_browse.reveal();
            }
            Tool::Terrain if self.palette_mode != PaletteMode::Lands => {
                self.palette_mode = PaletteMode::Lands;
                self.lands_browse.reveal();
            }
            _ => {}
        }
        self.tool = tool;
        if tool != Tool::Region {
            self.pasting = false;
            self.moving = None;
        }
        self.corner = None;
        self.stroke = None;
        if tool != Tool::Select {
            self.picked = None;
        }
    }
}
