//! UOFiddler: Ultra-fast native Rust rewrite of UOFiddler.
//! Native multi-platform (Linux, Windows, macOS) client viewer and editor for Ultima Online.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use uo_core::*;

mod files_tab;
mod map;
mod packets_tab;
mod paperdoll_tab;

/// All 38 bitfield flags defined in Ultima Online TileData
pub const TILEDATA_FLAGS: [(&str, u64); 38] = [
    ("Background", 1 << 0),
    ("Weapon", 1 << 1),
    ("Transparent", 1 << 2),
    ("Translucent", 1 << 3),
    ("Wall", 1 << 4),
    ("Damaging", 1 << 5),
    ("Impassable", 1 << 6),
    ("Wet", 1 << 7),
    ("Unknown1", 1 << 8),
    ("Surface", 1 << 9),
    ("Bridge", 1 << 10),
    ("Generic", 1 << 11),
    ("Window", 1 << 12),
    ("NoShoot", 1 << 13),
    ("PrefixA", 1 << 14),
    ("PrefixAn", 1 << 15),
    ("Internal", 1 << 16),
    ("Foliage", 1 << 17),
    ("PartialHue", 1 << 18),
    ("Unknown2", 1 << 19),
    ("Map", 1 << 20),
    ("Container", 1 << 21),
    ("Wearable", 1 << 22),
    ("LightSource", 1 << 23),
    ("Animated", 1 << 24),
    ("NoDiagonal", 1 << 25),
    ("Unknown3", 1 << 26),
    ("Armor", 1 << 27),
    ("Roof", 1 << 28),
    ("Door", 1 << 29),
    ("StairBack", 1 << 30),
    ("StairRight", 1 << 31),
    ("AlphaBlend", 1 << 32),
    ("UseNewArt", 1 << 33),
    ("ArtUsed", 1 << 34),
    ("NoHouse", 1 << 35),
    ("NoDraw", 1 << 36),
    ("HoverOver", 1 << 37),
];

/// How the Map tab draws the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapMode {
    /// Top-down minimap coloured with radarcol.mul
    Radar,
    /// Isometric rendering with the real art, like the game client
    Live,
    /// Whole-facet overview image (facetNN.mul / Multimap.rle)
    Overview,
}

/// All 17 functional tabs supported by UOFiddler (TileData integrated directly into Items & Land)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UoTab {
    Items,
    LandTiles,
    Gumps,
    Hues,
    Textures,
    RadarCol,
    Animations,
    Cliloc,
    Sounds,
    Map,
    Multis,
    Light,
    Fonts,
    Skills,
    Speech,
    AnimData,
    GameData,
    Packets,
    Files,
    Paperdoll,
}

impl UoTab {
    pub const ALL: [UoTab; 20] = [
        UoTab::Items,
        UoTab::LandTiles,
        UoTab::Gumps,
        UoTab::Hues,
        UoTab::Textures,
        UoTab::RadarCol,
        UoTab::Animations,
        UoTab::Cliloc,
        UoTab::Sounds,
        UoTab::Map,
        UoTab::Multis,
        UoTab::Light,
        UoTab::Fonts,
        UoTab::Skills,
        UoTab::Speech,
        UoTab::AnimData,
        UoTab::GameData,
        UoTab::Packets,
        UoTab::Files,
        UoTab::Paperdoll,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            UoTab::Items => "📦 Items",
            UoTab::LandTiles => "🌱 Land Tiles",
            UoTab::Gumps => "🖼 Gumps",
            UoTab::Hues => "🎨 Hues",
            UoTab::Textures => "🧱 Textures",
            UoTab::RadarCol => "🎯 RadarCol",
            UoTab::Animations => "🐉 Animations",
            UoTab::Cliloc => "💬 Cliloc",
            UoTab::Sounds => "🔊 Sounds",
            UoTab::Map => "🗺 Map",
            UoTab::Multis => "🏰 Multis",
            UoTab::Light => "💡 Light",
            UoTab::Fonts => "🔤 Fonts",
            UoTab::Skills => "⚔ Skills",
            UoTab::Speech => "🗣 Speech",
            UoTab::AnimData => "⏱ AnimData",
            UoTab::GameData => "🎮 GameData",
            UoTab::Packets => "📡 Comunicação",
            UoTab::Files => "📁 Arquivos",
            UoTab::Paperdoll => "🧍 Paperdoll",
        }
    }

    pub fn category(&self) -> TabCategory {
        match self {
            UoTab::Items
            | UoTab::LandTiles
            | UoTab::Textures
            | UoTab::Gumps
            | UoTab::Animations
            | UoTab::Paperdoll
            | UoTab::Multis => TabCategory::Art,

            UoTab::Hues
            | UoTab::RadarCol
            | UoTab::Cliloc
            | UoTab::AnimData
            | UoTab::GameData
            | UoTab::Packets
            | UoTab::Files => TabCategory::Data,

            UoTab::Map
            | UoTab::Light
            | UoTab::Sounds
            | UoTab::Speech
            | UoTab::Fonts
            | UoTab::Skills => TabCategory::World,
        }
    }
}

/// Logical grouping of UOFiddler tabs for intuitive modern navigation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabCategory {
    Art,
    Data,
    World,
}

impl TabCategory {
    pub const ALL: [TabCategory; 3] = [
        TabCategory::Art,
        TabCategory::Data,
        TabCategory::World,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            TabCategory::Art => "🎨 Arte & Gráficos",
            TabCategory::Data => "📋 Dados & Tabelas",
            TabCategory::World => "🗺️ Mundo & Áudio",
        }
    }
}

/// Draws an authentic transparent checkerboard background (Aseprite / Photoshop style)
pub fn draw_checkerboard(painter: &egui::Painter, rect: egui::Rect, cell_size: f32) {
    let c1 = egui::Color32::from_rgb(18, 19, 24);
    let c2 = egui::Color32::from_rgb(28, 30, 38);
    let cols = ((rect.width() / cell_size).ceil() as usize) + 1;
    let rows = ((rect.height() / cell_size).ceil() as usize) + 1;

    for y in 0..rows {
        for x in 0..cols {
            let color = if (x + y) % 2 == 0 { c1 } else { c2 };
            let min = egui::pos2(
                (rect.min.x + x as f32 * cell_size).min(rect.max.x),
                (rect.min.y + y as f32 * cell_size).min(rect.max.y),
            );
            let max = egui::pos2(
                (min.x + cell_size).min(rect.max.x),
                (min.y + cell_size).min(rect.max.y),
            );
            if min.x < max.x && min.y < max.y {
                painter.rect_filled(egui::Rect::from_min_max(min, max), 0.0, color);
            }
        }
    }
}

/// Quick PNG exporter for any RGBA image into 'exported_art/' directory
pub fn export_rgba_png(width: u32, height: u32, rgba: &[u8], filename: &str) -> Option<String> {
    let dir = std::path::Path::new("exported_art");
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join(filename);

    if let Some(img) = image::RgbaImage::from_raw(width, height, rgba.to_vec()) {
        if img.save(&path).is_ok() {
            return Some(path.display().to_string());
        }
    }
    None
}

/// Parses Hex (`0x1F00` or `1F00`) or Decimal string into an item/element index
pub fn parse_id_input(text: &str) -> Option<usize> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(stripped) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        usize::from_str_radix(stripped, 16).ok()
    } else {
        t.parse::<usize>().or_else(|_| usize::from_str_radix(t, 16)).ok()
    }
}

/// Modern sleek dark visual theme (Obsidian / Indigo Palette)
pub fn setup_custom_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();

    visuals.dark_mode = true;
    visuals.override_text_color = Some(egui::Color32::from_rgb(226, 232, 240));

    // Panels and Windows
    visuals.panel_fill = egui::Color32::from_rgb(17, 18, 23);
    visuals.window_fill = egui::Color32::from_rgb(22, 24, 31);
    visuals.window_stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(45, 48, 62));
    visuals.window_rounding = egui::Rounding::same(8.0);
    visuals.extreme_bg_color = egui::Color32::from_rgb(12, 13, 17);

    // Inactive Widgets
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(26, 28, 37);
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgba_premultiplied(26, 28, 37, 0);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(42, 45, 58));
    visuals.widgets.inactive.rounding = egui::Rounding::same(5.0);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(203, 213, 225));

    // Hovered Widgets
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(38, 41, 54);
    visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(34, 37, 48);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(99, 102, 241));
    visuals.widgets.hovered.rounding = egui::Rounding::same(5.0);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);

    // Active Widgets (Pressed)
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(79, 70, 229);
    visuals.widgets.active.weak_bg_fill = egui::Color32::from_rgb(67, 56, 202);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(129, 140, 248));
    visuals.widgets.active.rounding = egui::Rounding::same(5.0);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5_f32, egui::Color32::WHITE);

    // Menus & Open Dropdowns
    visuals.widgets.open.bg_fill = egui::Color32::from_rgb(28, 30, 40);
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(99, 102, 241));
    visuals.widgets.open.rounding = egui::Rounding::same(5.0);

    // Selection
    visuals.selection.bg_fill = egui::Color32::from_rgb(49, 46, 129);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(129, 140, 248));

    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.window_margin = egui::Margin::same(12.0);
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.bar_inner_margin = 2.0;
    ctx.set_style(style);
}

pub const ACTIONS_MONSTER: [(u32, &str); 17] = [
    (0, "Walk"),
    (1, "Stand / Idle"),
    (2, "Die 1"),
    (3, "Die 2"),
    (4, "Attack 1"),
    (5, "Attack 2"),
    (6, "Attack 3"),
    (7, "Cast / Special 1"),
    (8, "Cast / Special 2"),
    (9, "Get Hit"),
    (10, "Block Right"),
    (11, "Block Left"),
    (12, "Idle"),
    (13, "Fidget"),
    (14, "Fly"),
    (15, "Take Off"),
    (16, "Get Hit In Air"),
];

pub const ACTIONS_ANIMAL: [(u32, &str); 13] = [
    (0, "Walk"),
    (1, "Run"),
    (2, "Idle 1"),
    (3, "Eat"),
    (4, "Alert"),
    (5, "Attack 1"),
    (6, "Attack 2"),
    (7, "Get Hit"),
    (8, "Die 1"),
    (9, "Idle 2"),
    (10, "Fidget"),
    (11, "Lie Down"),
    (12, "Die 2"),
];

pub const ACTIONS_SEA: [(u32, &str); 9] = [
    (0, "Walk"),
    (1, "Run"),
    (2, "Idle 1"),
    (3, "Idle 2"),
    (4, "Fidget"),
    (5, "Attack 1"),
    (6, "Attack 2"),
    (7, "Get Hit"),
    (8, "Die 1"),
];

pub const ACTIONS_HUMAN: [(u32, &str); 37] = [
    (0, "Walk_01"),
    (1, "WalkStaff_01"),
    (2, "Run_01"),
    (3, "RunStaff_01"),
    (4, "Idle_01"),
    (5, "Idle_02"),
    (6, "Fidget_Yawn_Stretch_01"),
    (7, "CombatIdle1H_01"),
    (8, "CombatIdle2H_01"),
    (9, "AttackSlash1H_01"),
    (10, "AttackPierce1H_01"),
    (11, "AttackBash1H_01"),
    (12, "AttackBash2H_01"),
    (13, "AttackSlash2H_01"),
    (14, "AttackPierce2H_01"),
    (15, "CombatAdvance_1H_01"),
    (16, "Spell1"),
    (17, "Spell2"),
    (18, "AttackBow_01"),
    (19, "AttackCrossbow_01"),
    (20, "GetHit_Fr_Hi_01"),
    (21, "GetHit_Fr_Med_01"),
    (22, "GetHit_Fr_Low_01"),
    (23, "Block_Shield_01"),
    (24, "Punch_01"),
    (25, "Bow_01"),
    (26, "Salute_01"),
    (27, "Ingest_Eat_01"),
    (28, "Horse_Walk_01"),
    (29, "Horse_Run_01"),
    (30, "Horse_Idle_01"),
    (31, "Horse_Attack1H_01"),
    (32, "Horse_AttackBow_01"),
    (33, "Horse_AttackCrossbow_01"),
    (34, "Horse_Spell2_01"),
    (35, "Die_Hard_Fwd_01"),
    (36, "Die_Hard_Back_01"),
];

pub const POPULAR_BODIES: [(u32, &str); 20] = [
    (400, "400 - Humano Masculino"),
    (401, "401 - Humana Feminina"),
    (9, "9 - Daemon (Demônio)"),
    (12, "12 - Dragon Red (Dragão)"),
    (59, "59 - Dragon"),
    (200, "200 - Horse (Cavalo)"),
    (21, "21 - Lizardman"),
    (18, "18 - Ettin"),
    (3, "3 - Zombie"),
    (4, "4 - Gargoyle"),
    (5, "5 - Eagle"),
    (7, "7 - Orc"),
    (8, "8 - Corpser"),
    (15, "15 - Giant Snake"),
    (16, "16 - Gazer"),
    (28, "28 - Giant Spider"),
    (48, "48 - Scorpion"),
    (50, "50 - Skeleton"),
    (51, "51 - Slime"),
    (79, "79 - Lich"),
];

pub const DIRECTIONS: [(u8, &str); 8] = [
    (0, "0 - South (Sul)"),
    (1, "1 - South-West (Sudoeste)"),
    (2, "2 - West (Oeste)"),
    (3, "3 - North-West (Noroeste)"),
    (4, "4 - North (Norte)"),
    (5, "5 - North-East (Nordeste - Espelhado)"),
    (6, "6 - East (Leste - Espelhado)"),
    (7, "7 - South-East (Sudeste - Espelhado)"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimCategoryFilter {
    All,
    Monsters,    // 0
    SeaMonsters, // 1
    Animals,     // 2
    Humanoids,   // 3
    Equipment,   // is_equip
    UopModern,   // is_uop
}

pub const PRESET_LOCATIONS: [(usize, &str, u32, u32); 32] = [
    // Felucca (0)
    (0, "Britain Bank [Felucca]", 1420, 1690),
    (0, "Lord British's Castle", 1390, 1625),
    (0, "West Britain Bank", 1435, 1680),
    (0, "Moonglow Bank", 4470, 1175),
    (0, "Trinsic Bank", 1815, 2820),
    (0, "Yew Bank", 540, 980),
    (0, "Minoc Bank", 2470, 540),
    (0, "Skara Brae", 630, 2230),
    (0, "Jhelom", 1400, 3810),
    (0, "Magincia", 3730, 2160),
    (0, "Buccaneer's Den", 2705, 2160),
    (0, "Vesper Bank", 2895, 675),
    (0, "Cove", 2240, 1210),
    (0, "Ocllo", 3650, 2520),
    (0, "Nujel'm", 3750, 1270),
    (0, "Delucia [Lost Lands]", 5260, 3990),
    (0, "Papua [Lost Lands]", 5730, 3210),
    (0, "Wind", 5165, 245),
    // Trammel (1)
    (1, "Britain Bank [Trammel]", 1420, 1690),
    (1, "Lord British's Castle", 1390, 1625),
    (1, "Moonglow Bank", 4470, 1175),
    (1, "Trinsic Bank", 1815, 2820),
    (1, "Yew Bank", 540, 980),
    (1, "Minoc Bank", 2470, 540),
    (1, "Haven / New Haven", 3500, 2570),
    // Ilshenar (2)
    (2, "Ilshenar Entrance", 845, 640),
    (2, "Gargoyle City", 850, 680),
    // Malas (3)
    (3, "Luna Bank [Malas]", 990, 520),
    (3, "Umbra Bank [Malas]", 2090, 1340),
    // Tokuno (4)
    (4, "Zento Bank [Tokuno]", 730, 1245),
    // Ter Mur (5)
    (5, "Royal City [Ter Mur]", 850, 3520),
    (5, "Holy City", 995, 3975),
];

pub struct UoFiddlerApp {
    file_manager: FileManager,
    active_tab: UoTab,

    // Modern UI & Navigation State
    active_category: Option<TabCategory>,
    preview_checkerboard: bool,
    item_jump_input: String,
    land_jump_input: String,
    gump_jump_input: String,
    texture_jump_input: String,

    // Keyboard navigation state
    keyboard_navigated: bool,

    // Splash screen state
    splash_active: bool,
    splash_start: Instant,
    splash_progress: f32,
    splash_msg: &'static str,

    // Settings dialog state
    show_settings_dialog: bool,
    show_about_dialog: bool,
    settings_classic_input: String,
    settings_ec_input: String,
    settings_high_seas: bool,

    // Core Data & File Indices
    art_index: Option<FileIndex>,
    ec_art_index: EcArtIndex,
    multi_index: Option<FileIndex>,
    multi_record_size: usize,
    gump_index: Option<FileIndex>,
    gump_index_ec: Option<FileIndex>,
    texture_index: Option<FileIndex>,
    light_index: Option<FileIndex>,
    sound_index: Option<FileIndex>,
    skills_index: Option<FileIndex>,
    anim_indices: HashMap<u32, FileIndex>,
    anim_resolver: ClassicAnimResolver,
    equip_gump: EquipGumpMap,
    packets_tab: packets_tab::PacketsTab,
    files_tab: files_tab::FilesTab,
    paperdoll_tab: paperdoll_tab::PaperdollTab,
    uop_anim_index: UopAnimationIndex,
    tiledata: Option<TileDataManager>,
    hues: Option<HuesManager>,
    radar_col: Option<RadarColManager>,
    cliloc: Option<ClilocManager>,
    fonts_mgr: Option<FontsManager>,
    speech_mgr: Option<SpeechManager>,
    animdata_mgr: Option<AnimDataManager>,
    map_mgr: MapManager,

    // Pre-indexed IDs for instant browsing
    valid_gumps: Vec<u32>,
    valid_textures: Vec<u32>,
    valid_sounds: Vec<SoundEntry>,
    valid_lights: Vec<u32>,

    // Items Tab state
    item_mode_enhanced: bool,
    item_search: String,
    item_selected: usize,
    item_texture: Option<egui::TextureHandle>,
    item_hue: u32,
    item_zoom: f32,
    item_view_mode: bool, // false = Grid, true = List
    item_thumb_cache: HashMap<usize, egui::TextureHandle>,

    // Land Tiles Tab state
    land_mode_enhanced: bool,
    land_search: String,
    land_selected: usize,
    land_texture: Option<egui::TextureHandle>,
    land_zoom: f32,
    land_view_mode: bool, // false = Grid, true = List
    land_thumb_cache: HashMap<usize, egui::TextureHandle>,

    // Animations Tab state
    anim_mode_enhanced: bool,
    anim_body_defs: Vec<AnimationDef>,
    anim_search: String,
    anim_category_filter: AnimCategoryFilter,
    anim_body: u32,
    anim_action: u32,
    anim_direction: u8,
    anim_fps: u32,
    anim_playing: bool,
    anim_last_frame_time: Instant,
    anim_current_frame_idx: usize,
    anim_frames: Vec<AnimationFrame>,
    anim_texture: Option<egui::TextureHandle>,
    anim_zoom: f32,

    // Multis Tab state
    multi_id: u32,
    multi_max_z: i16,
    multi_texture: Option<egui::TextureHandle>,
    multi_zoom: f32,
    multi_pan: egui::Vec2,
    multi_mode_enhanced: bool,
    multi_search: String,
    uop_multis: HashMap<u32, MultiStructure>,
    multilist_entries: Vec<MultilistEntry>,
    valid_multis: Vec<u32>,
    current_multi_structure: Option<MultiStructure>,

    // Gumps Tab state
    gump_mode_enhanced: bool,
    gump_id: u32,
    gump_search: String,
    gump_texture: Option<egui::TextureHandle>,
    gump_dim: (u32, u32),
    gump_zoom: f32,

    // Hues Tab state
    hue_search: String,
    hue_selected: usize,

    // Cliloc Tab state
    cliloc_search: String,

    // Textures Tab state
    texture_id: u32,
    texture_search: String,
    texture_texture: Option<egui::TextureHandle>,
    texture_dim: u32,
    texture_zoom: f32,

    // RadarCol Tab state
    radar_is_items: bool,
    radar_search: String,
    radar_selected: usize,
    radar_preview_tex: Option<egui::TextureHandle>,

    // Sounds Tab state
    sound_search: String,
    sound_selected: usize,

    // Light Tab state
    light_id: u32,
    light_texture: Option<egui::TextureHandle>,
    light_dim: (u32, u32),

    // Fonts Tab state
    font_selected: usize,
    font_preview_text: String,
    font_texture: Option<egui::TextureHandle>,

    // Speech Tab state
    speech_search: String,
    #[allow(dead_code)]
    speech_selected: Option<usize>,
    #[allow(dead_code)]
    speech_edit_kw: String,

    // AnimData Tab state
    animdata_selected: Option<u32>,
    animdata_preview_tex: Option<egui::TextureHandle>,

    // Map Tab state
    map_id: usize,
    map_x: u32,
    map_y: u32,
    map_zoom: f32,
    map_texture: Option<egui::TextureHandle>,
    map_show_statics: bool,
    map_last_render_size: egui::Vec2,
    map_mode: MapMode,
    map_use_diff: bool,
    map_max_z: i32,
    map_panel: map::MapPanel,
    overview_tex: Option<(usize, bool, egui::TextureHandle, u32, u32)>,
    overview_multimap: bool,

    // GameData Tab state
    gamedata_archive: GameDataArchive,
    gamedata_search: String,
    gamedata_selected_idx: usize,
    gamedata_row_search: String,
    gamedata_view_raw: bool,

    status_message: String,
}

impl Default for UoFiddlerApp {
    fn default() -> Self {
        let fm = FileManager::new();
        let classic_str = fm
            .uo_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let ec_str = fm
            .uo_ec_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let hs = fm.high_seas;
        let anim_body_defs = load_animation_defs(&fm);
        let uop_multis = if let Some(p) = fm.get_file_path("MultiCollection.uop") {
            load_uop_multis(p)
        } else {
            HashMap::new()
        };
        let multilist_entries = load_multilist_entries(&fm);
        let mut gamedata_archive = GameDataArchive::new();
        if let Some(ec_dir) = &fm.uo_ec_dir {
            gamedata_archive.load_directory(ec_dir);
        } else if let Some(uo_dir) = &fm.uo_dir {
            gamedata_archive.load_directory(uo_dir);
        }

        let mut app = Self {
            file_manager: fm,
            // UOFIDDLER_START_TAB=<tab name> selects the initial tab (used for automated screenshots)
            active_tab: std::env::var("UOFIDDLER_START_TAB")
                .ok()
                .and_then(|n| {
                    UoTab::ALL.iter().copied().find(|t| {
                        format!("{:?}", t).eq_ignore_ascii_case(n.trim())
                    })
                })
                .unwrap_or(UoTab::Items),
            active_category: Some(TabCategory::Art),
            preview_checkerboard: false,
            item_jump_input: String::new(),
            land_jump_input: String::new(),
            gump_jump_input: String::new(),
            texture_jump_input: String::new(),
            keyboard_navigated: false,
            splash_active: true,
            splash_start: Instant::now(),
            splash_progress: 0.0,
            splash_msg: "Verificando diretório do Ultima Online...",
            show_settings_dialog: false,
            show_about_dialog: false,
            settings_classic_input: classic_str,
            settings_ec_input: ec_str,
            settings_high_seas: hs,
            art_index: None,
            ec_art_index: EcArtIndex::new(),
            multi_index: None,
            multi_record_size: 16,
            gump_index: None,
            gump_index_ec: None,
            texture_index: None,
            light_index: None,
            sound_index: None,
            skills_index: None,
            anim_indices: HashMap::new(),
            anim_resolver: ClassicAnimResolver::default(),
            equip_gump: EquipGumpMap::default(),
            packets_tab: packets_tab::PacketsTab::default(),
            files_tab: files_tab::FilesTab::default(),
            paperdoll_tab: paperdoll_tab::PaperdollTab::default(),
            uop_anim_index: UopAnimationIndex::new(),
            tiledata: None,
            hues: None,
            radar_col: None,
            cliloc: None,
            fonts_mgr: None,
            speech_mgr: None,
            animdata_mgr: None,
            map_mgr: MapManager::new(),
            valid_gumps: Vec::new(),
            valid_textures: Vec::new(),
            valid_sounds: Vec::new(),
            valid_lights: Vec::new(),
            item_mode_enhanced: false,
            item_search: String::new(),
            item_selected: 2, // Ankh by default
            item_texture: None,
            item_hue: 0,
            item_zoom: 1.0_f32,
            item_view_mode: false, // Grid mode by default
            item_thumb_cache: HashMap::new(),
            land_mode_enhanced: false,
            land_search: String::new(),
            land_selected: 2,
            land_texture: None,
            land_zoom: 1.0_f32,
            land_view_mode: false, // Grid mode by default
            land_thumb_cache: HashMap::new(),
            anim_mode_enhanced: false,
            anim_body_defs,
            anim_search: String::new(),
            anim_category_filter: AnimCategoryFilter::All,
            anim_body: 9,   // Daemon
            anim_action: 1, // Stand / Idle
            anim_direction: 0,
            anim_fps: 10,
            anim_playing: true,
            anim_last_frame_time: Instant::now(),
            anim_current_frame_idx: 0,
            anim_frames: Vec::new(),
            anim_texture: None,
            anim_zoom: 1.0_f32,
            multi_id: 0,
            multi_max_z: 100,
            multi_texture: None,
            multi_zoom: 1.0_f32,
            multi_pan: egui::Vec2::ZERO,
            multi_mode_enhanced: false,
            multi_search: String::new(),
            uop_multis,
            multilist_entries,
            valid_multis: Vec::new(),
            current_multi_structure: None,
            gump_mode_enhanced: false,
            gump_id: 0,
            gump_search: String::new(),
            gump_texture: None,
            gump_dim: (0, 0),
            gump_zoom: 1.0_f32,
            hue_search: String::new(),
            hue_selected: 0,
            cliloc_search: String::new(),
            texture_id: 1,
            texture_search: String::new(),
            texture_texture: None,
            texture_dim: 64,
            texture_zoom: 1.0_f32,
            radar_is_items: true,
            radar_search: String::new(),
            radar_selected: 2,
            radar_preview_tex: None,
            sound_search: String::new(),
            sound_selected: 0,
            light_id: 0,
            light_texture: None,
            light_dim: (0, 0),
            font_selected: 0,
            font_preview_text: "The quick brown fox jumps over the lazy dog! 1234567890"
                .to_string(),
            font_texture: None,
            speech_search: String::new(),
            speech_selected: None,
            speech_edit_kw: String::new(),
            animdata_selected: None,
            animdata_preview_tex: None,
            map_id: 0,
            map_x: 1420, // Britain Bank
            map_y: 1690,
            map_zoom: 1.0_f32,
            map_texture: None,
            map_show_statics: true,
            map_last_render_size: egui::Vec2::ZERO,
            map_mode: MapMode::Radar,
            map_use_diff: true,
            map_max_z: 127,
            map_panel: map::MapPanel::default(),
            overview_tex: None,
            overview_multimap: false,
            gamedata_archive,
            gamedata_search: String::new(),
            gamedata_selected_idx: 0,
            gamedata_row_search: String::new(),
            gamedata_view_raw: false,
            status_message: "Iniciando UOFiddler...".to_string(),
        };

        app.refresh_valid_multis();
        // UOFIDDLER_MAP=<radar|live|overview>,<facet>,<x>,<y>[,nodiff] sets the Map tab state (automated screenshots)
        if let Ok(v) = std::env::var("UOFIDDLER_MAP") {
            let p: Vec<&str> = v.split(',').map(|x| x.trim()).collect();
            app.map_mode = match p.first().copied() {
                Some("live") => MapMode::Live,
                Some("overview") => MapMode::Overview,
                _ => MapMode::Radar,
            };
            if let Some(n) = p.get(1).and_then(|x| x.parse().ok()) {
                app.map_id = n;
            }
            if let Some(n) = p.get(2).and_then(|x| x.parse().ok()) {
                app.map_x = n;
            }
            if let Some(n) = p.get(3).and_then(|x| x.parse().ok()) {
                app.map_y = n;
            }
            app.map_use_diff = !p.contains(&"nodiff");
            // UOFIDDLER_EDITOR=<tool>[/x,y][/...] opens the map editor (see MapEditor::focus).
            if let Ok(spec) = std::env::var("UOFIDDLER_EDITOR") {
                app.map_panel.open_editor(app.map_id);
                app.map_panel.editor.focus(&spec);
            }
            // Isometric view options: zoom=<f>, maxz=<z>, noroofs, grid
            for opt in &p[1..] {
                if let Some(z) = opt.strip_prefix("zoom=").and_then(|z| z.parse().ok()) {
                    app.map_panel.view.zoom = z;
                } else if let Some(z) = opt.strip_prefix("maxz=").and_then(|z| z.parse().ok()) {
                    app.map_max_z = z;
                    app.map_panel.view.cut.cut = Some(z);
                } else if *opt == "noroofs" {
                    app.map_panel.view.cut.hide_roofs = true;
                } else if *opt == "grid" {
                    app.map_panel.view.show_grid = true;
                }
            }
        }
        app
    }
}

impl UoFiddlerApp {
    fn refresh_valid_multis(&mut self) {
        let mut list = Vec::new();
        if self.multi_mode_enhanced {
            for &id in self.uop_multis.keys() {
                list.push(id);
            }
        } else if let Some(mi) = &self.multi_index {
            for id in 0..0x4000 {
                if mi.is_valid(id) {
                    list.push(id as u32);
                }
            }
        }
        list.sort_unstable();
        self.valid_multis = list;
        if !self.valid_multis.is_empty() && !self.valid_multis.contains(&self.multi_id) {
            self.multi_id = self.valid_multis[0];
        }
    }

    fn get_multi_name(&self, id: u32) -> Option<String> {
        self.multilist_entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.name.clone())
    }

    fn refresh_valid_gumps(&mut self) {
        let gi = if self.gump_mode_enhanced {
            self.gump_index_ec.as_ref().or(self.gump_index.as_ref())
        } else {
            self.gump_index.as_ref()
        };
        if let Some(gi) = gi {
            self.valid_gumps = (0..0xFFFF)
                .filter(|&i| gi.is_valid(i))
                .map(|i| i as u32)
                .collect();
            if !self.valid_gumps.is_empty() && !self.valid_gumps.contains(&self.gump_id) {
                self.gump_id = self.valid_gumps[0];
            }
        }
    }

    fn get_item_bitmap(&self, item_id: usize) -> Option<ArtBitmap> {
        if self.item_mode_enhanced {
            if let Some(bmp) = self.ec_art_index.get_static(item_id as u32) {
                return Some(bmp);
            }
        }
        if let Some(art) = &self.art_index {
            let static_id = 0x4000 + item_id;
            if let Some(data) = art.get_data(static_id) {
                return decode_static(&data);
            }
        }
        None
    }

    fn get_land_bitmap(&self, land_id: usize) -> Option<ArtBitmap> {
        if self.land_mode_enhanced {
            if let Some(td) = &self.tiledata {
                if let Some(tile) = td.land_tiles.get(land_id) {
                    if tile.texture_id > 0 {
                        if let Some(ti) = &self.texture_index {
                            if let Some(data) = ti.get_data(tile.texture_id as usize) {
                                let extra = ti
                                    .entries
                                    .get(tile.texture_id as usize)
                                    .map(|e| e.extra)
                                    .unwrap_or(0);
                                if let Some(tex) = decode_texture(&data, extra) {
                                    return Some(ArtBitmap {
                                        width: tex.size as u16,
                                        height: tex.size as u16,
                                        rgba: tex.rgba,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(art) = &self.art_index {
            if let Some(data) = art.get_data(land_id) {
                return decode_land(&data);
            }
        }
        None
    }

    fn get_current_gump_bitmap(&self) -> Option<GumpBitmap> {
        let gi_opt = if self.gump_mode_enhanced {
            self.gump_index_ec.as_ref().or(self.gump_index.as_ref())
        } else {
            self.gump_index.as_ref()
        };

        if let Some(gi) = gi_opt {
            let id = self.gump_id as usize;
            if let Some(data) = gi.get_data(id) {
                let entry = gi.entries[id];
                let (w, h, payload) =
                    if gi.is_uop() || entry.compression == 1 || entry.compression == 3 {
                        if data.len() >= 8 {
                            let w = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                            let h = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
                            (w, h, &data[8..])
                        } else {
                            (0, 0, &data[..])
                        }
                    } else {
                        let w = ((entry.extra >> 16) & 0xFFFF) as u32;
                        let h = (entry.extra & 0xFFFF) as u32;
                        (w, h, &data[..])
                    };

                return decode_gump(payload, w, h);
            }
        }
        None
    }

    fn get_current_texture_bitmap(&self) -> Option<ArtBitmap> {
        if let Some(ti) = &self.texture_index {
            let id = self.texture_id as usize;
            if let Some(data) = ti.get_data(id) {
                let entry = ti.entries[id];
                if let Some(bmp) = decode_texture(&data, entry.extra) {
                    return Some(ArtBitmap {
                        width: bmp.size as u16,
                        height: bmp.size as u16,
                        rgba: bmp.rgba,
                    });
                }
            }
        }
        None
    }

    fn reload_all_client_files(&mut self) {
        let t0 = Instant::now();
        self.art_index = None;
        self.multi_index = None;
        self.gump_index = None;
        self.texture_index = None;
        self.light_index = None;
        self.sound_index = None;
        self.skills_index = None;
        self.anim_indices.clear();
        self.tiledata = None;
        self.hues = None;
        self.radar_col = None;
        self.cliloc = None;
        self.fonts_mgr = None;
        self.speech_mgr = None;
        self.animdata_mgr = None;
        self.valid_gumps.clear();
        self.valid_textures.clear();
        self.valid_sounds.clear();
        self.valid_lights.clear();
        self.item_thumb_cache.clear();
        self.land_thumb_cache.clear();

        // 1. Classic & Enhanced Art
        if let Some(uop) = self.file_manager.get_file_path("artlegacymul.uop") {
            self.art_index = FileIndex::from_uop(uop, ".tga", 0x14000).ok();
        } else if let (Some(idx), Some(mul)) = (
            self.file_manager.get_file_path("artidx.mul"),
            self.file_manager.get_file_path("art.mul"),
        ) {
            self.art_index = FileIndex::from_mul_idx(idx, mul, 0x14000).ok();
        }

        self.ec_art_index = EcArtIndex::new();
        if let Some(ec_dir) = &self.file_manager.uo_ec_dir {
            self.ec_art_index.load_directory(ec_dir);
        }
        if let Some(uo_dir) = &self.file_manager.uo_dir {
            self.ec_art_index.load_directory(uo_dir);
        }

        // 2. Multis
        if let (Some(idx), Some(mul)) = (
            self.file_manager.get_file_path("multi.idx"),
            self.file_manager.get_file_path("multi.mul"),
        ) {
            self.multi_index = FileIndex::from_mul_idx(idx, mul, 0x4000).ok();
            if let Some(mi) = &self.multi_index {
                self.multi_record_size = detect_multi_record_size(mi);
            }
        }
        if let Some(p) = self.file_manager.get_file_path("MultiCollection.uop") {
            self.uop_multis = load_uop_multis(p);
        } else {
            self.uop_multis.clear();
        }
        self.multilist_entries = load_multilist_entries(&self.file_manager);
        self.refresh_valid_multis();

        // 3. Gumps (Classic & Enhanced)
        if let Some(uop) = self.file_manager.get_file_path("gumpartlegacymul.uop") {
            self.gump_index = FileIndex::from_uop(uop, ".tga", 0xFFFF).ok();
        } else if let (Some(idx), Some(mul)) = (
            self.file_manager.get_file_path("gumpidx.mul"),
            self.file_manager.get_file_path("gumpart.mul"),
        ) {
            self.gump_index = FileIndex::from_mul_idx(idx, mul, 0xFFFF).ok();
        }

        if let Some(ec_uop) = self.file_manager.get_ec_file_path("gumpartlegacymul.uop") {
            self.gump_index_ec = FileIndex::from_uop(ec_uop, ".tga", 0xFFFF).ok();
        } else {
            self.gump_index_ec = None;
        }

        self.refresh_valid_gumps();

        // 4. Textures
        if let (Some(idx), Some(mul)) = (
            self.file_manager.get_file_path("texidx.mul"),
            self.file_manager.get_file_path("texmaps.mul"),
        ) {
            self.texture_index = FileIndex::from_mul_idx(idx, mul, 0x4000).ok();
        }
        if let Some(ti) = &self.texture_index {
            self.valid_textures = (0..0x4000)
                .filter(|&i| ti.is_valid(i))
                .map(|i| i as u32)
                .collect();
            if let Some(&first) = self.valid_textures.first() {
                self.texture_id = first;
            }
        }

        // 5. Light
        if let (Some(idx), Some(mul)) = (
            self.file_manager.get_file_path("lightidx.mul"),
            self.file_manager.get_file_path("light.mul"),
        ) {
            self.light_index = FileIndex::from_mul_idx(idx, mul, 100).ok();
        }
        if let Some(li) = &self.light_index {
            self.valid_lights = (0..100)
                .filter(|&i| li.is_valid(i))
                .map(|i| i as u32)
                .collect();
            if let Some(&first) = self.valid_lights.first() {
                self.light_id = first;
            }
        }

        // 6. Sounds
        if let (Some(idx), Some(mul)) = (
            self.file_manager.get_file_path("soundidx.mul"),
            self.file_manager.get_file_path("sound.mul"),
        ) {
            if let Ok(fi) = FileIndex::from_mul_idx(idx, mul, 0xFFF) {
                let mut sounds = Vec::new();
                for i in 0..0xFFF {
                    if fi.is_valid(i) {
                        if let Some(data) = fi.get_data(i) {
                            if let Some(snd) = SoundEntry::parse(i as u32, &data) {
                                sounds.push(snd);
                            }
                        }
                    }
                }
                self.valid_sounds = sounds;
                self.sound_index = Some(fi);
            }
        }

        // 7. Skills
        if let (Some(idx), Some(mul)) = (
            self.file_manager.get_file_path("skills.idx"),
            self.file_manager.get_file_path("skills.mul"),
        ) {
            self.skills_index = FileIndex::from_mul_idx(idx, mul, 100).ok();
        }

        // 8. Anim1..6 MUL & UOP
        let anim_configs = [
            (1, "Anim.idx", "Anim.mul", 0x40000),
            (2, "Anim2.idx", "Anim2.mul", 0x10000),
            (3, "Anim3.idx", "Anim3.mul", 0x20000),
            (4, "Anim4.idx", "Anim4.mul", 0x20000),
            (5, "Anim5.idx", "Anim5.mul", 0x20000),
            (6, "Anim6.idx", "Anim6.mul", 0x20000),
        ];
        for (id, idx, mul, max_e) in anim_configs {
            if let (Some(i_p), Some(m_p)) = (
                self.file_manager.get_file_path(idx),
                self.file_manager.get_file_path(mul),
            ) {
                if let Ok(fi) = FileIndex::from_mul_idx(i_p, m_p, max_e) {
                    self.anim_indices.insert(id, fi);
                }
            }
        }

        let read_txt = |fm: &FileManager, n: &str| {
            fm.get_file_path(n).and_then(|p| std::fs::read_to_string(p).ok())
        };
        self.anim_resolver = ClassicAnimResolver::parse(
            read_txt(&self.file_manager, "body.def").as_deref(),
            read_txt(&self.file_manager, "bodyconv.def").as_deref(),
        );

        self.equip_gump = EquipGumpMap::parse(
            read_txt(&self.file_manager, "equipconv.def").as_deref(),
            read_txt(&self.file_manager, "gump.def").as_deref(),
        );

        self.uop_anim_index = UopAnimationIndex::new();
        if let Some(uo_dir) = &self.file_manager.uo_dir {
            self.uop_anim_index.load_directory(uo_dir);
        }
        if let Some(ec_dir) = &self.file_manager.uo_ec_dir {
            self.uop_anim_index.load_directory(ec_dir);
        }

        // 9. TileData
        if let Some(td_path) = self.file_manager.get_file_path("tiledata.mul") {
            self.tiledata = TileDataManager::from_file(td_path).ok();
        }

        // 10. Hues
        if let Some(h_path) = self.file_manager.get_file_path("hues.mul") {
            self.hues = HuesManager::from_file(h_path).ok();
        }

        // 11. RadarCol
        if let Some(rc_path) = self.file_manager.get_file_path("radarcol.mul") {
            self.radar_col = RadarColManager::from_file(rc_path).ok();
        }

        // 12. Cliloc
        if let Some(cl_path) = self.file_manager.get_file_path("cliloc.enu") {
            self.cliloc = ClilocManager::from_file(cl_path).ok();
        }

        // 13. Fonts
        if let Some(fp) = self.file_manager.get_file_path("fonts.mul") {
            self.fonts_mgr = FontsManager::from_file(fp).ok();
        }

        // 14. Speech
        if let Some(sp) = self.file_manager.get_file_path("speech.mul") {
            self.speech_mgr = SpeechManager::from_file(sp).ok();
        }

        // 15. AnimData
        if let Some(ap) = self.file_manager.get_file_path("animdata.mul") {
            self.animdata_mgr = AnimDataManager::from_file(ap).ok();
        }

        // 16. Maps
        self.map_mgr = MapManager::new();
        for m_id in 0..6 {
            let (m_path, s_idx, s_mul) = uo_core::map_files(m_id, |n| self.file_manager.get_file_path(n));
            self.map_mgr.load_map(m_id, m_path, s_idx, s_mul);
            let f = |n: String| self.file_manager.get_file_path(&n);
            self.map_mgr.load_diff(
                m_id,
                f(format!("mapdif{}.mul", m_id)),
                f(format!("mapdifl{}.mul", m_id)),
                f(format!("stadif{}.mul", m_id)),
                f(format!("stadifi{}.mul", m_id)),
                f(format!("stadifl{}.mul", m_id)),
            );
        }

        // 17. GameData (Enhanced Client 3D tables)
        self.gamedata_archive = GameDataArchive::new();
        if let Some(ec_dir) = &self.file_manager.uo_ec_dir {
            self.gamedata_archive.load_directory(ec_dir);
        } else if let Some(uo_dir) = &self.file_manager.uo_dir {
            self.gamedata_archive.load_directory(uo_dir);
        }

        let dt = t0.elapsed();
        self.status_message = format!(
            "Arquivos carregados em {:.2} ms ⚡ (Art: {}, TileData: {}, Gumps: {}, Cliloc: {}, Sounds: {})",
            dt.as_secs_f64() * 1000.0,
            self.art_index.is_some(),
            self.tiledata.is_some(),
            self.valid_gumps.len(),
            self.cliloc.as_ref().map(|c| c.entries.len()).unwrap_or(0),
            self.valid_sounds.len(),
        );
    }

    fn calculate_mul_anim_index(body: u32, action: u32, direction: u8, file_type: u32) -> usize {
        let base = if file_type == 2 {
            if body < 200 {
                body * 110
            } else {
                22000 + ((body - 200) * 65)
            }
        } else if file_type == 3 {
            if body < 300 {
                body * 65
            } else if body < 400 {
                33000 + ((body - 300) * 110)
            } else {
                35000 + ((body - 400) * 175)
            }
        } else {
            if body < 200 {
                body * 110
            } else if body < 400 {
                22000 + ((body - 200) * 65)
            } else {
                35000 + ((body - 400) * 175)
            }
        };

        let mut idx = base + (action * 5);
        if direction <= 4 {
            idx += direction as u32;
        } else {
            idx += (direction - ((direction - 4) * 2)) as u32;
        }
        idx as usize
    }

    fn is_mul_action_defined(&self, body: u32, action: u32) -> bool {
        let r = self.anim_resolver.resolve(body);
        if let Some(fi) = self.anim_indices.get(&r.file_type) {
            if fi.is_valid(ClassicAnimResolver::index(r.file_type, r.body, action, 0)) {
                return true;
            }
        }
        for file_type in [1u32, 2, 3, 4, 5] {
            let raw_index = Self::calculate_mul_anim_index(body, action, 0, file_type);
            if let Some(fi) = self.anim_indices.get(&file_type) {
                if fi.is_valid(raw_index) {
                    return true;
                }
            }
        }
        false
    }

    fn is_action_defined(&self, body: u32, action: u32) -> bool {
        if self.anim_mode_enhanced {
            self.uop_anim_index.is_action_defined(body, action)
        } else {
            self.is_mul_action_defined(body, action)
        }
    }

    fn get_first_defined_action(&self, body: u32) -> u32 {
        let cur_def = self.anim_body_defs.iter().find(|d| d.body == body);
        let anim_type = cur_def
            .map(|d| d.anim_type)
            .unwrap_or(if body >= 400 && body <= 403 { 3 } else { 0 });
        let actions: &[(u32, &str)] = match anim_type {
            1 => &ACTIONS_SEA,
            2 => &ACTIONS_ANIMAL,
            3 => &ACTIONS_HUMAN,
            _ => &ACTIONS_MONSTER,
        };
        for (act, _) in actions {
            if self.is_action_defined(body, *act) {
                return *act;
            }
        }
        if self.anim_mode_enhanced {
            for extra in 0..60 {
                if self.uop_anim_index.is_action_defined(body, extra) {
                    return extra;
                }
            }
        }
        0
    }

    fn anim_def_matches(&self, def: &AnimationDef, search_lower: &str) -> bool {
        let has_uop = def.is_uop
            || self.uop_anim_index.is_action_defined(def.body, 0)
            || self.uop_anim_index.is_action_defined(def.body, 1);
        if self.anim_mode_enhanced {
            if !has_uop
                && (self.anim_category_filter == AnimCategoryFilter::UopModern
                    || self.anim_category_filter == AnimCategoryFilter::All)
            {
                return false;
            }
        } else {
            if self.anim_category_filter == AnimCategoryFilter::UopModern {
                // If user selected UOP category specifically in classic mode, allow it
            } else if def.is_uop && !self.is_mul_action_defined(def.body, 0) {
                return false;
            }
        }

        match self.anim_category_filter {
            AnimCategoryFilter::All => {}
            AnimCategoryFilter::Monsters => {
                if def.anim_type != 0 || def.is_equip {
                    return false;
                }
            }
            AnimCategoryFilter::SeaMonsters => {
                if def.anim_type != 1 || def.is_equip {
                    return false;
                }
            }
            AnimCategoryFilter::Animals => {
                if def.anim_type != 2 || def.is_equip {
                    return false;
                }
            }
            AnimCategoryFilter::Humanoids => {
                if def.anim_type != 3 || def.is_equip {
                    return false;
                }
            }
            AnimCategoryFilter::Equipment => {
                if !def.is_equip {
                    return false;
                }
            }
            AnimCategoryFilter::UopModern => {
                if !has_uop {
                    return false;
                }
            }
        }

        if !search_lower.is_empty() {
            let matches_id = def.body.to_string().contains(search_lower)
                || format!("0x{:04x}", def.body).contains(search_lower);
            let matches_name = def.name.to_lowercase().contains(search_lower);
            if !matches_id && !matches_name {
                return false;
            }
        }

        true
    }

    fn load_animation(&mut self, ctx: &egui::Context) {
        let t0 = Instant::now();
        self.anim_frames.clear();
        self.anim_current_frame_idx = 0;

        // 1. Try UOP if in enhanced mode or as defined modern source
        if self.anim_mode_enhanced
            || self
                .uop_anim_index
                .is_action_defined(self.anim_body, self.anim_action)
        {
            if let Some(frames) = self.uop_anim_index.get_animation_frames(
                self.anim_body,
                self.anim_action,
                self.anim_direction,
            ) {
                let count = frames.len();
                self.anim_frames = frames;
                self.update_animation_texture(ctx);
                let dt = t0.elapsed();
                self.status_message = format!(
                    "Animação UOP Body {} Action {} carregada: {} quadros em {:.2} ms ⚡",
                    self.anim_body,
                    self.anim_action,
                    count,
                    dt.as_secs_f64() * 1000.0
                );
                return;
            }
        }

        // 2a. Classic MUL resolved through Body.def / Bodyconv.def
        {
            let r = self.anim_resolver.resolve(self.anim_body);
            let raw_index =
                ClassicAnimResolver::index(r.file_type, r.body, self.anim_action, self.anim_direction);
            if let Some(data) = self
                .anim_indices
                .get(&r.file_type)
                .and_then(|fi| fi.get_data(raw_index))
            {
                let frames = decode_animation_mul(&data, self.anim_direction > 4);
                if !frames.is_empty() {
                    let count = frames.len();
                    self.anim_frames = frames;
                    self.update_animation_texture(ctx);
                    self.status_message = format!(
                        "Animação MUL (Anim{}) Body {} -> {} Action {} carregada: {} quadros em {:.2} ms ⚡",
                        r.file_type,
                        self.anim_body,
                        r.body,
                        self.anim_action,
                        count,
                        t0.elapsed().as_secs_f64() * 1000.0
                    );
                    return;
                }
            }
        }

        // 2b. Fallback: try every classic MUL Anim1..Anim5 with the legacy formulas
        for file_type in [1u32, 2, 3, 4, 5] {
            let raw_index = Self::calculate_mul_anim_index(
                self.anim_body,
                self.anim_action,
                self.anim_direction,
                file_type,
            );
            if let Some(fi) = self.anim_indices.get(&file_type) {
                if let Some(data) = fi.get_data(raw_index) {
                    let flip = self.anim_direction > 4;
                    let frames = decode_animation_mul(&data, flip);
                    if !frames.is_empty() {
                        let count = frames.len();
                        self.anim_frames = frames;
                        self.update_animation_texture(ctx);
                        let dt = t0.elapsed();
                        self.status_message = format!(
                            "Animação MUL (Anim{}) Body {} Action {} carregada: {} quadros em {:.2} ms ⚡",
                            file_type,
                            self.anim_body,
                            self.anim_action,
                            count,
                            dt.as_secs_f64() * 1000.0
                        );
                        return;
                    }
                }
            }
        }

        // 3. Fallback to UOP
        if let Some(frames) = self.uop_anim_index.get_animation_frames(
            self.anim_body,
            self.anim_action,
            self.anim_direction,
        ) {
            let count = frames.len();
            self.anim_frames = frames;
            self.update_animation_texture(ctx);
            let dt = t0.elapsed();
            self.status_message = format!(
                "Animação UOP Body {} Action {} carregada: {} quadros em {:.2} ms ⚡",
                self.anim_body,
                self.anim_action,
                count,
                dt.as_secs_f64() * 1000.0
            );
            return;
        }

        self.status_message = format!(
            "Nenhum quadro encontrado para Body {} Action {}.",
            self.anim_body, self.anim_action
        );
    }

    fn update_animation_texture(&mut self, ctx: &egui::Context) {
        if let Some(frame) = self.anim_frames.get(self.anim_current_frame_idx) {
            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [frame.width as usize, frame.height as usize],
                &frame.rgba_data,
            );
            self.anim_texture =
                Some(ctx.load_texture("anim_frame", color_image, egui::TextureOptions::NEAREST));
        }
    }

    fn render_current_multi(&mut self, ctx: &egui::Context) {
        let t0 = Instant::now();
        let structure = if self.multi_mode_enhanced {
            self.uop_multis.get(&self.multi_id).cloned()
        } else {
            let Some(multi_idx) = &self.multi_index else {
                self.status_message = "Arquivo multi.mul não carregado.".to_string();
                self.current_multi_structure = None;
                self.multi_texture = None;
                return;
            };
            if let Some(data) = multi_idx.get_data(self.multi_id as usize) {
                MultiStructure::parse_sized(self.multi_id, &data, self.multi_record_size)
            } else {
                None
            }
        };

        let Some(structure) = structure else {
            self.status_message = format!(
                "Multi 0x{:04X} ({}) não possui dados.",
                self.multi_id, self.multi_id
            );
            self.current_multi_structure = None;
            self.multi_texture = None;
            return;
        };

        let comp_count = structure.components.len();

        let rendered = structure.render_isometric(self.multi_max_z, |item_id| {
            if let Some(art) = &self.art_index {
                let static_id = 0x4000 + (item_id as usize);
                if let Some(art_data) = art.get_data(static_id) {
                    return decode_static(&art_data);
                }
            }
            None
        });

        if let Some(bmp) = rendered {
            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [bmp.width as usize, bmp.height as usize],
                &bmp.rgba,
            );
            self.multi_texture =
                Some(ctx.load_texture("multi_render", color_image, egui::TextureOptions::NEAREST));
            let dt = t0.elapsed();
            let source_name = if self.multi_mode_enhanced {
                "Enhanced UOP"
            } else {
                "Clássico"
            };
            self.status_message = format!(
                "Multi 0x{:04X} ({}) [{}]: {} componentes, {}x{} renderizado em {:.2} ms ⚡",
                self.multi_id,
                self.multi_id,
                source_name,
                comp_count,
                bmp.width,
                bmp.height,
                dt.as_secs_f64() * 1000.0
            );
        } else {
            self.multi_texture = None;
        }
        self.current_multi_structure = Some(structure);
    }

    fn load_item_preview(&mut self, ctx: &egui::Context) {
        if let Some(mut bmp) = self.get_item_bitmap(self.item_selected) {
            if self.item_hue > 0 {
                if let Some(hm) = &self.hues {
                    if let Some(hue) = hm.hues.get((self.item_hue - 1) as usize) {
                        let pal = hue.get_rgba_palette();
                        apply_hue_bytes(&mut bmp.rgba, &pal, false);
                    }
                }
            }

            let img = egui::ColorImage::from_rgba_unmultiplied(
                [bmp.width as usize, bmp.height as usize],
                &bmp.rgba,
            );
            self.item_texture =
                Some(ctx.load_texture("item_preview", img, egui::TextureOptions::NEAREST));
            return;
        }
        self.item_texture = None;
    }

    fn load_land_preview(&mut self, ctx: &egui::Context) {
        if let Some(bmp) = self.get_land_bitmap(self.land_selected) {
            let img = egui::ColorImage::from_rgba_unmultiplied(
                [bmp.width as usize, bmp.height as usize],
                &bmp.rgba,
            );
            self.land_texture =
                Some(ctx.load_texture("land_preview", img, egui::TextureOptions::NEAREST));
            return;
        }
        self.land_texture = None;
    }

    fn load_gump(&mut self, ctx: &egui::Context) {
        let gi_opt = if self.gump_mode_enhanced {
            self.gump_index_ec.as_ref().or(self.gump_index.as_ref())
        } else {
            self.gump_index.as_ref()
        };

        if let Some(gi) = gi_opt {
            let id = self.gump_id as usize;
            if let Some(data) = gi.get_data(id) {
                let entry = gi.entries[id];
                let (w, h, payload) =
                    if gi.is_uop() || entry.compression == 1 || entry.compression == 3 {
                        if data.len() >= 8 {
                            let w = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                            let h = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
                            (w, h, &data[8..])
                        } else {
                            (0, 0, &data[..])
                        }
                    } else {
                        let w = ((entry.extra >> 16) & 0xFFFF) as u32;
                        let h = (entry.extra & 0xFFFF) as u32;
                        (w, h, &data[..])
                    };

                if let Some(bmp) = decode_gump(payload, w, h) {
                    self.gump_dim = (bmp.width, bmp.height);
                    let img = egui::ColorImage::from_rgba_unmultiplied(
                        [bmp.width as usize, bmp.height as usize],
                        &bmp.rgba,
                    );
                    self.gump_texture =
                        Some(ctx.load_texture("gump_render", img, egui::TextureOptions::NEAREST));
                    return;
                }
            }
        }
        self.gump_texture = None;
        self.gump_dim = (0, 0);
    }

    fn load_texture_preview(&mut self, ctx: &egui::Context) {
        if let Some(ti) = &self.texture_index {
            let id = self.texture_id as usize;
            if let Some(data) = ti.get_data(id) {
                let entry = ti.entries[id];
                if let Some(bmp) = decode_texture(&data, entry.extra) {
                    self.texture_dim = bmp.size;
                    let img = egui::ColorImage::from_rgba_unmultiplied(
                        [bmp.size as usize, bmp.size as usize],
                        &bmp.rgba,
                    );
                    self.texture_texture = Some(ctx.load_texture(
                        "texture_render",
                        img,
                        egui::TextureOptions::NEAREST,
                    ));
                    return;
                }
            }
        }
        self.texture_texture = None;
    }

    fn load_radar_preview(&mut self, ctx: &egui::Context) {
        if let Some(art) = &self.art_index {
            if self.radar_is_items {
                let static_id = 0x4000 + self.radar_selected;
                if let Some(data) = art.get_data(static_id) {
                    if let Some(bmp) = decode_static(&data) {
                        let img = egui::ColorImage::from_rgba_unmultiplied(
                            [bmp.width as usize, bmp.height as usize],
                            &bmp.rgba,
                        );
                        self.radar_preview_tex = Some(ctx.load_texture(
                            "radar_preview",
                            img,
                            egui::TextureOptions::NEAREST,
                        ));
                        return;
                    }
                }
            } else {
                if let Some(data) = art.get_data(self.radar_selected) {
                    if let Some(bmp) = decode_land(&data) {
                        let img = egui::ColorImage::from_rgba_unmultiplied(
                            [bmp.width as usize, bmp.height as usize],
                            &bmp.rgba,
                        );
                        self.radar_preview_tex = Some(ctx.load_texture(
                            "radar_preview",
                            img,
                            egui::TextureOptions::NEAREST,
                        ));
                        return;
                    }
                }
            }
        }
        self.radar_preview_tex = None;
    }

    fn load_light_preview(&mut self, ctx: &egui::Context) {
        if let Some(li) = &self.light_index {
            let id = self.light_id as usize;
            if let Some(data) = li.get_data(id) {
                let entry = li.entries[id];
                if let Some(bmp) = decode_light(&data, entry.extra) {
                    self.light_dim = (bmp.width, bmp.height);
                    let img = egui::ColorImage::from_rgba_unmultiplied(
                        [bmp.width as usize, bmp.height as usize],
                        &bmp.rgba,
                    );
                    self.light_texture =
                        Some(ctx.load_texture("light_render", img, egui::TextureOptions::NEAREST));
                    return;
                }
            }
        }
        self.light_texture = None;
        self.light_dim = (0, 0);
    }

    fn update_font_preview(&mut self, ctx: &egui::Context) {
        if let Some(fm) = &self.fonts_mgr {
            if let Some(Some(font)) = fm.ascii_fonts.get(self.font_selected) {
                if let Some((w, h, rgba)) = font.render_text(&self.font_preview_text) {
                    let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                    self.font_texture =
                        Some(ctx.load_texture("font_render", img, egui::TextureOptions::NEAREST));
                    return;
                }
            }
        }
        self.font_texture = None;
    }

    fn update_map_view_custom(&mut self, ctx: &egui::Context, width_px: f32, height_px: f32) {
        match self.map_mode {
            MapMode::Radar => {
                if let Some(rc) = &self.radar_col {
                    let block_px = (8.0 * self.map_zoom).max(4.0);
                    let num_bx = ((width_px / block_px).ceil() as u32 + 4).clamp(16, 240);
                    let num_by = ((height_px / block_px).ceil() as u32 + 4).clamp(12, 180);

                    let bx = self.map_x / 8;
                    let by = self.map_y / 8;
                    let start_bx = bx.saturating_sub(num_bx / 2);
                    let start_by = by.saturating_sub(num_by / 2);

                    if let Some((w, h, rgba)) = self.map_mgr.render_minimap(
                        self.map_id,
                        start_bx,
                        start_by,
                        num_bx,
                        num_by,
                        rc,
                        self.map_show_statics,
                        self.map_use_diff,
                    ) {
                        let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                        self.map_texture =
                            Some(ctx.load_texture("map_render", img, egui::TextureOptions::NEAREST));
                    }
                }
            }
            MapMode::Live => {}
            MapMode::Overview => {}
        }
    }

    /// Whole-facet overview: facetNN.mul (or Multimap.rle for the Britannia maps). Click to jump.
    fn show_map_overview(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let want_multimap = self.overview_multimap && self.map_id <= 1;
        let stale = self
            .overview_tex
            .as_ref()
            .map_or(true, |(id, mm, ..)| *id != self.map_id || *mm != want_multimap);
        if stale {
            self.overview_tex = None;
            let data = if want_multimap {
                self.file_manager.get_file_path("multimap.rle").and_then(|p| std::fs::read(p).ok()).and_then(|d| decode_multimap(&d, true))
            } else {
                self.file_manager
                    .get_file_path(&format!("facet0{}.mul", self.map_id))
                    .and_then(|p| std::fs::read(p).ok())
                    .and_then(|d| decode_facet(&d))
            };
            if let Some(m) = data {
                let img = egui::ColorImage::from_rgba_unmultiplied([m.width as usize, m.height as usize], &m.rgba);
                let tex = ctx.load_texture("map_overview", img, egui::TextureOptions::LINEAR);
                self.overview_tex = Some((self.map_id, want_multimap, tex, m.width, m.height));
            }
        }
        let Some((_, _, tex, iw, ih)) = &self.overview_tex else {
            ui.label("Arquivo facet não encontrado para esta faceta.");
            return;
        };
        let (iw, ih) = (*iw as f32, *ih as f32);
        let avail = ui.available_size();
        let scale = (avail.x / iw).min(avail.y / ih).max(0.05);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(iw * scale, ih * scale), egui::Sense::click());
        ui.painter().image(
            tex.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        // world size this image covers (Multimap.rle is a scaled-down Britannia)
        let (ww, wh) = (self.map_mgr.actual_widths[self.map_id] as f32, MAP_DEFS[self.map_id].height as f32);
        let to_world = |p: egui::Pos2| ((p.x - rect.min.x) / rect.width() * ww, (p.y - rect.min.y) / rect.height() * wh);
        if let Some(p) = resp.hover_pos() {
            let (wx, wy) = to_world(p);
            egui::show_tooltip_at_pointer(ctx, ui.layer_id(), egui::Id::new("ov_tip"), |ui| {
                ui.label(format!("X {}  Y {}", wx as i32, wy as i32));
            });
        }
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                let (wx, wy) = to_world(p);
                self.map_x = wx.clamp(0.0, ww - 1.0) as u32;
                self.map_y = wy.clamp(0.0, wh - 1.0) as u32;
                self.map_mode = MapMode::Live;
                self.map_texture = None;
            }
        }
        // marker of the current position
        let mx = rect.min.x + self.map_x as f32 / ww * rect.width();
        let my = rect.min.y + self.map_y as f32 / wh * rect.height();
        ui.painter().circle_stroke(egui::pos2(mx, my), 6.0, egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(255, 80, 80)));
    }

    fn handle_keyboard_navigation(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }

        let (up, down, left, right, pgup, pgdn, space, enter) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.key_pressed(egui::Key::PageUp),
                i.key_pressed(egui::Key::PageDown),
                i.key_pressed(egui::Key::Space),
                i.key_pressed(egui::Key::Enter),
            )
        });

        let (map_left, map_right, map_up, map_down) = ctx.input(|i| {
            (
                i.key_down(egui::Key::ArrowLeft),
                i.key_down(egui::Key::ArrowRight),
                i.key_down(egui::Key::ArrowUp),
                i.key_down(egui::Key::ArrowDown),
            )
        });

        match self.active_tab {
            UoTab::Items => {
                let total = self
                    .tiledata
                    .as_ref()
                    .map(|td| td.item_tiles.len())
                    .unwrap_or(0x10000);
                let step = if self.item_view_mode { 1 } else { 5 };
                let old = self.item_selected;
                if down {
                    self.item_selected = (self.item_selected + step).min(total.saturating_sub(1));
                } else if up {
                    self.item_selected = self.item_selected.saturating_sub(step);
                } else if right {
                    self.item_selected = (self.item_selected + 1).min(total.saturating_sub(1));
                } else if left {
                    self.item_selected = self.item_selected.saturating_sub(1);
                } else if pgdn {
                    self.item_selected =
                        (self.item_selected + step * 8).min(total.saturating_sub(1));
                } else if pgup {
                    self.item_selected = self.item_selected.saturating_sub(step * 8);
                }
                if self.item_selected != old {
                    self.keyboard_navigated = true;
                    self.load_item_preview(ctx);
                }
            }
            UoTab::LandTiles => {
                let total = self
                    .tiledata
                    .as_ref()
                    .map(|td| td.land_tiles.len())
                    .unwrap_or(0x4000);
                let step = if self.land_view_mode { 1 } else { 5 };
                let old = self.land_selected;
                if down {
                    self.land_selected = (self.land_selected + step).min(total.saturating_sub(1));
                } else if up {
                    self.land_selected = self.land_selected.saturating_sub(step);
                } else if right {
                    self.land_selected = (self.land_selected + 1).min(total.saturating_sub(1));
                } else if left {
                    self.land_selected = self.land_selected.saturating_sub(1);
                } else if pgdn {
                    self.land_selected =
                        (self.land_selected + step * 8).min(total.saturating_sub(1));
                } else if pgup {
                    self.land_selected = self.land_selected.saturating_sub(step * 8);
                }
                if self.land_selected != old {
                    self.keyboard_navigated = true;
                    self.load_land_preview(ctx);
                }
            }
            UoTab::Gumps => {
                if !self.valid_gumps.is_empty() {
                    let cur_idx = self
                        .valid_gumps
                        .iter()
                        .position(|&g| g == self.gump_id)
                        .unwrap_or(0);
                    let old = cur_idx;
                    let new_idx = if down || right {
                        (cur_idx + 1).min(self.valid_gumps.len() - 1)
                    } else if up || left {
                        cur_idx.saturating_sub(1)
                    } else if pgdn {
                        (cur_idx + 10).min(self.valid_gumps.len() - 1)
                    } else if pgup {
                        cur_idx.saturating_sub(10)
                    } else {
                        cur_idx
                    };
                    if new_idx != old {
                        self.gump_id = self.valid_gumps[new_idx];
                        self.keyboard_navigated = true;
                        self.load_gump(ctx);
                    }
                }
            }
            UoTab::Textures => {
                if !self.valid_textures.is_empty() {
                    let cur_idx = self
                        .valid_textures
                        .iter()
                        .position(|&t| t == self.texture_id)
                        .unwrap_or(0);
                    let old = cur_idx;
                    let new_idx = if down || right {
                        (cur_idx + 1).min(self.valid_textures.len() - 1)
                    } else if up || left {
                        cur_idx.saturating_sub(1)
                    } else if pgdn {
                        (cur_idx + 10).min(self.valid_textures.len() - 1)
                    } else if pgup {
                        cur_idx.saturating_sub(10)
                    } else {
                        cur_idx
                    };
                    if new_idx != old {
                        self.texture_id = self.valid_textures[new_idx];
                        self.keyboard_navigated = true;
                        self.load_texture_preview(ctx);
                    }
                }
            }
            UoTab::RadarCol => {
                let total: usize = if self.radar_is_items { 0x10000 } else { 0x4000 };
                let old = self.radar_selected;
                if down {
                    self.radar_selected = (self.radar_selected + 1).min(total.saturating_sub(1));
                } else if up {
                    self.radar_selected = self.radar_selected.saturating_sub(1);
                } else if pgdn {
                    self.radar_selected = (self.radar_selected + 10).min(total.saturating_sub(1));
                } else if pgup {
                    self.radar_selected = self.radar_selected.saturating_sub(10);
                }
                if self.radar_selected != old {
                    self.keyboard_navigated = true;
                    self.load_radar_preview(ctx);
                }
            }
            UoTab::Hues => {
                let total = self.hues.as_ref().map(|h| h.hues.len()).unwrap_or(3000);
                let old = self.hue_selected;
                if down {
                    self.hue_selected = (self.hue_selected + 1).min(total.saturating_sub(1));
                } else if up {
                    self.hue_selected = self.hue_selected.saturating_sub(1);
                } else if pgdn {
                    self.hue_selected = (self.hue_selected + 10).min(total.saturating_sub(1));
                } else if pgup {
                    self.hue_selected = self.hue_selected.saturating_sub(10);
                }
                if self.hue_selected != old {
                    self.keyboard_navigated = true;
                }
            }
            UoTab::Animations => {
                if down || up {
                    let search_lower = self.anim_search.trim().to_lowercase();
                    let matching: Vec<u32> = self
                        .anim_body_defs
                        .iter()
                        .filter(|d| self.anim_def_matches(d, &search_lower))
                        .map(|d| d.body)
                        .collect();
                    if !matching.is_empty() {
                        let cur_pos = matching
                            .iter()
                            .position(|&b| b == self.anim_body)
                            .unwrap_or(0);
                        let new_pos = if down {
                            (cur_pos + 1).min(matching.len() - 1)
                        } else {
                            cur_pos.saturating_sub(1)
                        };
                        if new_pos != cur_pos {
                            self.anim_body = matching[new_pos];
                            self.anim_action = self.get_first_defined_action(self.anim_body);
                            self.load_animation(ctx);
                        }
                    }
                } else if right && !self.anim_frames.is_empty() {
                    self.anim_current_frame_idx =
                        (self.anim_current_frame_idx + 1) % self.anim_frames.len();
                    self.update_animation_texture(ctx);
                } else if left && !self.anim_frames.is_empty() {
                    self.anim_current_frame_idx = if self.anim_current_frame_idx == 0 {
                        self.anim_frames.len() - 1
                    } else {
                        self.anim_current_frame_idx - 1
                    };
                    self.update_animation_texture(ctx);
                }
            }
            UoTab::Sounds => {
                if !self.valid_sounds.is_empty() {
                    let old = self.sound_selected;
                    if down {
                        self.sound_selected =
                            (self.sound_selected + 1).min(self.valid_sounds.len() - 1);
                    } else if up {
                        self.sound_selected = self.sound_selected.saturating_sub(1);
                    }
                    if self.sound_selected != old {
                        self.keyboard_navigated = true;
                    }
                    if enter || space {
                        if let Some(snd) = self.valid_sounds.get(self.sound_selected) {
                            snd.play();
                        }
                    }
                }
            }
            UoTab::Map => {
                let pan_speed = 32;
                let mut moved = false;
                if map_left {
                    self.map_x = self.map_x.saturating_sub(pan_speed);
                    moved = true;
                } else if map_right {
                    self.map_x = (self.map_x + pan_speed).min(7168);
                    moved = true;
                }
                if map_up {
                    self.map_y = self.map_y.saturating_sub(pan_speed);
                    moved = true;
                } else if map_down {
                    self.map_y = (self.map_y + pan_speed).min(4096);
                    moved = true;
                }
                if moved {
                    let w = self.map_last_render_size.x.max(400.0);
                    let h = self.map_last_render_size.y.max(300.0);
                    self.update_map_view_custom(ctx, w, h);
                }
            }
            UoTab::Multis => {
                if !self.valid_multis.is_empty() {
                    let cur_idx = self
                        .valid_multis
                        .iter()
                        .position(|&m| m == self.multi_id)
                        .unwrap_or(0);
                    let old = cur_idx;
                    let new_idx = if down || right {
                        (cur_idx + 1).min(self.valid_multis.len() - 1)
                    } else if up || left {
                        cur_idx.saturating_sub(1)
                    } else if pgdn {
                        (cur_idx + 10).min(self.valid_multis.len() - 1)
                    } else if pgup {
                        cur_idx.saturating_sub(10)
                    } else {
                        cur_idx
                    };
                    if new_idx != old {
                        self.multi_id = self.valid_multis[new_idx];
                        self.keyboard_navigated = true;
                        self.render_current_multi(ctx);
                    }
                }
            }
            UoTab::GameData => {
                if !self.gamedata_archive.entries.is_empty() {
                    let total = self.gamedata_archive.entries.len();
                    let old = self.gamedata_selected_idx;
                    if down {
                        self.gamedata_selected_idx =
                            (self.gamedata_selected_idx + 1).min(total - 1);
                    } else if up {
                        self.gamedata_selected_idx = self.gamedata_selected_idx.saturating_sub(1);
                    } else if pgdn {
                        self.gamedata_selected_idx =
                            (self.gamedata_selected_idx + 10).min(total - 1);
                    } else if pgup {
                        self.gamedata_selected_idx = self.gamedata_selected_idx.saturating_sub(10);
                    }
                    if self.gamedata_selected_idx != old {
                        self.keyboard_navigated = true;
                    }
                }
            }
            _ => {}
        }
    }
}

impl eframe::App for UoFiddlerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        map::capture::tick(ctx);
        // ------------------ 1. SPLASH SCREEN ------------------
        if self.splash_active {
            let elapsed = self.splash_start.elapsed().as_secs_f32();
            if elapsed < 0.6 {
                self.splash_progress = elapsed / 2.0;
                self.splash_msg = "Descobrindo arquivos do cliente UO...";
            } else if elapsed < 1.2 {
                self.splash_progress = elapsed / 2.0;
                self.splash_msg = "Mapeando FileIndex e arquivos UOP na memória...";
            } else if elapsed < 1.8 {
                self.splash_progress = elapsed / 2.0;
                self.splash_msg = "Inicializando TileData, Hues e Renderizadores...";
            } else {
                self.splash_progress = 1.0;
                self.splash_msg = "Pronto! Iniciando interface gráfica...";
                self.reload_all_client_files();
                self.load_item_preview(ctx);
                self.load_land_preview(ctx);
                self.load_animation(ctx);
                self.load_gump(ctx);
                self.load_texture_preview(ctx);
                self.load_radar_preview(ctx);
                self.load_light_preview(ctx);
                self.update_font_preview(ctx);
                self.map_last_render_size = egui::Vec2::ZERO;
                self.map_texture = None;
                self.render_current_multi(ctx);
                self.splash_active = false;
            }

            egui::CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.set_max_width(500.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(80.0);
                        ui.heading(
                            egui::RichText::new("⚔ UOFiddler")
                                .size(32.0)
                                .color(egui::Color32::from_rgb(100, 149, 237))
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new(
                                "Ultima Online Client Viewer & Editor (100% Rust Nativo)",
                            )
                            .italics()
                            .color(egui::Color32::LIGHT_GRAY),
                        );
                        ui.add_space(30.0);

                        let progress_bar = egui::ProgressBar::new(self.splash_progress)
                            .show_percentage()
                            .animate(true);
                        ui.add(progress_bar);

                        ui.add_space(14.0);
                        ui.label(
                            egui::RichText::new(self.splash_msg)
                                .monospace()
                                .color(egui::Color32::from_rgb(180, 200, 220)),
                        );

                        if let Some(dir) = &self.file_manager.uo_dir {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(format!("Pasta: {}", dir.display()))
                                    .size(11.0)
                                    .color(egui::Color32::GRAY),
                            );
                        }
                    });
                });
            });

            ctx.request_repaint_after(Duration::from_millis(16));
            return;
        }

        // The map editor takes the whole window while it is open.
        if self.map_panel.editing() {
            let mut res = map::MapRes {
                maps: &mut self.map_mgr,
                art: self.art_index.as_ref(),
                textures: self.texture_index.as_ref(),
                tiledata: self.tiledata.as_ref(),
                hues: self.hues.as_ref(),
                radar: self.radar_col.as_ref(),
            };
            self.map_panel.fullscreen(ctx, &mut res, self.map_id, &mut self.map_x, &mut self.map_y);
            return;
        }

        // Global Keyboard navigation
        self.handle_keyboard_navigation(ctx);

        // ------------------ 2. MODERN TOP MENU & TWO-TIER TABS HEADER ------------------
        egui::TopBottomPanel::top("top_panel")
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(18, 19, 25)).inner_margin(egui::Margin::symmetric(14.0, 8.0)))
            .show(ctx, |ui| {
                // Row 1: Header Brand, Category Pills, and Quick Actions
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("⚔ UOFiddler")
                            .size(17.0)
                            .strong()
                            .color(egui::Color32::from_rgb(129, 140, 248)),
                    );

                    // Client Path Badges
                    if self.file_manager.uo_dir.is_some() {
                        ui.label(
                            egui::RichText::new("● Classic 2D")
                                .size(11.0)
                                .color(egui::Color32::from_rgb(52, 211, 153)),
                        );
                    }
                    if self.file_manager.uo_ec_dir.is_some() {
                        ui.label(
                            egui::RichText::new("● Enhanced 3D")
                                .size(11.0)
                                .color(egui::Color32::from_rgb(96, 165, 250)),
                        );
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Category Filter Pills
                    let is_art = self.active_category == Some(TabCategory::Art);
                    let is_data = self.active_category == Some(TabCategory::Data);
                    let is_world = self.active_category == Some(TabCategory::World);
                    let is_all = self.active_category.is_none();

                    if ui.selectable_label(is_art, &format!("🎨 Arte ({})", UoTab::ALL.iter().filter(|t| t.category() == TabCategory::Art).count())).clicked() {
                        self.active_category = Some(TabCategory::Art);
                        if self.active_tab.category() != TabCategory::Art {
                            self.active_tab = UoTab::Items;
                            self.load_item_preview(ctx);
                        }
                    }
                    if ui.selectable_label(is_data, &format!("📋 Dados ({})", UoTab::ALL.iter().filter(|t| t.category() == TabCategory::Data).count())).clicked() {
                        self.active_category = Some(TabCategory::Data);
                        if self.active_tab.category() != TabCategory::Data {
                            self.active_tab = UoTab::Hues;
                        }
                    }
                    if ui.selectable_label(is_world, &format!("🗺️ Mundo ({})", UoTab::ALL.iter().filter(|t| t.category() == TabCategory::World).count())).clicked() {
                        self.active_category = Some(TabCategory::World);
                        if self.active_tab.category() != TabCategory::World {
                            self.active_tab = UoTab::Map;
                        }
                    }
                    if ui.selectable_label(is_all, &format!("★ Todas ({})", UoTab::ALL.len())).clicked() {
                        self.active_category = None;
                    }

                    // Right-side actions
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("ℹ Sobre").clicked() {
                            self.show_about_dialog = true;
                        }

                        if ui
                            .button("🔄 Recarregar")
                            .on_hover_text("Recarregar todos os arquivos do cliente UO")
                            .clicked()
                        {
                            self.reload_all_client_files();
                            self.load_item_preview(ctx);
                            self.load_land_preview(ctx);
                            self.load_animation(ctx);
                            self.load_gump(ctx);
                            self.load_texture_preview(ctx);
                            self.load_radar_preview(ctx);
                            self.load_light_preview(ctx);
                            self.update_font_preview(ctx);
                            self.map_last_render_size = egui::Vec2::ZERO;
                            self.map_texture = None;
                            self.render_current_multi(ctx);
                            self.status_message = "Arquivos recarregados com sucesso.".to_string();
                        }

                        if ui
                            .button("⚙ Opções / Pastas")
                            .on_hover_text("Configurar pastas do Ultima Online")
                            .clicked()
                        {
                            self.show_settings_dialog = true;
                        }

                        // Quick Tab Switcher ComboBox
                        egui::ComboBox::from_id_salt("quick_tab_selector")
                            .selected_text(self.active_tab.name())
                            .width(130.0)
                            .show_ui(ui, |ui| {
                                for tab in UoTab::ALL {
                                    if ui.selectable_value(&mut self.active_tab, tab, tab.name()).clicked() {
                                        self.active_category = Some(tab.category());
                                        match tab {
                                            UoTab::Animations => self.load_animation(ctx),
                                            UoTab::Gumps => self.load_gump(ctx),
                                            UoTab::Textures => self.load_texture_preview(ctx),
                                            UoTab::RadarCol => self.load_radar_preview(ctx),
                                            UoTab::Light => self.load_light_preview(ctx),
                                            UoTab::Fonts => self.update_font_preview(ctx),
                                            UoTab::Map => {
                                                self.map_last_render_size = egui::Vec2::ZERO;
                                                self.map_texture = None;
                                            }
                                            UoTab::Multis => self.render_current_multi(ctx),
                                            _ => {}
                                        }
                                    }
                                }
                            });
                    });
                });

                ui.add_space(4.0);

                // Row 2: Filtered Tabs as sleek modern buttons
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let visible_tabs: Vec<UoTab> = match self.active_category {
                            Some(cat) => UoTab::ALL
                                .iter()
                                .copied()
                                .filter(|t| t.category() == cat)
                                .collect(),
                            None => UoTab::ALL.to_vec(),
                        };

                        for tab in visible_tabs {
                            let is_active = self.active_tab == tab;
                            let btn = if is_active {
                                egui::Button::new(
                                    egui::RichText::new(tab.name())
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                )
                                .fill(egui::Color32::from_rgb(67, 56, 202))
                                .stroke(egui::Stroke::new(
                                    1.0_f32,
                                    egui::Color32::from_rgb(129, 140, 248),
                                ))
                            } else {
                                egui::Button::new(
                                    egui::RichText::new(tab.name())
                                        .color(egui::Color32::from_rgb(203, 213, 225)),
                                )
                                .fill(egui::Color32::from_rgb(26, 28, 37))
                                .stroke(egui::Stroke::new(
                                    1.0_f32,
                                    egui::Color32::from_rgb(42, 45, 58),
                                ))
                            };

                            if ui.add(btn).clicked() {
                                self.active_tab = tab;
                                match tab {
                                    UoTab::Animations => self.load_animation(ctx),
                                    UoTab::Gumps => self.load_gump(ctx),
                                    UoTab::Textures => self.load_texture_preview(ctx),
                                    UoTab::RadarCol => self.load_radar_preview(ctx),
                                    UoTab::Light => self.load_light_preview(ctx),
                                    UoTab::Fonts => self.update_font_preview(ctx),
                                    UoTab::Map => {
                                        self.map_last_render_size = egui::Vec2::ZERO;
                                        self.map_texture = None;
                                    }
                                    UoTab::Multis => self.render_current_multi(ctx),
                                    _ => {}
                                }
                            }
                        }
                    });
                });
            });

        // ------------------ 3. STATUS BAR ------------------
        egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(14, 15, 20)).inner_margin(egui::Margin::symmetric(14.0, 4.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&self.status_message)
                            .color(egui::Color32::from_rgb(203, 213, 225))
                            .small(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("UOFiddler • Rust 100% Nativo ⚡")
                                .color(egui::Color32::from_rgb(129, 140, 248))
                                .small(),
                        );
                        ui.separator();
                        ui.label(
                            egui::RichText::new("[↑↓ / ←→] Navegar • [Scroll] Zoom • [Home/End] Início/Fim")
                                .color(egui::Color32::from_rgb(148, 163, 184))
                                .small(),
                        );
                    });
                });
            });

        // ------------------ 4. CENTRAL ACTIVE TAB ------------------
        egui::CentralPanel::default().show(ctx, |ui| match self.active_tab {
            UoTab::Items => self.render_items_tab(ui, ctx),
            UoTab::LandTiles => self.render_land_tiles_tab(ui, ctx),
            UoTab::Gumps => self.render_gumps_tab(ui, ctx),
            UoTab::Hues => self.render_hues_tab(ui),
            UoTab::Textures => self.render_textures_tab(ui, ctx),
            UoTab::RadarCol => self.render_radar_col_tab(ui, ctx),
            UoTab::Animations => self.render_animations_tab(ui, ctx),
            UoTab::Cliloc => self.render_cliloc_tab(ui),
            UoTab::Sounds => self.render_sounds_tab(ui),
            UoTab::Map => self.render_map_tab(ui, ctx),
            UoTab::Multis => self.render_multis_tab(ui, ctx),
            UoTab::Light => self.render_light_tab(ui, ctx),
            UoTab::Fonts => self.render_fonts_tab(ui, ctx),
            UoTab::Skills => self.render_skills_tab(ui),
            UoTab::Speech => self.render_speech_tab(ui),
            UoTab::AnimData => self.render_animdata_tab(ui, ctx),
            UoTab::GameData => self.render_gamedata_tab(ui),
            UoTab::Packets => self.packets_tab.show(ui),
            UoTab::Paperdoll => {
                let data = self.tiledata.as_ref().map(|td| paperdoll_tab::PaperdollData {
                    tiledata: td,
                    equip_gump: &self.equip_gump,
                    gump_index: self.gump_index.as_ref(),
                    hues: self.hues.as_ref(),
                    resolver: &self.anim_resolver,
                    mul_anim: &self.anim_indices,
                    uop_anim: &self.uop_anim_index,
                    art: self.art_index.as_ref(),
                });
                self.paperdoll_tab.show(ui, ctx, data.as_ref());
            }
            UoTab::Files => {
                if let Some(tab) = self.files_tab.show(ui, ctx, &self.file_manager) {
                    self.active_tab = tab;
                }
            }
        });

        // ------------------ 5. SETTINGS / DIRECTORY PATHS DIALOG ------------------
        if self.show_settings_dialog {
            egui::Window::new("UOFiddler - Path Settings")
                .resizable(false)
                .collapsible(false)
                .min_width(560.0)
                .show(ctx, |ui| {
                    ui.group(|ui| {
                        ui.label(
                            egui::RichText::new("Ultima Online Classic (2D) Directory").strong(),
                        );
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut self.settings_classic_input);
                            if ui.button("Browse...").clicked() {
                                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                    self.settings_classic_input = path.display().to_string();
                                }
                            }
                            if ui.button("Auto-Detect").clicked() {
                                if let Some(p) = FileManager::auto_detect_uo_dir() {
                                    self.settings_classic_input = p.display().to_string();
                                }
                            }
                        });
                    });

                    ui.add_space(8.0);
                    ui.group(|ui| {
                        ui.label(
                            egui::RichText::new("Ultima Online Enhanced (3D) Directory").strong(),
                        );
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut self.settings_ec_input);
                            if ui.button("Browse...").clicked() {
                                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                    self.settings_ec_input = path.display().to_string();
                                }
                            }
                            if ui.button("Auto-Detect").clicked() {
                                if let Some(p) = FileManager::auto_detect_ec_dir() {
                                    self.settings_ec_input = p.display().to_string();
                                }
                            }
                        });
                    });

                    ui.add_space(8.0);
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("File Detection Status").strong());
                        let st = self.file_manager.get_detection_status();
                        let show_status = |ui: &mut egui::Ui, name: &str, opt: &Option<PathBuf>| {
                            ui.horizontal(|ui| {
                                ui.label(format!("{}:", name));
                                if let Some(p) = opt {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "✓ ({})",
                                            p.file_name().unwrap_or_default().to_string_lossy()
                                        ))
                                        .color(egui::Color32::from_rgb(102, 187, 106)),
                                    );
                                } else {
                                    ui.label(
                                        egui::RichText::new("✗ Not found")
                                            .color(egui::Color32::from_rgb(239, 83, 80)),
                                    );
                                }
                            });
                        };
                        show_status(ui, "Classic Art (art.mul / uop)", &st.classic_art);
                        show_status(ui, "TileData (tiledata.mul)", &st.tiledata);
                        show_status(ui, "Gumps (gumpart.mul / uop)", &st.gumps);
                        show_status(ui, "Animations (anim.mul / uop)", &st.animations);
                        show_status(ui, "Multis (multi.mul / uop)", &st.multis);
                        show_status(ui, "3D GameData (GameData.uop)", &st.ec_gamedata);
                        show_status(ui, "3D TileArt (tileart.uop)", &st.ec_tileart);
                    });

                    ui.add_space(8.0);
                    ui.checkbox(
                        &mut self.settings_high_seas,
                        "High Seas TileData & Multi format",
                    );

                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Save & Reload All").clicked() {
                                if !self.settings_classic_input.is_empty() {
                                    self.file_manager.set_uo_dir(&self.settings_classic_input);
                                }
                                if !self.settings_ec_input.is_empty() {
                                    self.file_manager.set_uo_ec_dir(&self.settings_ec_input);
                                }
                                self.file_manager.high_seas = self.settings_high_seas;
                                self.file_manager.save_settings();
                                self.reload_all_client_files();
                                self.load_item_preview(ctx);
                                self.load_land_preview(ctx);
                                self.load_animation(ctx);
                                self.load_gump(ctx);
                                self.load_texture_preview(ctx);
                                self.load_radar_preview(ctx);
                                self.load_light_preview(ctx);
                                self.update_font_preview(ctx);
                                self.map_last_render_size = egui::Vec2::ZERO;
                                self.map_texture = None;
                                self.render_current_multi(ctx);
                                self.show_settings_dialog = false;
                            }
                            if ui.button("Cancel").clicked() {
                                self.show_settings_dialog = false;
                            }
                        });
                    });
                });
        }

        // ------------------ 6. ABOUT DIALOG ------------------
        if self.show_about_dialog {
            egui::Window::new("About UOFiddler")
                .resizable(false)
                .collapsible(false)
                .min_width(450.0)
                .show(ctx, |ui| {
                    ui.heading("⚔ UOFiddler v1.0.0");
                    ui.label("Reescrita completa em Rust nativo de altíssima performance para Linux e multiplataforma.");
                    ui.label("Zero dores de cabeça com .NET, Wine ou lentidão do Python.");
                    ui.add_space(8.0);
                    ui.label("• Autor: Mauricio Nunes + Gemini");
                    ui.label("• Renderização direta em GPU via eframe / egui.");
                    ui.add_space(12.0);
                    if ui.button("Fechar").clicked() {
                        self.show_about_dialog = false;
                    }
                });
        }

        // ------------------ 7. ANIMATION CONTINUOUS PLAYBACK LOOP ------------------
        if self.anim_playing && !self.anim_frames.is_empty() && self.active_tab == UoTab::Animations
        {
            let interval = Duration::from_millis((1000 / self.anim_fps.max(1)) as u64);
            if self.anim_last_frame_time.elapsed() >= interval {
                self.anim_current_frame_idx =
                    (self.anim_current_frame_idx + 1) % self.anim_frames.len();
                self.anim_last_frame_time = Instant::now();
                self.update_animation_texture(ctx);
            }
            ctx.request_repaint_after(interval);
        }

        // Reset keyboard navigation pulse
        self.keyboard_navigated = false;
    }
}

// ----------------- TAB IMPLEMENTATIONS (DUAL GRID/LIST & PARITY) -----------------
impl UoFiddlerApp {
    // 1. ITEMS TAB (Grid + List Mode + Vertical Zoom Tool)
    fn render_items_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::SidePanel::left("items_left_panel")
            .resizable(true)
            .default_width(340.0)
            .min_width(260.0)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Itens Estáticos");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.selectable_value(&mut self.item_view_mode, true, "📄 List");
                        ui.selectable_value(&mut self.item_view_mode, false, "🔲 Grid");
                    });
                });

                // Client / Source selector
                ui.horizontal(|ui| {
                    let classic_btn =
                        ui.selectable_label(!self.item_mode_enhanced, "📜 Clássico (2D)");
                    if classic_btn.clicked() && self.item_mode_enhanced {
                        self.item_mode_enhanced = false;
                        self.item_thumb_cache.clear();
                        self.load_item_preview(ctx);
                    }
                    let enhanced_btn =
                        ui.selectable_label(self.item_mode_enhanced, "✨ Enhanced (3D UOP)");
                    if enhanced_btn.clicked() && !self.item_mode_enhanced {
                        self.item_mode_enhanced = true;
                        self.item_thumb_cache.clear();
                        self.load_item_preview(ctx);
                    }
                });

                let total_items = self
                    .tiledata
                    .as_ref()
                    .map(|td| td.item_tiles.len())
                    .unwrap_or(0x10000);

                ui.horizontal(|ui| {
                    ui.label("🔍 Buscar:");
                    ui.text_edit_singleline(&mut self.item_search);
                    if !self.item_search.is_empty() && ui.button("✖").clicked() {
                        self.item_search.clear();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("🎯 Ir para ID:");
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.item_jump_input)
                            .desired_width(80.0)
                            .hint_text("0x..."),
                    );
                    if (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        || ui.button("Ir").clicked()
                    {
                        if let Some(id) = parse_id_input(&self.item_jump_input) {
                            if id < total_items {
                                self.item_selected = id;
                                self.keyboard_navigated = true;
                                self.load_item_preview(ctx);
                                self.status_message =
                                    format!("Navegado para Item 0x{:04X} ({})", id, id);
                            }
                        }
                    }
                });

                ui.separator();
                let search_lower = self.item_search.trim().to_lowercase();

                if self.item_view_mode {
                    // List Mode
                    let row_height = 20.0;
                    if search_lower.is_empty() {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show_rows(ui, row_height, total_items, |ui, row_range| {
                                if self.keyboard_navigated {
                                    let target_y = (self.item_selected as f32) * row_height;
                                    ui.scroll_to_rect(
                                        egui::Rect::from_min_size(
                                            egui::pos2(0.0, target_y),
                                            egui::vec2(10.0, row_height),
                                        ),
                                        Some(egui::Align::Center),
                                    );
                                }
                                for id in row_range {
                                    let hex_str = format!("0x{:04X} ({})", id, id);
                                    let name = self
                                        .tiledata
                                        .as_ref()
                                        .and_then(|td| td.item_tiles.get(id))
                                        .map(|t| t.name.as_str())
                                        .unwrap_or("");
                                    let label = if name.is_empty() {
                                        hex_str
                                    } else {
                                        format!("{} - {}", hex_str, name)
                                    };

                                    let is_sel = self.item_selected == id;
                                    let resp = ui.selectable_label(is_sel, label);
                                    if resp.clicked() {
                                        self.item_selected = id;
                                        self.load_item_preview(ctx);
                                    }
                                    if is_sel && self.keyboard_navigated {
                                        resp.scroll_to_me(Some(egui::Align::Center));
                                    }
                                }
                            });
                    } else {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let mut matches = 0;
                                for id in 0..total_items {
                                    let hex_str = format!("0x{:04X} ({})", id, id);
                                    let name = self
                                        .tiledata
                                        .as_ref()
                                        .and_then(|td| td.item_tiles.get(id))
                                        .map(|t| t.name.as_str())
                                        .unwrap_or("");
                                    if !hex_str.to_lowercase().contains(&search_lower)
                                        && !name.to_lowercase().contains(&search_lower)
                                    {
                                        continue;
                                    }
                                    let label = if name.is_empty() {
                                        hex_str
                                    } else {
                                        format!("{} - {}", hex_str, name)
                                    };
                                    let is_sel = self.item_selected == id;
                                    let resp = ui.selectable_label(is_sel, label);
                                    if resp.clicked() {
                                        self.item_selected = id;
                                        self.load_item_preview(ctx);
                                    }
                                    if is_sel && self.keyboard_navigated {
                                        resp.scroll_to_me(Some(egui::Align::Center));
                                    }
                                    matches += 1;
                                    if matches >= 500 {
                                        break;
                                    }
                                }
                            });
                    }
                } else {
                    // Grid Mode (Thumbnail cards with responsive columns)
                    let card_w = 58.0;
                    let card_h = 66.0;
                    let spacing = 4.0;
                    let avail_w = ui.available_width();
                    let cols = ((avail_w) / (card_w + spacing)).floor().max(1.0) as usize;
                    let total_rows = (total_items + cols - 1) / cols;
                    let row_h = card_h + spacing;

                    let mut clicked_id = None;

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show_rows(ui, row_h, total_rows, |ui, row_range| {
                            if self.keyboard_navigated {
                                let target_row = self.item_selected / cols;
                                let target_y = (target_row as f32) * row_h;
                                ui.scroll_to_rect(
                                    egui::Rect::from_min_size(
                                        egui::pos2(0.0, target_y),
                                        egui::vec2(10.0, row_h),
                                    ),
                                    Some(egui::Align::Center),
                                );
                            }

                            for r in row_range {
                                ui.horizontal(|ui| {
                                    for c in 0..cols {
                                        let id = r * cols + c;
                                        if id >= total_items {
                                            break;
                                        }

                                        let is_sel = self.item_selected == id;
                                        let bg = if is_sel {
                                            egui::Color32::from_rgb(60, 70, 110)
                                        } else {
                                            egui::Color32::from_rgb(25, 25, 32)
                                        };

                                        let (rect, resp) = ui.allocate_exact_size(
                                            egui::vec2(card_w, card_h),
                                            egui::Sense::click(),
                                        );
                                        if resp.clicked() {
                                            clicked_id = Some(id);
                                        }
                                        if is_sel && self.keyboard_navigated {
                                            resp.scroll_to_me(Some(egui::Align::Center));
                                        }

                                        ui.painter().rect_filled(rect, 4.0, bg);
                                        if is_sel {
                                            ui.painter().rect_stroke(
                                                rect,
                                                4.0,
                                                egui::Stroke::new(
                                                    1.5_f32,
                                                    egui::Color32::from_rgb(100, 149, 237),
                                                ),
                                            );
                                        }

                                        // Ensure thumbnail texture with bilinear minification to prevent distortion
                                        if !self.item_thumb_cache.contains_key(&id) {
                                            if let Some(bmp) = self.get_item_bitmap(id) {
                                                let c_img =
                                                    egui::ColorImage::from_rgba_unmultiplied(
                                                        [bmp.width as usize, bmp.height as usize],
                                                        &bmp.rgba,
                                                    );
                                                let handle = ctx.load_texture(
                                                    format!("it_{}", id),
                                                    c_img,
                                                    egui::TextureOptions {
                                                        magnification: egui::TextureFilter::Nearest,
                                                        minification: egui::TextureFilter::Linear,
                                                        wrap_mode: egui::TextureWrapMode::ClampToEdge,
                                                        mipmap_mode: None,
                                                    },
                                                );
                                                self.item_thumb_cache.insert(id, handle);
                                            }
                                        }

                                        if let Some(tex) = self.item_thumb_cache.get(&id) {
                                            let orig = tex.size_vec2();
                                            let max_dim = 42.0_f32;
                                            let scale =
                                                (max_dim / orig.x).min(max_dim / orig.y).min(1.0);
                                            let display_size = orig * scale;
                                            let icon_rect = egui::Rect::from_center_size(
                                                egui::pos2(rect.center().x, rect.min.y + 24.0),
                                                display_size,
                                            );
                                            ui.painter().image(
                                                tex.id(),
                                                icon_rect,
                                                egui::Rect::from_min_max(
                                                    egui::pos2(0.0, 0.0),
                                                    egui::pos2(1.0, 1.0),
                                                ),
                                                egui::Color32::WHITE,
                                            );
                                        }

                                        // Hex Label
                                        ui.painter().text(
                                            egui::pos2(rect.center().x, rect.max.y - 10.0),
                                            egui::Align2::CENTER_CENTER,
                                            format!("0x{:04X}", id),
                                            egui::FontId::monospace(9.0),
                                            egui::Color32::from_rgb(180, 190, 210),
                                        );
                                    }
                                });
                            }
                        });

                    if let Some(id) = clicked_id {
                        self.item_selected = id;
                        self.load_item_preview(ctx);
                    }
                }
            });

        // 2. Right Panel: TileData Properties & Flags Inspector
        egui::SidePanel::right("item_inspector_panel")
            .resizable(true)
            .default_width(320.0)
            .min_width(280.0)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("📋 Inspetor TileData");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("💾 Aplicar")
                            .on_hover_text("Salvar modificações deste item na memória")
                            .clicked()
                        {
                            self.status_message = format!(
                                "✓ Propriedades do Item 0x{:04X} salvas na memória!",
                                self.item_selected
                            );
                        }
                    });
                });
                ui.separator();

                if let Some(td) = &mut self.tiledata {
                    if let Some(item_info) = td.item_tiles.get_mut(self.item_selected) {
                        egui::Grid::new("item_props_grid")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                ui.label("Nome:");
                                ui.text_edit_singleline(&mut item_info.name);
                                ui.end_row();

                                ui.label("Peso:");
                                ui.add(egui::DragValue::new(&mut item_info.weight).range(0..=255));
                                ui.end_row();

                                ui.label("Altura:");
                                ui.add(egui::DragValue::new(&mut item_info.height).range(0..=255));
                                ui.end_row();

                                ui.label("Anim ID:");
                                ui.horizontal(|ui| {
                                    ui.add(egui::DragValue::new(&mut item_info.anim_id).range(0..=65535));
                                    ui.label(format!("(0x{:04X})", item_info.anim_id));
                                });
                                ui.end_row();

                                ui.label("Qualidade / Camada:");
                                ui.add(egui::DragValue::new(&mut item_info.quality).range(0..=255));
                                ui.end_row();

                                ui.label("Hue Padrão:");
                                ui.add(egui::DragValue::new(&mut item_info.hue).range(0..=255));
                                ui.end_row();

                                ui.label("Flags Hex:");
                                ui.label(
                                    egui::RichText::new(format!("0x{:016X}", item_info.flags))
                                        .monospace()
                                        .color(egui::Color32::from_rgb(129, 140, 248)),
                                );
                                ui.end_row();
                            });

                        ui.add_space(8.0);
                        ui.separator();
                        ui.label(egui::RichText::new("Flags (Bitfield 38):").strong());

                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                egui::Grid::new("item_flags_grid")
                                    .num_columns(2)
                                    .spacing([12.0, 4.0])
                                    .show(ui, |ui| {
                                        for (i, (name, mask)) in TILEDATA_FLAGS.iter().enumerate() {
                                            let mut checked = (item_info.flags & mask) != 0;
                                            if ui.checkbox(&mut checked, *name).changed() {
                                                if checked {
                                                    item_info.flags |= mask;
                                                } else {
                                                    item_info.flags &= !mask;
                                                }
                                            }
                                            if (i + 1) % 2 == 0 {
                                                ui.end_row();
                                            }
                                        }
                                    });
                            });
                    }
                } else {
                    ui.label("TileData não carregado.");
                }
            });

        // 3. Central Panel: Art Canvas Preview & Zoom
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(format!(
                    "Item 0x{:04X} ({})",
                    self.item_selected, self.item_selected
                ));

                if ui
                    .button("📋 Copiar Hex")
                    .on_hover_text("Copiar ID Hexadecimal para a área de transferência")
                    .clicked()
                {
                    let hex = format!("0x{:04X}", self.item_selected);
                    ctx.copy_text(hex.clone());
                    self.status_message = format!("✓ {} copiado!", hex);
                }

                if ui
                    .button("📋 Copiar Dec")
                    .on_hover_text("Copiar ID Decimal para a área de transferência")
                    .clicked()
                {
                    let dec = format!("{}", self.item_selected);
                    ctx.copy_text(dec.clone());
                    self.status_message = format!("✓ {} copiado!", dec);
                }

                if ui
                    .button("💾 Exportar PNG")
                    .on_hover_text("Salvar imagem do item na pasta 'exported_art/'")
                    .clicked()
                {
                    if let Some(bmp) = self.get_item_bitmap(self.item_selected) {
                        let filename = format!("item_0x{:04X}.png", self.item_selected);
                        if let Some(path) = export_rgba_png(
                            bmp.width as u32,
                            bmp.height as u32,
                            &bmp.rgba,
                            &filename,
                        ) {
                            self.status_message = format!("✓ Imagem exportada para {}", path);
                        }
                    }
                }

                let chk_label = if self.preview_checkerboard {
                    "🏁 Grade: Ligada"
                } else {
                    "⬛ Grade: Desligada"
                };
                if ui
                    .selectable_label(self.preview_checkerboard, chk_label)
                    .on_hover_text("Alternar fundo entre sólido e grade transparente")
                    .clicked()
                {
                    self.preview_checkerboard = !self.preview_checkerboard;
                }

                ui.separator();
                ui.label("Testar Hue:");
                if ui
                    .add(egui::DragValue::new(&mut self.item_hue).range(0..=3000))
                    .changed()
                {
                    self.load_item_preview(ctx);
                }
            });

            ui.add_space(8.0);

            let avail = ui.available_size();
            let zoom_w = 46.0;
            let canvas_w = (avail.x - zoom_w - 10.0).max(100.0);
            let canvas_h = avail.y.max(100.0);

            ui.horizontal(|ui| {
                // 1:1 Canvas filling 100% of the entire remaining panel height & width
                let (rect, resp) = ui.allocate_exact_size(
                    egui::vec2(canvas_w, canvas_h),
                    egui::Sense::click_and_drag(),
                );
                if self.preview_checkerboard {
                    draw_checkerboard(ui.painter(), rect, 16.0);
                } else {
                    ui.painter()
                        .rect_filled(rect, 4.0, egui::Color32::from_rgb(18, 18, 22));
                }
                ui.painter().rect_stroke(
                    rect,
                    4.0,
                    egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(45, 45, 55)),
                );

                if resp.hovered() {
                    let scroll = ui.input(|i| i.raw_scroll_delta.y);
                    if scroll > 0.0 {
                        self.item_zoom = (self.item_zoom * 1.15).min(5.0);
                    } else if scroll < 0.0 {
                        self.item_zoom = (self.item_zoom / 1.15).max(0.5);
                    }
                }

                if let Some(tex) = &self.item_texture {
                    let center = rect.center();
                    let size = tex.size_vec2() * self.item_zoom; // 1:1 by default
                    let img_rect = egui::Rect::from_center_size(center, size);
                    ui.painter().image(
                        tex.id(),
                        img_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Nenhuma arte de item disponível",
                        egui::FontId::proportional(14.0),
                        egui::Color32::GRAY,
                    );
                }

                // Zoom tool on the right (neat vertical slider)
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(format!("{:.1}x", self.item_zoom)).small());
                    let slider_h = (canvas_h - 100.0).clamp(80.0, 220.0);
                    ui.add_sized(
                        [24.0, slider_h],
                        egui::Slider::new(&mut self.item_zoom, 0.5..=5.0)
                            .vertical()
                            .show_value(false),
                    );
                    ui.add_space(4.0);
                    if ui.button("1x").on_hover_text("Zoom 100%").clicked() {
                        self.item_zoom = 1.0;
                    }
                    if ui.button("2x").on_hover_text("Zoom 200%").clicked() {
                        self.item_zoom = 2.0;
                    }
                    if ui.button("3x").on_hover_text("Zoom 300%").clicked() {
                        self.item_zoom = 3.0;
                    }
                });
            });
        });
    }

    // 2. LAND TILES TAB (Grid + List Mode + Vertical Zoom Tool)
    fn render_land_tiles_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::SidePanel::left("land_left_panel")
            .resizable(true)
            .default_width(340.0)
            .min_width(260.0)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Terrenos (Land Tiles)");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.selectable_value(&mut self.land_view_mode, true, "📄 List");
                        ui.selectable_value(&mut self.land_view_mode, false, "🔲 Grid");
                    });
                });

                // Client / Source selector
                ui.horizontal(|ui| {
                    let classic_btn =
                        ui.selectable_label(!self.land_mode_enhanced, "📜 Clássico (2D Diamond)");
                    if classic_btn.clicked() && self.land_mode_enhanced {
                        self.land_mode_enhanced = false;
                        self.land_thumb_cache.clear();
                        self.load_land_preview(ctx);
                    }
                    let enhanced_btn =
                        ui.selectable_label(self.land_mode_enhanced, "✨ Enhanced (3D Textura)");
                    if enhanced_btn.clicked() && !self.land_mode_enhanced {
                        self.land_mode_enhanced = true;
                        self.land_thumb_cache.clear();
                        self.load_land_preview(ctx);
                    }
                });

                let total_land = self
                    .tiledata
                    .as_ref()
                    .map(|td| td.land_tiles.len())
                    .unwrap_or(0x4000);

                ui.horizontal(|ui| {
                    ui.label("🔍 Buscar:");
                    ui.text_edit_singleline(&mut self.land_search);
                    if !self.land_search.is_empty() && ui.button("✖").clicked() {
                        self.land_search.clear();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("🎯 Ir para ID:");
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.land_jump_input)
                            .desired_width(80.0)
                            .hint_text("0x..."),
                    );
                    if (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        || ui.button("Ir").clicked()
                    {
                        if let Some(id) = parse_id_input(&self.land_jump_input) {
                            if id < total_land {
                                self.land_selected = id;
                                self.keyboard_navigated = true;
                                self.load_land_preview(ctx);
                                self.status_message =
                                    format!("Navegado para Terreno 0x{:04X} ({})", id, id);
                            }
                        }
                    }
                });

                ui.separator();
                let row_height = 20.0;
                let search_lower = self.land_search.trim().to_lowercase();

                if self.land_view_mode {
                    // List Mode
                    if search_lower.is_empty() {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show_rows(ui, row_height, total_land, |ui, row_range| {
                                if self.keyboard_navigated {
                                    let target_y = (self.land_selected as f32) * row_height;
                                    ui.scroll_to_rect(
                                        egui::Rect::from_min_size(
                                            egui::pos2(0.0, target_y),
                                            egui::vec2(10.0, row_height),
                                        ),
                                        Some(egui::Align::Center),
                                    );
                                }
                                for id in row_range {
                                    let hex_str = format!("0x{:04X} ({})", id, id);
                                    let name = self
                                        .tiledata
                                        .as_ref()
                                        .and_then(|td| td.land_tiles.get(id))
                                        .map(|t| t.name.as_str())
                                        .unwrap_or("");
                                    let label = if name.is_empty() {
                                        hex_str
                                    } else {
                                        format!("{} - {}", hex_str, name)
                                    };

                                    let is_sel = self.land_selected == id;
                                    let resp = ui.selectable_label(is_sel, label);
                                    if resp.clicked() {
                                        self.land_selected = id;
                                        self.load_land_preview(ctx);
                                    }
                                    if is_sel && self.keyboard_navigated {
                                        resp.scroll_to_me(Some(egui::Align::Center));
                                    }
                                }
                            });
                    } else {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let mut matches = 0;
                                for id in 0..total_land {
                                    let hex_str = format!("0x{:04X} ({})", id, id);
                                    let name = self
                                        .tiledata
                                        .as_ref()
                                        .and_then(|td| td.land_tiles.get(id))
                                        .map(|t| t.name.as_str())
                                        .unwrap_or("");
                                    if !hex_str.to_lowercase().contains(&search_lower)
                                        && !name.to_lowercase().contains(&search_lower)
                                    {
                                        continue;
                                    }
                                    let label = if name.is_empty() {
                                        hex_str
                                    } else {
                                        format!("{} - {}", hex_str, name)
                                    };
                                    let is_sel = self.land_selected == id;
                                    let resp = ui.selectable_label(is_sel, label);
                                    if resp.clicked() {
                                        self.land_selected = id;
                                        self.load_land_preview(ctx);
                                    }
                                    if is_sel && self.keyboard_navigated {
                                        resp.scroll_to_me(Some(egui::Align::Center));
                                    }
                                    matches += 1;
                                    if matches >= 500 {
                                        break;
                                    }
                                }
                            });
                    }
                } else {
                    // Grid Mode (Responsive columns)
                    let card_w = 58.0;
                    let card_h = 66.0;
                    let spacing = 4.0;
                    let avail_w = ui.available_width();
                    let cols = ((avail_w) / (card_w + spacing)).floor().max(1.0) as usize;
                    let total_rows = (total_land + cols - 1) / cols;
                    let row_h = card_h + spacing;

                    let mut clicked_id = None;

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show_rows(ui, row_h, total_rows, |ui, row_range| {
                            if self.keyboard_navigated {
                                let target_row = self.land_selected / cols;
                                let target_y = (target_row as f32) * row_h;
                                ui.scroll_to_rect(
                                    egui::Rect::from_min_size(
                                        egui::pos2(0.0, target_y),
                                        egui::vec2(10.0, row_h),
                                    ),
                                    Some(egui::Align::Center),
                                );
                            }

                            for r in row_range {
                                ui.horizontal(|ui| {
                                    for c in 0..cols {
                                        let id = r * cols + c;
                                        if id >= total_land {
                                            break;
                                        }

                                        let is_sel = self.land_selected == id;
                                        let bg = if is_sel {
                                            egui::Color32::from_rgb(60, 70, 110)
                                        } else {
                                            egui::Color32::from_rgb(25, 25, 32)
                                        };

                                        let (rect, resp) = ui.allocate_exact_size(
                                            egui::vec2(card_w, card_h),
                                            egui::Sense::click(),
                                        );
                                        if resp.clicked() {
                                            clicked_id = Some(id);
                                        }
                                        if is_sel && self.keyboard_navigated {
                                            resp.scroll_to_me(Some(egui::Align::Center));
                                        }

                                        ui.painter().rect_filled(rect, 4.0, bg);
                                        if is_sel {
                                            ui.painter().rect_stroke(
                                                rect,
                                                4.0,
                                                egui::Stroke::new(
                                                    1.5_f32,
                                                    egui::Color32::from_rgb(100, 149, 237),
                                                ),
                                            );
                                        }

                                        if !self.land_thumb_cache.contains_key(&id) {
                                            if let Some(bmp) = self.get_land_bitmap(id) {
                                                let c_img =
                                                    egui::ColorImage::from_rgba_unmultiplied(
                                                        [bmp.width as usize, bmp.height as usize],
                                                        &bmp.rgba,
                                                    );
                                                let handle = ctx.load_texture(
                                                    format!("lt_{}", id),
                                                    c_img,
                                                    egui::TextureOptions {
                                                        magnification: egui::TextureFilter::Nearest,
                                                        minification: egui::TextureFilter::Linear,
                                                        wrap_mode: egui::TextureWrapMode::ClampToEdge,
                                                        mipmap_mode: None,
                                                    },
                                                );
                                                self.land_thumb_cache.insert(id, handle);
                                            }
                                        }

                                        if let Some(tex) = self.land_thumb_cache.get(&id) {
                                            let orig = tex.size_vec2();
                                            let max_dim = 42.0_f32;
                                            let scale =
                                                (max_dim / orig.x).min(max_dim / orig.y).min(1.0);
                                            let display_size = orig * scale;
                                            let icon_rect = egui::Rect::from_center_size(
                                                egui::pos2(rect.center().x, rect.min.y + 24.0),
                                                display_size,
                                            );
                                            ui.painter().image(
                                                tex.id(),
                                                icon_rect,
                                                egui::Rect::from_min_max(
                                                    egui::pos2(0.0, 0.0),
                                                    egui::pos2(1.0, 1.0),
                                                ),
                                                egui::Color32::WHITE,
                                            );
                                        }

                                        ui.painter().text(
                                            egui::pos2(rect.center().x, rect.max.y - 10.0),
                                            egui::Align2::CENTER_CENTER,
                                            format!("0x{:04X}", id),
                                            egui::FontId::monospace(9.0),
                                            egui::Color32::from_rgb(180, 190, 210),
                                        );
                                    }
                                });
                            }
                        });

                    if let Some(id) = clicked_id {
                        self.land_selected = id;
                        self.load_land_preview(ctx);
                    }
                }
            });

        // 2. Right Panel: TileData Properties & Flags Inspector for Land Tiles
        egui::SidePanel::right("land_inspector_panel")
            .resizable(true)
            .default_width(320.0)
            .min_width(280.0)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("📋 Inspetor TileData");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("💾 Aplicar")
                            .on_hover_text("Salvar modificações deste terreno na memória")
                            .clicked()
                        {
                            self.status_message = format!(
                                "✓ Propriedades do Terreno 0x{:04X} salvas na memória!",
                                self.land_selected
                            );
                        }
                    });
                });
                ui.separator();

                if let Some(td) = &mut self.tiledata {
                    if let Some(land_info) = td.land_tiles.get_mut(self.land_selected) {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                egui::Grid::new("land_prop_grid")
                                    .num_columns(2)
                                    .spacing([8.0, 6.0])
                                    .show(ui, |ui| {
                                        ui.label(egui::RichText::new("Nome:").strong());
                                        ui.text_edit_singleline(&mut land_info.name);
                                        ui.end_row();

                                        ui.label("Texture ID:");
                                        ui.horizontal(|ui| {
                                            ui.add(egui::DragValue::new(&mut land_info.texture_id));
                                            ui.label(format!("(0x{:04X})", land_info.texture_id));
                                        });
                                        ui.end_row();

                                        ui.label("Flags (Hex):");
                                        let mut flags_hex = format!("0x{:016X}", land_info.flags);
                                        if ui
                                            .add(
                                                egui::TextEdit::singleline(&mut flags_hex)
                                                    .desired_width(140.0),
                                            )
                                            .changed()
                                        {
                                            let clean = flags_hex
                                                .trim_start_matches("0x")
                                                .trim_start_matches("0X");
                                            if let Ok(val) = u64::from_str_radix(clean, 16) {
                                                land_info.flags = val;
                                            }
                                        }
                                        ui.end_row();
                                    });

                                ui.add_space(8.0);
                                ui.separator();
                                ui.label(egui::RichText::new("Flags (Bitfield):").strong());
                                ui.add_space(4.0);

                                // 2 columns for 38 flags
                                egui::Grid::new("land_flags_grid")
                                    .num_columns(2)
                                    .spacing([6.0, 4.0])
                                    .show(ui, |ui| {
                                        for (idx, (name, mask)) in TILEDATA_FLAGS.iter().enumerate()
                                        {
                                            let mut is_set = (land_info.flags & mask) != 0;
                                            if ui.checkbox(&mut is_set, *name).changed() {
                                                if is_set {
                                                    land_info.flags |= mask;
                                                } else {
                                                    land_info.flags &= !mask;
                                                }
                                            }
                                            if (idx + 1) % 2 == 0 {
                                                ui.end_row();
                                            }
                                        }
                                    });
                            });
                    } else {
                        ui.label("Terreno não encontrado no TileData.");
                    }
                } else {
                    ui.label("TileData não carregado.");
                }
            });

        // 3. Central Panel: Land Tile Graphic Preview
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(format!(
                    "Terreno 0x{:04X} ({})",
                    self.land_selected, self.land_selected
                ));

                if ui
                    .button("📋 Copiar Hex")
                    .on_hover_text("Copiar ID Hexadecimal para a área de transferência")
                    .clicked()
                {
                    let hex = format!("0x{:04X}", self.land_selected);
                    ctx.copy_text(hex.clone());
                    self.status_message = format!("✓ {} copiado!", hex);
                }

                if ui
                    .button("📋 Copiar Dec")
                    .on_hover_text("Copiar ID Decimal para a área de transferência")
                    .clicked()
                {
                    let dec = format!("{}", self.land_selected);
                    ctx.copy_text(dec.clone());
                    self.status_message = format!("✓ {} copiado!", dec);
                }

                if ui
                    .button("💾 Exportar PNG")
                    .on_hover_text("Salvar imagem do terreno na pasta 'exported_art/'")
                    .clicked()
                {
                    if let Some(bmp) = self.get_land_bitmap(self.land_selected) {
                        let filename = format!("land_0x{:04X}.png", self.land_selected);
                        if let Some(path) = export_rgba_png(
                            bmp.width as u32,
                            bmp.height as u32,
                            &bmp.rgba,
                            &filename,
                        ) {
                            self.status_message = format!("✓ Imagem exportada para {}", path);
                        }
                    }
                }

                let chk_label = if self.preview_checkerboard {
                    "🏁 Grade: Ligada"
                } else {
                    "⬛ Grade: Desligada"
                };
                if ui
                    .selectable_label(self.preview_checkerboard, chk_label)
                    .on_hover_text("Alternar fundo entre sólido e grade transparente")
                    .clicked()
                {
                    self.preview_checkerboard = !self.preview_checkerboard;
                }
            });

            ui.add_space(8.0);

            let avail = ui.available_size();
            let zoom_w = 46.0;
            let canvas_w = (avail.x - zoom_w - 10.0).max(100.0);
            let canvas_h = avail.y.max(100.0);

            ui.horizontal(|ui| {
                // 1:1 Canvas filling 100% of the entire remaining panel
                let (rect, resp) = ui.allocate_exact_size(
                    egui::vec2(canvas_w, canvas_h),
                    egui::Sense::click_and_drag(),
                );
                if self.preview_checkerboard {
                    draw_checkerboard(ui.painter(), rect, 16.0);
                } else {
                    ui.painter()
                        .rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 25, 20));
                }
                ui.painter().rect_stroke(
                    rect,
                    4.0,
                    egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(45, 55, 45)),
                );

                if resp.hovered() {
                    let scroll = ui.input(|i| i.raw_scroll_delta.y);
                    if scroll > 0.0 {
                        self.land_zoom = (self.land_zoom * 1.15).min(5.0);
                    } else if scroll < 0.0 {
                        self.land_zoom = (self.land_zoom / 1.15).max(0.5);
                    }
                }

                if let Some(tex) = &self.land_texture {
                    let center = rect.center();
                    let size = tex.size_vec2() * self.land_zoom; // 1:1 by default
                    let img_rect = egui::Rect::from_center_size(center, size);
                    ui.painter().image(
                        tex.id(),
                        img_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Nenhum terreno disponível",
                        egui::FontId::proportional(14.0),
                        egui::Color32::GRAY,
                    );
                }

                // Zoom tool on the right (neat vertical slider)
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(format!("{:.1}x", self.land_zoom)).small());
                    let slider_h = (canvas_h - 100.0).clamp(80.0, 220.0);
                    ui.add_sized(
                        [24.0, slider_h],
                        egui::Slider::new(&mut self.land_zoom, 0.5..=5.0)
                            .vertical()
                            .show_value(false),
                    );
                    ui.add_space(4.0);
                    if ui.button("1x").on_hover_text("Zoom 100%").clicked() {
                        self.land_zoom = 1.0;
                    }
                    if ui.button("2x").on_hover_text("Zoom 200%").clicked() {
                        self.land_zoom = 2.0;
                    }
                    if ui.button("3x").on_hover_text("Zoom 300%").clicked() {
                        self.land_zoom = 3.0;
                    }
                });
            });
        });
    }

    // 3. GUMPS TAB (Full Mythic UOP / MUL Decoding & 1:1 Image Canvas)
    fn render_gumps_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::SidePanel::left("gumps_left_panel")
            .resizable(true)
            .default_width(300.0)
            .min_width(220.0)
            .show_inside(ui, |ui| {
                ui.heading("Gump Art Viewer");

                // Source selector
                ui.horizontal(|ui| {
                    let classic_btn = ui.selectable_label(!self.gump_mode_enhanced, "📜 Clássico");
                    if classic_btn.clicked() && self.gump_mode_enhanced {
                        self.gump_mode_enhanced = false;
                        self.refresh_valid_gumps();
                        self.load_gump(ctx);
                    }
                    let enhanced_btn =
                        ui.selectable_label(self.gump_mode_enhanced, "✨ Enhanced (EC UOP)");
                    if enhanced_btn.clicked() && !self.gump_mode_enhanced {
                        self.gump_mode_enhanced = true;
                        self.refresh_valid_gumps();
                        self.load_gump(ctx);
                    }
                });

                ui.label(format!(
                    "Total de Gumps válidos: {}",
                    self.valid_gumps.len()
                ));

                ui.horizontal(|ui| {
                    ui.label("🔍 Buscar:");
                    ui.text_edit_singleline(&mut self.gump_search);
                    if !self.gump_search.is_empty() && ui.button("✖").clicked() {
                        self.gump_search.clear();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("🎯 Ir para ID:");
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.gump_jump_input)
                            .desired_width(80.0)
                            .hint_text("0x..."),
                    );
                    if (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        || ui.button("Ir").clicked()
                    {
                        if let Some(id) = parse_id_input(&self.gump_jump_input) {
                            if self.valid_gumps.contains(&(id as u32)) {
                                self.gump_id = id as u32;
                                self.keyboard_navigated = true;
                                self.load_gump(ctx);
                                self.status_message =
                                    format!("Navegado para Gump 0x{:04X} ({})", id, id);
                            }
                        }
                    }
                });

                ui.separator();
                let search_lower = self.gump_search.trim().to_lowercase();
                let filtered: Vec<u32> = if search_lower.is_empty() {
                    self.valid_gumps.clone()
                } else {
                    self.valid_gumps
                        .iter()
                        .copied()
                        .filter(|&id| {
                            format!("0x{:04x}", id).contains(&search_lower)
                                || id.to_string().contains(&search_lower)
                        })
                        .collect()
                };

                let row_height = 20.0;
                let total = filtered.len();

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, row_height, total, |ui, row_range| {
                        for idx in row_range {
                            let gid = filtered[idx];
                            let label = format!("Gump 0x{:04X} ({})", gid, gid);
                            let is_sel = self.gump_id == gid;
                            let resp = ui.selectable_label(is_sel, label);
                            if resp.clicked() {
                                self.gump_id = gid;
                                self.load_gump(ctx);
                            }
                            if is_sel && self.keyboard_navigated {
                                resp.scroll_to_me(Some(egui::Align::Center));
                            }
                        }
                    });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            if let Some((anim, female)) = self.equip_gump.anim_for_gump(self.gump_id) {
                let has_anim = self.is_action_defined(anim, 0)
                    || self.uop_anim_index.is_action_defined(anim, 0)
                    || (0..40).any(|a| self.is_mul_action_defined(anim, a));
                ui.horizontal(|ui| {
                    ui.label(format!("🔗 Animação de equipamento: body {} ({})", anim, if female { "♀" } else { "♂" }));
                    if has_anim && ui.button("Abrir em Animations").clicked() {
                        self.anim_body = anim;
                        self.anim_action = self.get_first_defined_action(anim);
                        self.anim_mode_enhanced = false;
                        self.active_tab = UoTab::Animations;
                        self.load_animation(ctx);
                    } else if !has_anim {
                        ui.weak("(sem animação nos arquivos)");
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.heading(format!(
                    "Gump 0x{:04X} ({}) - Dimensões: {} x {} px",
                    self.gump_id, self.gump_id, self.gump_dim.0, self.gump_dim.1
                ));

                if ui
                    .button("📋 Copiar Hex")
                    .on_hover_text("Copiar ID Hexadecimal para a área de transferência")
                    .clicked()
                {
                    let hex = format!("0x{:04X}", self.gump_id);
                    ctx.copy_text(hex.clone());
                    self.status_message = format!("✓ {} copiado!", hex);
                }

                if ui
                    .button("📋 Copiar Dec")
                    .on_hover_text("Copiar ID Decimal para a área de transferência")
                    .clicked()
                {
                    let dec = format!("{}", self.gump_id);
                    ctx.copy_text(dec.clone());
                    self.status_message = format!("✓ {} copiado!", dec);
                }

                if ui
                    .button("💾 Exportar PNG")
                    .on_hover_text("Salvar imagem do Gump na pasta 'exported_art/'")
                    .clicked()
                {
                    if let Some(bmp) = self.get_current_gump_bitmap() {
                        let filename = format!("gump_0x{:04X}.png", self.gump_id);
                        if let Some(path) = export_rgba_png(
                            bmp.width,
                            bmp.height,
                            &bmp.rgba,
                            &filename,
                        ) {
                            self.status_message = format!("✓ Imagem exportada para {}", path);
                        }
                    }
                }

                let chk_label = if self.preview_checkerboard {
                    "🏁 Grade: Ligada"
                } else {
                    "⬛ Grade: Desligada"
                };
                if ui
                    .selectable_label(self.preview_checkerboard, chk_label)
                    .on_hover_text("Alternar fundo entre sólido e grade transparente")
                    .clicked()
                {
                    self.preview_checkerboard = !self.preview_checkerboard;
                }
            });
            ui.separator();
            ui.add_space(6.0);

            let avail = ui.available_size();
            let zoom_w = 46.0;
            let canvas_w = (avail.x - zoom_w - 10.0).max(100.0);
            let canvas_h = avail.y.max(100.0);

            ui.horizontal(|ui| {
                // 1:1 Canvas filling 100% of the entire remaining panel
                let (rect, resp) = ui.allocate_exact_size(
                    egui::vec2(canvas_w, canvas_h),
                    egui::Sense::click_and_drag(),
                );
                if self.preview_checkerboard {
                    draw_checkerboard(ui.painter(), rect, 16.0);
                } else {
                    ui.painter()
                        .rect_filled(rect, 4.0, egui::Color32::from_rgb(18, 18, 22));
                }
                ui.painter().rect_stroke(
                    rect,
                    4.0,
                    egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(45, 45, 55)),
                );

                if resp.hovered() {
                    let scroll = ui.input(|i| i.raw_scroll_delta.y);
                    if scroll > 0.0 {
                        self.gump_zoom = (self.gump_zoom * 1.15).min(5.0);
                    } else if scroll < 0.0 {
                        self.gump_zoom = (self.gump_zoom / 1.15).max(0.5);
                    }
                }

                if let Some(tex) = &self.gump_texture {
                    let center = rect.center();
                    let size = tex.size_vec2() * self.gump_zoom;
                    let img_rect = egui::Rect::from_center_size(center, size);
                    ui.painter().image(
                        tex.id(),
                        img_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Nenhum Gump carregado",
                        egui::FontId::proportional(14.0),
                        egui::Color32::GRAY,
                    );
                }

                // Zoom tool on the right (neat vertical slider)
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(format!("{:.1}x", self.gump_zoom)).small());
                    let slider_h = (canvas_h - 100.0).clamp(80.0, 220.0);
                    ui.add_sized(
                        [24.0, slider_h],
                        egui::Slider::new(&mut self.gump_zoom, 0.2..=5.0)
                            .vertical()
                            .show_value(false),
                    );
                    ui.add_space(4.0);
                    if ui.button("1x").on_hover_text("Zoom 100%").clicked() {
                        self.gump_zoom = 1.0;
                    }
                    if ui.button("2x").on_hover_text("Zoom 200%").clicked() {
                        self.gump_zoom = 2.0;
                    }
                });
            });
        });
    }

    // 5. HUES TAB
    fn render_hues_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Hues Table (3.000 Cores e Gradientes)");
        ui.horizontal(|ui| {
            ui.label("Buscar Hue:");
            ui.text_edit_singleline(&mut self.hue_search);
            if ui.button("✖").clicked() {
                self.hue_search.clear();
            }
        });

        ui.separator();
        if let Some(hm) = &self.hues {
            let total_hues = hm.hues.len();
            let row_height = 22.0;
            let search_lower = self.hue_search.trim().to_lowercase();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, row_height, total_hues, |ui, row_range| {
                    for i in row_range {
                        let hue = &hm.hues[i];
                        let label = format!(
                            "Hue 0x{:04X} ({}) - {}",
                            hue.index + 1,
                            hue.index + 1,
                            hue.name
                        );
                        if !search_lower.is_empty() && !label.to_lowercase().contains(&search_lower)
                        {
                            continue;
                        }

                        ui.horizontal(|ui| {
                            let is_sel = self.hue_selected == hue.index as usize;
                            let resp = ui
                                .selectable_label(is_sel, egui::RichText::new(&label).monospace());
                            if resp.clicked() {
                                self.hue_selected = hue.index as usize;
                            }
                            if is_sel && self.keyboard_navigated {
                                resp.scroll_to_me(Some(egui::Align::Center));
                            }

                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(256.0, 14.0), egui::Sense::hover());
                            let pal = hue.get_rgba_palette();
                            let seg_w = rect.width() / 32.0;
                            for j in 0..32 {
                                let c = pal[j];
                                let color = egui::Color32::from_rgba_premultiplied(
                                    (c & 0xFF) as u8,
                                    ((c >> 8) & 0xFF) as u8,
                                    ((c >> 16) & 0xFF) as u8,
                                    255,
                                );
                                let seg_rect = egui::Rect::from_min_size(
                                    egui::pos2(rect.min.x + (j as f32) * seg_w, rect.min.y),
                                    egui::vec2(seg_w, rect.height()),
                                );
                                ui.painter().rect_filled(seg_rect, 0.0, color);
                            }
                        });
                    }
                });
        } else {
            ui.label("hues.mul não encontrado.");
        }
    }

    // 6. TEXTURES TAB (Natural 1:1 Size + Vertical Zoom Tool)
    fn render_textures_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::SidePanel::left("tex_left_panel")
            .resizable(true)
            .default_width(280.0)
            .min_width(220.0)
            .show_inside(ui, |ui| {
                ui.heading("Textures (texmaps.mul)");
                ui.label(format!("Total de texturas: {}", self.valid_textures.len()));

                ui.horizontal(|ui| {
                    ui.label("🔍 Buscar:");
                    ui.text_edit_singleline(&mut self.texture_search);
                    if !self.texture_search.is_empty() && ui.button("✖").clicked() {
                        self.texture_search.clear();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("🎯 Ir para ID:");
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.texture_jump_input)
                            .desired_width(80.0)
                            .hint_text("0x..."),
                    );
                    if (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        || ui.button("Ir").clicked()
                    {
                        if let Some(id) = parse_id_input(&self.texture_jump_input) {
                            if self.valid_textures.contains(&(id as u32)) {
                                self.texture_id = id as u32;
                                self.keyboard_navigated = true;
                                self.load_texture_preview(ctx);
                                self.status_message =
                                    format!("Navegado para Textura 0x{:04X} ({})", id, id);
                            }
                        }
                    }
                });

                ui.separator();
                let search_lower = self.texture_search.trim().to_lowercase();
                let filtered: Vec<u32> = if search_lower.is_empty() {
                    self.valid_textures.clone()
                } else {
                    self.valid_textures
                        .iter()
                        .copied()
                        .filter(|&id| {
                            format!("0x{:04x}", id).contains(&search_lower)
                                || id.to_string().contains(&search_lower)
                        })
                        .collect()
                };

                let row_height = 20.0;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, row_height, filtered.len(), |ui, row_range| {
                        if self.keyboard_navigated {
                            let pos = filtered
                                .iter()
                                .position(|&t| t == self.texture_id)
                                .unwrap_or(0);
                            let target_y = (pos as f32) * row_height;
                            ui.scroll_to_rect(
                                egui::Rect::from_min_size(
                                    egui::pos2(0.0, target_y),
                                    egui::vec2(10.0, row_height),
                                ),
                                Some(egui::Align::Center),
                            );
                        }
                        for idx in row_range {
                            let tid = filtered[idx];
                            let label = format!("Textura 0x{:04X} ({})", tid, tid);
                            let is_sel = self.texture_id == tid;
                            let resp = ui.selectable_label(is_sel, label);
                            if resp.clicked() {
                                self.texture_id = tid;
                                self.load_texture_preview(ctx);
                            }
                            if is_sel && self.keyboard_navigated {
                                resp.scroll_to_me(Some(egui::Align::Center));
                            }
                        }
                    });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(format!(
                    "Textura 0x{:04X} ({}) - Dimensões: {} x {} px",
                    self.texture_id, self.texture_id, self.texture_dim, self.texture_dim
                ));

                if ui
                    .button("📋 Copiar Hex")
                    .on_hover_text("Copiar ID Hexadecimal para a área de transferência")
                    .clicked()
                {
                    let hex = format!("0x{:04X}", self.texture_id);
                    ctx.copy_text(hex.clone());
                    self.status_message = format!("✓ {} copiado!", hex);
                }

                if ui
                    .button("📋 Copiar Dec")
                    .on_hover_text("Copiar ID Decimal para a área de transferência")
                    .clicked()
                {
                    let dec = format!("{}", self.texture_id);
                    ctx.copy_text(dec.clone());
                    self.status_message = format!("✓ {} copiado!", dec);
                }

                if ui
                    .button("💾 Exportar PNG")
                    .on_hover_text("Salvar imagem da textura na pasta 'exported_art/'")
                    .clicked()
                {
                    if let Some(bmp) = self.get_current_texture_bitmap() {
                        let filename = format!("texture_0x{:04X}.png", self.texture_id);
                        if let Some(path) = export_rgba_png(
                            bmp.width as u32,
                            bmp.height as u32,
                            &bmp.rgba,
                            &filename,
                        ) {
                            self.status_message = format!("✓ Imagem exportada para {}", path);
                        }
                    }
                }

                let chk_label = if self.preview_checkerboard {
                    "🏁 Grade: Ligada"
                } else {
                    "⬛ Grade: Desligada"
                };
                if ui
                    .selectable_label(self.preview_checkerboard, chk_label)
                    .on_hover_text("Alternar fundo entre sólido e grade transparente")
                    .clicked()
                {
                    self.preview_checkerboard = !self.preview_checkerboard;
                }
            });
            ui.separator();
            ui.add_space(6.0);

            let avail = ui.available_size();
            let zoom_w = 46.0;
            let canvas_w = (avail.x - zoom_w - 10.0).max(100.0);
            let canvas_h = avail.y.max(100.0);

            ui.horizontal(|ui| {
                // 1:1 Canvas filling 100% of the entire remaining panel
                let (rect, resp) = ui.allocate_exact_size(
                    egui::vec2(canvas_w, canvas_h),
                    egui::Sense::click_and_drag(),
                );
                if self.preview_checkerboard {
                    draw_checkerboard(ui.painter(), rect, 16.0);
                } else {
                    ui.painter()
                        .rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 20, 24));
                }
                ui.painter().rect_stroke(
                    rect,
                    4.0,
                    egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(45, 45, 55)),
                );

                if resp.hovered() {
                    let scroll = ui.input(|i| i.raw_scroll_delta.y);
                    if scroll > 0.0 {
                        self.texture_zoom = (self.texture_zoom * 1.15).min(5.0);
                    } else if scroll < 0.0 {
                        self.texture_zoom = (self.texture_zoom / 1.15).max(0.5);
                    }
                }

                if let Some(tex) = &self.texture_texture {
                    let center = rect.center();
                    let size = tex.size_vec2() * self.texture_zoom; // 1:1 by default
                    let img_rect = egui::Rect::from_center_size(center, size);
                    ui.painter().image(
                        tex.id(),
                        img_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Nenhuma textura carregada",
                        egui::FontId::proportional(14.0),
                        egui::Color32::GRAY,
                    );
                }

                // Zoom tool on the right (neat vertical slider)
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(format!("{:.1}x", self.texture_zoom)).small());
                    let slider_h = (canvas_h - 100.0).clamp(80.0, 220.0);
                    ui.add_sized(
                        [24.0, slider_h],
                        egui::Slider::new(&mut self.texture_zoom, 0.5..=5.0)
                            .vertical()
                            .show_value(false),
                    );
                    ui.add_space(4.0);
                    if ui.button("1x").on_hover_text("Zoom 100%").clicked() {
                        self.texture_zoom = 1.0;
                    }
                    if ui.button("2x").on_hover_text("Zoom 200%").clicked() {
                        self.texture_zoom = 2.0;
                    }
                    if ui.button("3x").on_hover_text("Zoom 300%").clicked() {
                        self.texture_zoom = 3.0;
                    }
                });
            });
        });
    }

    // 7. RADARCOL TAB
    fn render_radar_col_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::SidePanel::left("radar_left_panel")
            .resizable(true)
            .default_width(320.0)
            .min_width(260.0)
            .show_inside(ui, |ui| {
                ui.heading("RadarCol Table");
                ui.horizontal(|ui| {
                    if ui.selectable_label(self.radar_is_items, "Items").clicked() {
                        self.radar_is_items = true;
                        self.load_radar_preview(ctx);
                    }
                    if ui
                        .selectable_label(!self.radar_is_items, "Land Tiles")
                        .clicked()
                    {
                        self.radar_is_items = false;
                        self.load_radar_preview(ctx);
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Buscar:");
                    ui.text_edit_singleline(&mut self.radar_search);
                    if ui.button("✖").clicked() {
                        self.radar_search.clear();
                    }
                });

                ui.separator();
                let total = if self.radar_is_items { 0x10000 } else { 0x4000 };
                let row_height = 22.0;
                let search_lower = self.radar_search.trim().to_lowercase();
                let radar_ref = self.radar_col.as_ref();
                let mut clicked_id = None;

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, row_height, total, |ui, row_range| {
                        for id in row_range {
                            let label = format!("0x{:04X} ({})", id, id);
                            if !search_lower.is_empty()
                                && !label.to_lowercase().contains(&search_lower)
                            {
                                continue;
                            }

                            ui.horizontal(|ui| {
                                let is_sel = self.radar_selected == id;
                                let resp = ui.selectable_label(
                                    is_sel,
                                    egui::RichText::new(&label).monospace(),
                                );
                                if resp.clicked() {
                                    clicked_id = Some(id);
                                }
                                if is_sel && self.keyboard_navigated {
                                    resp.scroll_to_me(Some(egui::Align::Center));
                                }

                                if let Some(rc) = radar_ref {
                                    let col = if self.radar_is_items {
                                        rc.get_item_rgba(id)
                                    } else {
                                        rc.get_land_rgba(id)
                                    };
                                    let color32 = egui::Color32::from_rgba_premultiplied(
                                        col[0], col[1], col[2], col[3],
                                    );
                                    let (swatch_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(28.0, 14.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(swatch_rect, 2.0, color32);
                                }
                            });
                        }
                    });

                if let Some(id) = clicked_id {
                    self.radar_selected = id;
                    self.load_radar_preview(ctx);
                }
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading(format!(
                "Cor de Radar - 0x{:04X} ({})",
                self.radar_selected, self.radar_selected
            ));
            ui.separator();

            if let Some(rc) = &self.radar_col {
                let col = if self.radar_is_items {
                    rc.get_item_rgba(self.radar_selected)
                } else {
                    rc.get_land_rgba(self.radar_selected)
                };
                let raw_c16 = if self.radar_is_items {
                    rc.get_item_color(self.radar_selected)
                } else {
                    rc.get_land_color(self.radar_selected)
                };

                ui.horizontal(|ui| {
                    let color32 =
                        egui::Color32::from_rgba_premultiplied(col[0], col[1], col[2], col[3]);
                    let (swatch_rect, _) =
                        ui.allocate_exact_size(egui::vec2(80.0, 40.0), egui::Sense::hover());
                    ui.painter().rect_filled(swatch_rect, 4.0, color32);
                    ui.painter().rect_stroke(
                        swatch_rect,
                        4.0,
                        egui::Stroke::new(1.0_f32, egui::Color32::WHITE),
                    );

                    ui.vertical(|ui| {
                        ui.label(format!("Valor 16-bit: 0x{:04X}", raw_c16));
                        ui.label(format!(
                            "RGBA: rgba({}, {}, {}, {})",
                            col[0], col[1], col[2], col[3]
                        ));
                    });
                });
            }

            ui.add_space(12.0);
            ui.heading("Arte do Tile (Tamanho Real):");
            egui::Frame::canvas(ui.style()).show(ui, |ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(200.0, 200.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 20, 25));

                if let Some(tex) = &self.radar_preview_tex {
                    let center = rect.center();
                    let size = tex.size_vec2(); // Real 1:1 size
                    let img_rect = egui::Rect::from_center_size(center, size);
                    ui.painter().image(
                        tex.id(),
                        img_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Sem imagem",
                        egui::FontId::proportional(14.0),
                        egui::Color32::GRAY,
                    );
                }
            });
        });
    }

    // 8. ANIMATIONS TAB (1,300+ Creatures, Full Categories, Search, Actions & Vertical Zoom Tool)
    fn render_animations_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::SidePanel::left("anim_left_panel")
            .resizable(true)
            .default_width(340.0)
            .min_width(280.0)
            .show_inside(ui, |ui| {
                ui.heading("Controle de Animações");
                ui.add_space(4.0);

                // Source selector: Clássico vs Enhanced UOP
                ui.horizontal(|ui| {
                    let classic_btn =
                        ui.selectable_label(!self.anim_mode_enhanced, "📜 Clássico (Anim.mul)");
                    if classic_btn.clicked() && self.anim_mode_enhanced {
                        self.anim_mode_enhanced = false;
                        if self.anim_category_filter == AnimCategoryFilter::UopModern {
                            self.anim_category_filter = AnimCategoryFilter::All;
                        }
                        let search_lower = self.anim_search.trim().to_lowercase();
                        let cur_def = self
                            .anim_body_defs
                            .iter()
                            .find(|d| d.body == self.anim_body);
                        let cur_valid = cur_def
                            .map(|d| self.anim_def_matches(d, &search_lower))
                            .unwrap_or(false);
                        if !cur_valid {
                            if let Some(first) = self
                                .anim_body_defs
                                .iter()
                                .find(|d| self.anim_def_matches(d, &search_lower))
                            {
                                self.anim_body = first.body;
                            }
                        }
                        self.anim_action = self.get_first_defined_action(self.anim_body);
                        self.load_animation(ctx);
                    }
                    let enhanced_btn =
                        ui.selectable_label(self.anim_mode_enhanced, "✨ Enhanced (UOP)");
                    if enhanced_btn.clicked() && !self.anim_mode_enhanced {
                        self.anim_mode_enhanced = true;
                        let search_lower = self.anim_search.trim().to_lowercase();
                        let cur_def = self
                            .anim_body_defs
                            .iter()
                            .find(|d| d.body == self.anim_body);
                        let cur_valid = cur_def
                            .map(|d| self.anim_def_matches(d, &search_lower))
                            .unwrap_or(false);
                        if !cur_valid {
                            if let Some(first) = self
                                .anim_body_defs
                                .iter()
                                .find(|d| self.anim_def_matches(d, &search_lower))
                            {
                                self.anim_body = first.body;
                            }
                        }
                        self.anim_action = self.get_first_defined_action(self.anim_body);
                        self.load_animation(ctx);
                    }
                });
                ui.add_space(4.0);

                // Category and search
                ui.group(|ui| {
                    ui.label("🔍 Filtrar Criaturas:");
                    ui.horizontal(|ui| {
                        if ui.text_edit_singleline(&mut self.anim_search).changed() {
                            let search_lower = self.anim_search.trim().to_lowercase();
                            let cur_def = self
                                .anim_body_defs
                                .iter()
                                .find(|d| d.body == self.anim_body);
                            let cur_valid = cur_def
                                .map(|d| self.anim_def_matches(d, &search_lower))
                                .unwrap_or(false);
                            if !cur_valid {
                                if let Some(first) = self
                                    .anim_body_defs
                                    .iter()
                                    .find(|d| self.anim_def_matches(d, &search_lower))
                                {
                                    self.anim_body = first.body;
                                    self.anim_action = self.get_first_defined_action(first.body);
                                    self.load_animation(ctx);
                                }
                            }
                        }
                        if ui.button("✖").clicked() {
                            self.anim_search.clear();
                        }
                    });

                    ui.horizontal(|ui| {
                        let cat_label = match self.anim_category_filter {
                            AnimCategoryFilter::All => "Todos",
                            AnimCategoryFilter::Monsters => "Monstros (0)",
                            AnimCategoryFilter::SeaMonsters => "Criaturas Marinhas (1)",
                            AnimCategoryFilter::Animals => "Animais (2)",
                            AnimCategoryFilter::Humanoids => "Humanos / Elfos (3)",
                            AnimCategoryFilter::Equipment => "Equipamentos / Armaduras",
                            AnimCategoryFilter::UopModern => "UOP Modernos (Stygian/TOL)",
                        };

                        let old_cat = self.anim_category_filter;
                        egui::ComboBox::from_label("Categoria")
                            .selected_text(cat_label)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.anim_category_filter,
                                    AnimCategoryFilter::All,
                                    "Todos",
                                );
                                ui.selectable_value(
                                    &mut self.anim_category_filter,
                                    AnimCategoryFilter::Monsters,
                                    "Monstros (0)",
                                );
                                ui.selectable_value(
                                    &mut self.anim_category_filter,
                                    AnimCategoryFilter::SeaMonsters,
                                    "Criaturas Marinhas (1)",
                                );
                                ui.selectable_value(
                                    &mut self.anim_category_filter,
                                    AnimCategoryFilter::Animals,
                                    "Animais (2)",
                                );
                                ui.selectable_value(
                                    &mut self.anim_category_filter,
                                    AnimCategoryFilter::Humanoids,
                                    "Humanos / Elfos (3)",
                                );
                                ui.selectable_value(
                                    &mut self.anim_category_filter,
                                    AnimCategoryFilter::Equipment,
                                    "Equipamentos / Armaduras",
                                );
                                ui.selectable_value(
                                    &mut self.anim_category_filter,
                                    AnimCategoryFilter::UopModern,
                                    "UOP Modernos (Stygian/TOL)",
                                );
                            });

                        if self.anim_category_filter != old_cat {
                            if self.anim_category_filter == AnimCategoryFilter::UopModern {
                                self.anim_mode_enhanced = true;
                            }
                            let search_lower = self.anim_search.trim().to_lowercase();
                            let cur_def = self
                                .anim_body_defs
                                .iter()
                                .find(|d| d.body == self.anim_body);
                            let cur_valid = cur_def
                                .map(|d| self.anim_def_matches(d, &search_lower))
                                .unwrap_or(false);
                            if !cur_valid {
                                if let Some(first) = self
                                    .anim_body_defs
                                    .iter()
                                    .find(|d| self.anim_def_matches(d, &search_lower))
                                {
                                    self.anim_body = first.body;
                                    self.anim_action = self.get_first_defined_action(first.body);
                                    self.load_animation(ctx);
                                }
                            }
                        }
                    });

                    let search_lower = self.anim_search.trim().to_lowercase();
                    let current_body_name = self
                        .anim_body_defs
                        .iter()
                        .find(|d| d.body == self.anim_body)
                        .map(|d| d.name.as_str())
                        .unwrap_or("Personalizado");

                    let mut chosen_body: Option<u32> = None;
                    let current_body = self.anim_body;

                    egui::ComboBox::from_label("Corpo / Criatura")
                        .selected_text(format!("{} - {}", self.anim_body, current_body_name))
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(280.0)
                                .show(ui, |ui| {
                                    let mut count = 0;
                                    for def in &self.anim_body_defs {
                                        if !self.anim_def_matches(def, &search_lower) {
                                            continue;
                                        }

                                        let is_uop = def.is_uop
                                            || self.uop_anim_index.is_action_defined(def.body, 0)
                                            || self.uop_anim_index.is_action_defined(def.body, 1);
                                        let tag = if is_uop { "[UOP]" } else { "[MUL]" };
                                        let is_sel = current_body == def.body;
                                        let label = format!("{} {} - {}", tag, def.body, def.name);
                                        if ui.selectable_label(is_sel, label).clicked() {
                                            chosen_body = Some(def.body);
                                        }
                                        count += 1;
                                        if count >= 600 {
                                            ui.label("... (refine a busca)");
                                            break;
                                        }
                                    }
                                });
                        });

                    if let Some(b) = chosen_body {
                        self.anim_body = b;
                        self.anim_action = self.get_first_defined_action(b);
                        self.load_animation(ctx);
                    }

                    ui.horizontal(|ui| {
                        ui.label("Body ID Manual:");
                        if ui
                            .add(egui::DragValue::new(&mut self.anim_body).range(0..=65535))
                            .changed()
                        {
                            self.anim_action = self.get_first_defined_action(self.anim_body);
                            self.load_animation(ctx);
                        }
                    });

                    ui.separator();
                    ui.label("Seleção de Ação:");

                    // Dynamically pick actions based on body type
                    let cur_def = self
                        .anim_body_defs
                        .iter()
                        .find(|d| d.body == self.anim_body);
                    let anim_type = cur_def.map(|d| d.anim_type).unwrap_or(
                        if self.anim_body >= 400 && self.anim_body <= 403 {
                            3
                        } else {
                            0
                        },
                    );

                    let actions: &[(u32, &str)] = match anim_type {
                        1 => &ACTIONS_SEA,
                        2 => &ACTIONS_ANIMAL,
                        3 => &ACTIONS_HUMAN,
                        _ => &ACTIONS_MONSTER,
                    };

                    let current_action_name = actions
                        .iter()
                        .find(|(act, _)| *act == self.anim_action)
                        .map(|(_, n)| *n)
                        .unwrap_or("Ação");

                    egui::ComboBox::from_label("Ação")
                        .selected_text(format!("{} - {}", self.anim_action, current_action_name))
                        .show_ui(ui, |ui| {
                            for (act, name) in actions {
                                let is_def = self.is_action_defined(self.anim_body, *act);
                                let check = if is_def { "✓ " } else { "   " };
                                if ui
                                    .selectable_value(
                                        &mut self.anim_action,
                                        *act,
                                        format!("{}{:02} - {}", check, act, name),
                                    )
                                    .clicked()
                                {
                                    self.load_animation(ctx);
                                }
                            }
                            if self.anim_mode_enhanced {
                                for extra_act in (actions.len() as u32)..60 {
                                    if self
                                        .uop_anim_index
                                        .is_action_defined(self.anim_body, extra_act)
                                    {
                                        if ui
                                            .selectable_value(
                                                &mut self.anim_action,
                                                extra_act,
                                                format!("✓ {:02} - Extra {}", extra_act, extra_act),
                                            )
                                            .clicked()
                                        {
                                            self.load_animation(ctx);
                                        }
                                    }
                                }
                            }
                        });

                    ui.horizontal(|ui| {
                        ui.label("Ação ID:");
                        if ui
                            .add(egui::DragValue::new(&mut self.anim_action).range(0..=80))
                            .changed()
                        {
                            self.load_animation(ctx);
                        }
                    });

                    ui.separator();
                    // Direction ComboBox
                    let current_dir_name = DIRECTIONS
                        .iter()
                        .find(|(d, _)| *d == self.anim_direction)
                        .map(|(_, n)| *n)
                        .unwrap_or("0");
                    egui::ComboBox::from_label("Direção")
                        .selected_text(current_dir_name)
                        .show_ui(ui, |ui| {
                            for (d, name) in DIRECTIONS {
                                if ui
                                    .selectable_value(&mut self.anim_direction, d, name)
                                    .clicked()
                                {
                                    self.load_animation(ctx);
                                }
                            }
                        });

                    ui.add_space(4.0);
                    if ui.button("⚡ Recarregar Animação").clicked() {
                        self.load_animation(ctx);
                    }
                });

                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.label("Reprodução em Tempo Real:");
                    ui.horizontal(|ui| {
                        let play_btn_text = if self.anim_playing {
                            "⏸ Pausar"
                        } else {
                            "▶ Reproduzir"
                        };
                        if ui.button(play_btn_text).clicked() {
                            self.anim_playing = !self.anim_playing;
                        }

                        if ui.button("◀").clicked() && !self.anim_frames.is_empty() {
                            self.anim_current_frame_idx = if self.anim_current_frame_idx == 0 {
                                self.anim_frames.len() - 1
                            } else {
                                self.anim_current_frame_idx - 1
                            };
                            self.update_animation_texture(ctx);
                        }

                        if ui.button("▶").clicked() && !self.anim_frames.is_empty() {
                            self.anim_current_frame_idx =
                                (self.anim_current_frame_idx + 1) % self.anim_frames.len();
                            self.update_animation_texture(ctx);
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Velocidade:");
                        ui.add(egui::Slider::new(&mut self.anim_fps, 1..=30).text("fps"));
                    });

                    let total_frames = self.anim_frames.len();
                    ui.label(format!(
                        "Quadro: {} / {}",
                        if total_frames == 0 {
                            0
                        } else {
                            self.anim_current_frame_idx + 1
                        },
                        total_frames
                    ));
                });
            });

        // Zoom tool on the right (full vertical height)
        egui::SidePanel::right("anim_zoom_panel")
            .resizable(false)
            .exact_width(52.0)
            .frame(egui::Frame::none())
            .show_inside(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new(format!("{:.1}x", self.anim_zoom)).small());
                    ui.add_sized(
                        [24.0, 180.0],
                        egui::Slider::new(&mut self.anim_zoom, 0.5..=5.0)
                            .vertical()
                            .show_value(false),
                    );
                    ui.add_space(6.0);
                    if ui.button("1x").clicked() {
                        self.anim_zoom = 1.0;
                    }
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading(format!(
                "Visualizador - Body {} | Ação {} | Dir {}",
                self.anim_body, self.anim_action, self.anim_direction
            ));
            // Link animation -> paperdoll gump (equipment animations map to gump 50000+/60000+)
            {
                let gi = self.gump_index.as_ref();
                let exists = |g: u32| gi.map_or(false, |f| f.is_valid(g as usize));
                let (gm, _) = self.equip_gump.gump_for_anim(400, self.anim_body, false, exists);
                let (gf, _) = self.equip_gump.gump_for_anim(401, self.anim_body, true, exists);
                let mut goto: Option<u32> = None;
                ui.horizontal(|ui| {
                    ui.label("🔗 Gump:");
                    if exists(gm) && ui.button(format!("♂ {} (0x{:04X})", gm, gm)).clicked() {
                        goto = Some(gm);
                    }
                    if gf != gm && exists(gf) && ui.button(format!("♀ {} (0x{:04X})", gf, gf)).clicked() {
                        goto = Some(gf);
                    }
                    if !exists(gm) && !exists(gf) {
                        ui.weak("nenhum gump associado a este corpo");
                    }
                });
                if let Some(g) = goto {
                    self.gump_id = g;
                    self.gump_mode_enhanced = false;
                    self.active_tab = UoTab::Gumps;
                    self.load_gump(ctx);
                }
            }
            ui.add_space(8.0);

            let available = ui.available_size();
            let (rect, resp) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());
            ui.painter()
                .rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 20, 25));
            ui.painter().rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(45, 45, 55)),
            );

            if resp.hovered() {
                let scroll = ui.input(|i| i.raw_scroll_delta.y);
                if scroll > 0.0 {
                    self.anim_zoom = (self.anim_zoom * 1.15).min(5.0);
                } else if scroll < 0.0 {
                    self.anim_zoom = (self.anim_zoom / 1.15).max(0.5);
                }
            }

            if let Some(texture) = &self.anim_texture {
                let center = rect.center();
                let size = texture.size_vec2() * self.anim_zoom; // Natural 1:1 with zoom
                let img_rect = egui::Rect::from_center_size(center, size);
                ui.painter().image(
                    texture.id(),
                    img_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            } else {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Nenhuma animação carregada",
                    egui::FontId::proportional(14.0),
                    egui::Color32::GRAY,
                );
            }
        });
    }

    // 9. CLILOC TAB
    fn render_cliloc_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Cliloc Localization Table");
        ui.horizontal(|ui| {
            ui.label("Buscar Cliloc:");
            ui.text_edit_singleline(&mut self.cliloc_search);
            if ui.button("✖").clicked() {
                self.cliloc_search.clear();
            }
        });

        ui.separator();
        if let Some(cl) = &self.cliloc {
            let total_entries = cl.entries.len();
            let row_height = 20.0;
            let search_lower = self.cliloc_search.trim().to_lowercase();

            if search_lower.is_empty() {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, row_height, total_entries, |ui, row_range| {
                        for i in row_range {
                            let entry = &cl.entries[i];
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{:8}", entry.number))
                                        .monospace()
                                        .color(egui::Color32::from_rgb(159, 168, 218)),
                                );
                                ui.label(&entry.text);
                            });
                        }
                    });
            } else {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut count = 0;
                        for entry in &cl.entries {
                            if !entry.number.to_string().contains(&search_lower)
                                && !entry.text.to_lowercase().contains(&search_lower)
                            {
                                continue;
                            }

                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{:8}", entry.number))
                                        .monospace()
                                        .color(egui::Color32::from_rgb(159, 168, 218)),
                                );
                                ui.label(&entry.text);
                            });
                            count += 1;
                            if count >= 500 {
                                break;
                            }
                        }
                    });
            }
        } else {
            ui.label("cliloc.enu não encontrado.");
        }
    }

    // 10. SOUNDS TAB
    fn render_sounds_tab(&mut self, ui: &mut egui::Ui) {
        egui::SidePanel::left("sounds_left_panel")
            .resizable(true)
            .default_width(320.0)
            .min_width(260.0)
            .show_inside(ui, |ui| {
                ui.heading("Sounds (sound.mul)");
                ui.label(format!(
                    "Total de efeitos sonoros: {}",
                    self.valid_sounds.len()
                ));

                ui.horizontal(|ui| {
                    ui.label("Buscar:");
                    ui.text_edit_singleline(&mut self.sound_search);
                    if ui.button("✖").clicked() {
                        self.sound_search.clear();
                    }
                });

                ui.separator();
                let search_lower = self.sound_search.trim().to_lowercase();
                let filtered_indices: Vec<usize> = if search_lower.is_empty() {
                    (0..self.valid_sounds.len()).collect()
                } else {
                    self.valid_sounds
                        .iter()
                        .enumerate()
                        .filter_map(|(i, s)| {
                            let hex = format!("0x{:03x}", s.sound_id);
                            if hex.contains(&search_lower)
                                || s.sound_id.to_string().contains(&search_lower)
                                || s.name.to_lowercase().contains(&search_lower)
                            {
                                Some(i)
                            } else {
                                None
                            }
                        })
                        .collect()
                };

                let row_height = 20.0;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, row_height, filtered_indices.len(), |ui, row_range| {
                        for idx in row_range {
                            let sound_idx = filtered_indices[idx];
                            let snd = &self.valid_sounds[sound_idx];
                            let label = format!("0x{:03X} - {}", snd.sound_id, snd.name);
                            let is_sel = self.sound_selected == sound_idx;
                            let resp = ui.selectable_label(is_sel, label);
                            if resp.clicked() {
                                self.sound_selected = sound_idx;
                            }
                            if is_sel && self.keyboard_navigated {
                                resp.scroll_to_me(Some(egui::Align::Center));
                            }
                        }
                    });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("Detalhes e Reprodução de Áudio");
            ui.separator();

            if let Some(snd) = self.valid_sounds.get(self.sound_selected) {
                ui.label(format!(
                    "Sound ID: {} (0x{:03X})",
                    snd.sound_id, snd.sound_id
                ));
                ui.label(format!("Nome do Arquivo: {}", snd.name));
                ui.label(format!(
                    "Tamanho PCM: {} bytes (WAV 22050Hz Mono)",
                    snd.pcm_data.len()
                ));

                ui.add_space(16.0);
                if ui
                    .button(
                        egui::RichText::new("▶ Tocar Som")
                            .size(18.0)
                            .color(egui::Color32::from_rgb(102, 187, 106)),
                    )
                    .clicked()
                {
                    snd.play();
                }
            } else {
                ui.label("Nenhum som selecionado.");
            }
        });
    }

    // 11. MAP TAB (Full Canvas Fill, Dynamic Blocks, Zoom & Contextual Presets)
    fn render_map_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut map_changed = false;

        ui.horizontal(|ui| {
            ui.label("🖥 Modo:");
            for (m, name) in [
                (MapMode::Radar, "📡 Radar"),
                (MapMode::Live, "🎮 Isometric"),
                (MapMode::Overview, "🗺 Visão geral (facet)"),
            ] {
                if ui.selectable_label(self.map_mode == m, name).clicked() {
                    self.map_mode = m;
                    map_changed = true;
                }
            }
            ui.separator();
            let (dl, ds) = self.map_mgr.diff_counts(self.map_id);
            let has_diff = dl + ds > 0;
            ui.add_enabled_ui(has_diff, |ui| {
                if ui
                    .checkbox(&mut self.map_use_diff, "mapdif")
                    .on_hover_text(if has_diff {
                        format!("Aplicar patches mapdif/stadif desta faceta ({} blocos de terreno, {} de statics)", dl, ds)
                    } else {
                        "Esta faceta não tem arquivos mapdif/stadif".to_string()
                    })
                    .changed()
                {
                    map_changed = true;
                }
            });
            if self.map_mode == MapMode::Overview && self.map_id <= 1 {
                ui.separator();
                if ui.checkbox(&mut self.overview_multimap, "Multimap.rle").changed() {
                    map_changed = true;
                }
            }
            if self.map_mode == MapMode::Live {
                ui.separator();
                ui.label("Zoom:");
                ui.add(egui::Slider::new(&mut self.map_panel.view.zoom, map::view::MIN_ZOOM..=map::view::MAX_ZOOM));
                ui.label("Max Z:");
                if ui
                    .add(egui::Slider::new(&mut self.map_max_z, -128..=127))
                    .on_hover_text("Hide everything above this height (peel off roofs and upper floors)")
                    .changed()
                {
                    self.map_panel.view.cut.cut = (self.map_max_z < 127).then_some(self.map_max_z);
                }
                ui.checkbox(&mut self.map_panel.view.cut.hide_roofs, "Hide roofs");
                ui.checkbox(&mut self.map_panel.view.show_grid, "Grid");
            }
        });

        ui.horizontal(|ui| {
            ui.label("🌐 Faceta:");
            for (m_id, name) in [
                (0, "0: Felucca"),
                (1, "1: Trammel"),
                (2, "2: Ilshenar"),
                (3, "3: Malas"),
                (4, "4: Tokuno"),
                (5, "5: Ter Mur"),
            ] {
                if ui.selectable_label(self.map_id == m_id, name).clicked() {
                    self.map_id = m_id;
                    map_changed = true;
                }
            }
        });

        // Contextual Presets belonging ONLY to current map_id
        ui.horizontal_wrapped(|ui| {
            ui.label("📍 Presets desta Faceta:");
            for (m, name, x, y) in PRESET_LOCATIONS {
                if m == self.map_id {
                    if ui.button(name).clicked() {
                        self.map_x = x;
                        self.map_y = y;
                        map_changed = true;
                    }
                }
            }
        });

        ui.horizontal(|ui| {
            ui.label("Coord X:");
            if ui
                .add(egui::DragValue::new(&mut self.map_x).range(0..=7168))
                .changed()
            {
                map_changed = true;
            }
            ui.label("Y:");
            if ui
                .add(egui::DragValue::new(&mut self.map_y).range(0..=4096))
                .changed()
            {
                map_changed = true;
            }
            if self.map_mode == MapMode::Radar {
                ui.label("Zoom:");
                if ui
                    .add(egui::Slider::new(&mut self.map_zoom, 0.5..=4.0).text("x"))
                    .changed()
                {
                    map_changed = true;
                }
            }
            if ui.checkbox(&mut self.map_show_statics, "Statics").changed() {
                map_changed = true;
            }
        });

        ui.separator();

        if self.map_mode == MapMode::Overview {
            self.show_map_overview(ui, ctx);
            return;
        }
        if self.map_mode == MapMode::Live {
            self.map_panel.view.show_statics = self.map_show_statics;
            let mut res = map::MapRes {
                maps: &mut self.map_mgr,
                art: self.art_index.as_ref(),
                textures: self.texture_index.as_ref(),
                tiledata: self.tiledata.as_ref(),
                hues: self.hues.as_ref(),
                radar: self.radar_col.as_ref(),
            };
            self.map_panel.show(ui, &mut res, self.map_id, &mut self.map_x, &mut self.map_y);
            return;
        }

        // Canvas that fills 100% of the entire area
        egui::Frame::canvas(ui.style()).show(ui, |ui| {
            let available = ui.available_size();
            let (rect, resp) = ui.allocate_exact_size(available, egui::Sense::drag());
            ui.painter()
                .rect_filled(rect, 4.0, egui::Color32::from_rgb(15, 20, 18));

            if resp.dragged() {
                let delta = resp.drag_delta();
                let (max_x, max_y) = (
                    self.map_mgr.actual_widths[self.map_id].saturating_sub(1) as f32,
                    (MAP_DEFS[self.map_id].height - 1) as f32,
                );
                let pan_scale = (1.0 / self.map_zoom).max(0.25);
                self.map_x = (self.map_x as f32 - (delta.x * pan_scale)).clamp(0.0, max_x) as u32;
                self.map_y = (self.map_y as f32 - (delta.y * pan_scale)).clamp(0.0, max_y) as u32;
                map_changed = true;
            }

            if resp.hovered() {
                let scroll_y = ui.input(|i| i.raw_scroll_delta.y);
                if scroll_y != 0.0 {
                    let factor = if scroll_y > 0.0 { 1.15 } else { 0.85 };
                    let new_zoom = (self.map_zoom * factor).clamp(0.5, 4.0);
                    if (new_zoom - self.map_zoom).abs() > 0.01 {
                        self.map_zoom = new_zoom;
                        map_changed = true;
                    }
                }
            }

            let size_diff = (available - self.map_last_render_size).length();
            if map_changed || self.map_texture.is_none() || size_diff > 2.0 {
                self.map_last_render_size = available;
                self.update_map_view_custom(ctx, available.x, available.y);
            }

            if let Some(tex) = &self.map_texture {
                let center = rect.center();
                let size = rect.size(); // Fill entire allocated canvas rect
                let img_rect = egui::Rect::from_center_size(center, size);
                ui.painter().image(
                    tex.id(),
                    img_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            } else {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("Carregando mapa {}...", self.map_id),
                    egui::FontId::proportional(16.0),
                    egui::Color32::from_rgb(180, 200, 190),
                );
            }
        });
    }

    // 12. MULTIS TAB
    fn render_multis_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::SidePanel::left("multis_left_panel")
            .resizable(true)
            .default_width(320.0)
            .min_width(260.0)
            .show_inside(ui, |ui| {
                ui.heading("Controle de Multis");
                ui.add_space(4.0);

                // Source selector: Clássico vs Enhanced
                ui.horizontal(|ui| {
                    let classic_btn = ui.selectable_label(!self.multi_mode_enhanced, "📜 Clássico");
                    if classic_btn.clicked() && self.multi_mode_enhanced {
                        self.multi_mode_enhanced = false;
                        self.refresh_valid_multis();
                        self.render_current_multi(ctx);
                    }
                    let enhanced_btn =
                        ui.selectable_label(self.multi_mode_enhanced, "✨ Enhanced (UOP)");
                    if enhanced_btn.clicked() && !self.multi_mode_enhanced {
                        self.multi_mode_enhanced = true;
                        self.refresh_valid_multis();
                        self.render_current_multi(ctx);
                    }
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("🔍 Buscar:");
                    ui.text_edit_singleline(&mut self.multi_search);
                    if !self.multi_search.is_empty() && ui.button("✖").clicked() {
                        self.multi_search.clear();
                    }
                });

                let search_lower = self.multi_search.trim().to_lowercase();
                let filtered: Vec<u32> = self
                    .valid_multis
                    .iter()
                    .copied()
                    .filter(|&id| {
                        if search_lower.is_empty() {
                            return true;
                        }
                        if format!("0x{:04x}", id).contains(&search_lower)
                            || id.to_string().contains(&search_lower)
                        {
                            return true;
                        }
                        if let Some(name) = self.get_multi_name(id) {
                            if name.to_lowercase().contains(&search_lower) {
                                return true;
                            }
                        }
                        false
                    })
                    .collect();

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Total: {} multis", filtered.len()))
                            .color(egui::Color32::GRAY),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.multi_mode_enhanced {
                            ui.label(
                                egui::RichText::new("MultiCollection.uop")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(100, 180, 255)),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("multi.mul")
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(180, 220, 100)),
                            );
                        }
                    });
                });
                ui.separator();

                let total = filtered.len();
                let mut new_multi_selected = None;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, 22.0, total, |ui, row_range| {
                        for idx in row_range {
                            let id = filtered[idx];
                            let name = self
                                .get_multi_name(id)
                                .unwrap_or_else(|| "Estrutura".to_string());
                            let label = format!("0x{:04X} ({}) - {}", id, id, name);
                            let is_sel = self.multi_id == id;
                            let resp = ui.selectable_label(is_sel, label);
                            if resp.clicked() {
                                new_multi_selected = Some(id);
                            }
                            if is_sel && self.keyboard_navigated {
                                resp.scroll_to_me(Some(egui::Align::Center));
                            }
                        }
                    });

                if let Some(id) = new_multi_selected {
                    self.multi_id = id;
                    self.render_current_multi(ctx);
                }
            });

        // Vertical Zoom Tool on Right
        egui::SidePanel::right("multis_zoom_panel")
            .resizable(false)
            .exact_width(55.0)
            .show_inside(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.label(
                        egui::RichText::new("3.0x")
                            .size(10.0)
                            .color(egui::Color32::GRAY),
                    );
                    ui.add_space(4.0);
                    ui.add(
                        egui::Slider::new(&mut self.multi_zoom, 0.2..=3.0)
                            .vertical()
                            .show_value(false),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("0.2x")
                            .size(10.0)
                            .color(egui::Color32::GRAY),
                    );
                    ui.add_space(10.0);
                    ui.label(format!("{:.2}", self.multi_zoom));
                    ui.label(
                        egui::RichText::new("Zoom")
                            .size(11.0)
                            .color(egui::Color32::GRAY),
                    );
                    if ui
                        .button("1x")
                        .on_hover_text("Redefinir Zoom para 100%")
                        .clicked()
                    {
                        self.multi_zoom = 1.0;
                    }
                    if ui
                        .button("⟲")
                        .on_hover_text("Centralizar Posição")
                        .clicked()
                    {
                        self.multi_pan = egui::Vec2::ZERO;
                    }
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let multi_name = self.get_multi_name(self.multi_id).unwrap_or_else(|| "Estrutura Multi".to_string());
            let comp_count = self.current_multi_structure.as_ref().map(|s| s.components.len()).unwrap_or(0);

            ui.horizontal(|ui| {
                ui.heading(format!("Multi 0x{:04X} ({}) - {}", self.multi_id, self.multi_id, multi_name));
                ui.label(egui::RichText::new(format!("({} componentes)", comp_count)).color(egui::Color32::LIGHT_GRAY));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⟲ Centralizar").clicked() {
                        self.multi_pan = egui::Vec2::ZERO;
                    }
                });
            });

            ui.horizontal(|ui| {
                ui.label("Corte Z (Andar):");
                if ui.add(egui::Slider::new(&mut self.multi_max_z, -20..=120).text("Z máx")).changed() {
                    self.render_current_multi(ctx);
                }
                if ui.button("Todos").clicked() {
                    self.multi_max_z = 120;
                    self.render_current_multi(ctx);
                }
                if ui.button("Térreo").clicked() {
                    self.multi_max_z = 20;
                    self.render_current_multi(ctx);
                }
                if ui.button("1º Andar").clicked() {
                    self.multi_max_z = 40;
                    self.render_current_multi(ctx);
                }
                if ui.button("⚡ Recarregar").clicked() {
                    self.render_current_multi(ctx);
                }
            });

            ui.separator();

            egui::CollapsingHeader::new(format!("📋 Lista de Componentes ({})", comp_count))
                .default_open(false)
                .show(ui, |ui| {
                    if let Some(structure) = &self.current_multi_structure {
                        let total_comp = structure.components.len();
                        egui::ScrollArea::vertical()
                            .max_height(130.0)
                            .auto_shrink([false, false])
                            .show_rows(ui, 18.0, total_comp, |ui, row_range| {
                                for idx in row_range {
                                    let c = &structure.components[idx];
                                    ui.label(format!(
                                        "#{:03}: Item 0x{:04X} ({}) | Offset: ({}, {}, {}) | Flags: 0x{:04X}",
                                        idx, c.item_id, c.item_id, c.offset_x, c.offset_y, c.offset_z, c.flags
                                    ));
                                }
                            });
                    }
                });

            ui.add_space(4.0);

            egui::Frame::canvas(ui.style()).show(ui, |ui| {
                let available = ui.available_size();
                let (rect, response) = ui.allocate_exact_size(available, egui::Sense::drag());
                ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 24, 20));

                if response.dragged() {
                    self.multi_pan += response.drag_delta();
                }

                if response.hovered() {
                    let scroll_y = ui.input(|i| i.raw_scroll_delta.y);
                    if scroll_y != 0.0 {
                        let factor = if scroll_y > 0.0 { 1.1 } else { 0.9 };
                        self.multi_zoom = (self.multi_zoom * factor).clamp(0.2, 3.0);
                    }
                }

                if let Some(texture) = &self.multi_texture {
                    let center = rect.center() + self.multi_pan;
                    let size = texture.size_vec2() * self.multi_zoom;
                    let img_rect = egui::Rect::from_center_size(center, size);
                    ui.painter().image(
                        texture.id(),
                        img_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Selecione um Multi para renderização instantânea",
                        egui::FontId::proportional(14.0),
                        egui::Color32::GRAY,
                    );
                }
            });
        });
    }

    // 13. LIGHT TAB
    fn render_light_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::SidePanel::left("light_left_panel")
            .resizable(true)
            .default_width(260.0)
            .min_width(200.0)
            .show_inside(ui, |ui| {
                ui.heading("Light Patterns (light.mul)");
                ui.separator();
                let total = self.valid_lights.len();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, 20.0, total, |ui, row_range| {
                        for idx in row_range {
                            let lid = self.valid_lights[idx];
                            let label = format!("Light {} (0x{:02X})", lid, lid);
                            let is_sel = self.light_id == lid;
                            let resp = ui.selectable_label(is_sel, label);
                            if resp.clicked() {
                                self.light_id = lid;
                                self.load_light_preview(ctx);
                            }
                            if is_sel && self.keyboard_navigated {
                                resp.scroll_to_me(Some(egui::Align::Center));
                            }
                        }
                    });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading(format!(
                "Light Mask #{} - Dimensões: {} x {} px",
                self.light_id, self.light_dim.0, self.light_dim.1
            ));
            ui.separator();

            egui::Frame::canvas(ui.style()).show(ui, |ui| {
                let available = ui.available_size();
                let (rect, _) = ui.allocate_exact_size(available, egui::Sense::hover());
                ui.painter().rect_filled(rect, 4.0, egui::Color32::BLACK);

                if let Some(tex) = &self.light_texture {
                    let center = rect.center();
                    let size = tex.size_vec2(); // 1:1 real size
                    let img_rect = egui::Rect::from_center_size(center, size);
                    ui.painter().image(
                        tex.id(),
                        img_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Nenhum padrão de luz carregado",
                        egui::FontId::proportional(14.0),
                        egui::Color32::GRAY,
                    );
                }
            });
        });
    }

    // 14. FONTS TAB
    fn render_fonts_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Fonts Manager (fonts.mul)");
        ui.horizontal(|ui| {
            ui.label("Fonte:");
            for i in 0..10 {
                if ui
                    .selectable_label(self.font_selected == i, format!("Fonte {}", i))
                    .clicked()
                {
                    self.font_selected = i;
                    self.update_font_preview(ctx);
                }
            }
        });

        ui.horizontal(|ui| {
            ui.label("Texto de Teste:");
            if ui
                .text_edit_singleline(&mut self.font_preview_text)
                .changed()
            {
                self.update_font_preview(ctx);
            }
        });

        ui.separator();
        egui::Frame::canvas(ui.style()).show(ui, |ui| {
            let available = ui.available_size();
            let (rect, _) = ui.allocate_exact_size(available, egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 20, 24));

            if let Some(tex) = &self.font_texture {
                let center = rect.center();
                let size = tex.size_vec2() * 2.0;
                let img_rect = egui::Rect::from_center_size(center, size);
                ui.painter().image(
                    tex.id(),
                    img_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            } else {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Sem preview de texto",
                    egui::FontId::proportional(14.0),
                    egui::Color32::GRAY,
                );
            }
        });
    }

    // 15. SKILLS TAB
    fn render_skills_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Skills Table (skills.mul)");
        if let Some(si) = &self.skills_index {
            let valid: Vec<usize> = (0..100).filter(|&i| si.is_valid(i)).collect();
            ui.label(format!("Habilidades encontradas: {}", valid.len()));
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for id in valid {
                        if let Some(data) = si.get_data(id) {
                            let has_action = data.first().map(|&b| b == 1).unwrap_or(false);
                            let name = if data.len() > 1 {
                                String::from_utf8_lossy(&data[1..])
                                    .trim_matches('\0')
                                    .to_string()
                            } else {
                                String::new()
                            };
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(format!("{:2}:", id)).monospace());
                                ui.label(name);
                                if has_action {
                                    ui.label(
                                        egui::RichText::new("[Ação]")
                                            .color(egui::Color32::from_rgb(102, 187, 106)),
                                    );
                                }
                            });
                        }
                    }
                });
        } else {
            ui.label("skills.mul / skills.idx não encontrado.");
        }
    }

    // 16. SPEECH TAB
    fn render_speech_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Speech Keywords (speech.mul)");
        ui.horizontal(|ui| {
            ui.label("Buscar Palavra-chave:");
            ui.text_edit_singleline(&mut self.speech_search);
            if ui.button("✖").clicked() {
                self.speech_search.clear();
            }
        });

        ui.separator();
        if let Some(sm) = &self.speech_mgr {
            let search_lower = self.speech_search.trim().to_lowercase();
            let filtered: Vec<&SpeechEntry> = sm
                .entries
                .iter()
                .filter(|e| {
                    if search_lower.is_empty() {
                        true
                    } else {
                        e.keyword.to_lowercase().contains(&search_lower)
                            || format!("0x{:04x}", e.entry_id).contains(&search_lower)
                            || e.entry_id.to_string().contains(&search_lower)
                    }
                })
                .collect();

            let row_height = 20.0;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, row_height, filtered.len(), |ui, row_range| {
                    for idx in row_range {
                        let e = filtered[idx];
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("0x{:04X}", e.entry_id))
                                    .monospace()
                                    .color(egui::Color32::from_rgb(159, 168, 218)),
                            );
                            ui.label(&e.keyword);
                        });
                    }
                });
        } else {
            ui.label("speech.mul não encontrado.");
        }
    }

    // 17. ANIMDATA TAB
    fn render_animdata_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::SidePanel::left("animdata_left_panel")
            .resizable(true)
            .default_width(320.0)
            .min_width(260.0)
            .show_inside(ui, |ui| {
                ui.heading("AnimData (animdata.mul)");
                ui.separator();
                if let Some(adm) = &self.animdata_mgr {
                    let mut keys: Vec<u32> = adm.entries.keys().copied().collect();
                    keys.sort();
                    ui.label(format!("Itens animados: {}", keys.len()));

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show_rows(ui, 20.0, keys.len(), |ui, row_range| {
                            for idx in row_range {
                                let item_id = keys[idx];
                                let label = format!("Item 0x{:04X} ({})", item_id, item_id);
                                let is_sel = self.animdata_selected == Some(item_id);
                                let resp = ui.selectable_label(is_sel, label);
                                if resp.clicked() {
                                    self.animdata_selected = Some(item_id);
                                    if let Some(art) = &self.art_index {
                                        if let Some(data) = art.get_data(0x4000 + item_id as usize)
                                        {
                                            if let Some(bmp) = decode_static(&data) {
                                                let img = egui::ColorImage::from_rgba_unmultiplied(
                                                    [bmp.width as usize, bmp.height as usize],
                                                    &bmp.rgba,
                                                );
                                                self.animdata_preview_tex = Some(ctx.load_texture(
                                                    "ad_preview",
                                                    img,
                                                    egui::TextureOptions::NEAREST,
                                                ));
                                            }
                                        }
                                    }
                                }
                                if is_sel && self.keyboard_navigated {
                                    resp.scroll_to_me(Some(egui::Align::Center));
                                }
                            }
                        });
                } else {
                    ui.label("animdata.mul não encontrado.");
                }
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("Propriedades de Animação");
            ui.separator();

            if let Some(item_id) = self.animdata_selected {
                if let Some(adm) = &self.animdata_mgr {
                    if let Some(e) = adm.entries.get(&item_id) {
                        ui.label(format!("Item ID: 0x{:04X} ({})", e.item_id, e.item_id));
                        ui.label(format!("Quantidade de Quadros: {}", e.frame_count));
                        ui.label(format!("Intervalo de Quadros: {}", e.frame_interval));
                        ui.label(format!("Start Delay: {}", e.frame_start));
                        ui.label(format!(
                            "Offsets de Quadros: {:?}",
                            &e.frames[..e.frame_count as usize]
                        ));
                    }
                }

                ui.add_space(12.0);
                egui::Frame::canvas(ui.style()).show(ui, |ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(180.0, 180.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 20, 25));
                    if let Some(tex) = &self.animdata_preview_tex {
                        let center = rect.center();
                        let size = tex.size_vec2(); // 1:1 real size
                        let img_rect = egui::Rect::from_center_size(center, size);
                        ui.painter().image(
                            tex.id(),
                            img_rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    }
                });
            } else {
                ui.label("Selecione um item animado na lista à esquerda.");
            }
        });
    }

    // 18. GAMEDATA TAB (79 Enhanced 3D Tables, Grid View, Monospace Raw & Full Exporter)
    fn render_gamedata_tab(&mut self, ui: &mut egui::Ui) {
        if self.gamedata_archive.entries.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.heading("⚠ GameData.uop não encontrado");
                ui.add_space(8.0);
                ui.label("O arquivo GameData.uop contém as 79 tabelas de configuração do cliente 3D Enhanced (EC).");
                ui.label("Configure o caminho da pasta do Ultima Online Enhanced nas Configurações (F3 ou Menu Arquivo).");
            });
            return;
        }

        // Left Panel: Table list & search
        egui::SidePanel::left("gamedata_left_panel")
            .resizable(true)
            .default_width(320.0)
            .min_width(260.0)
            .show_inside(ui, |ui| {
                ui.heading("Tabelas de Configuração");
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label("🔍 Buscar:");
                    ui.text_edit_singleline(&mut self.gamedata_search);
                    if !self.gamedata_search.is_empty() && ui.button("✖").clicked() {
                        self.gamedata_search.clear();
                    }
                });
                ui.add_space(4.0);

                let search_lower = self.gamedata_search.trim().to_lowercase();
                let matching: Vec<usize> = self
                    .gamedata_archive
                    .entries
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, e)| {
                        if search_lower.is_empty()
                            || e.name.to_lowercase().contains(&search_lower)
                            || format!("{:02}", idx).contains(&search_lower)
                            || format!("0x{:016x}", e.hash).contains(&search_lower)
                        {
                            Some(idx)
                        } else {
                            None
                        }
                    })
                    .collect();

                ui.label(
                    egui::RichText::new(format!("Total: {} tabelas", matching.len()))
                        .color(egui::Color32::GRAY),
                );
                ui.separator();

                egui::ScrollArea::vertical()
                    .max_height(ui.available_height() - 110.0)
                    .show(ui, |ui| {
                        for &idx in &matching {
                            let entry = &self.gamedata_archive.entries[idx];
                            let is_sel = self.gamedata_selected_idx == idx;
                            let label = format!("[{:02}] {}", entry.index, entry.name);
                            let resp = ui.selectable_label(is_sel, label);
                            if resp.clicked() {
                                self.gamedata_selected_idx = idx;
                            }
                            if self.keyboard_navigated && is_sel {
                                resp.scroll_to_me(Some(egui::Align::Center));
                            }
                        }
                    });

                ui.separator();
                ui.group(|ui| {
                    ui.label("Exportação:");
                    ui.horizontal(|ui| {
                        if ui.button("Exportar Atual (CSV)").clicked() {
                            if let Some(entry) = self
                                .gamedata_archive
                                .entries
                                .get(self.gamedata_selected_idx)
                            {
                                let filename = format!("export_{}", entry.name);
                                if std::fs::write(&filename, &entry.raw_data).is_ok() {
                                    self.status_message = format!(
                                        "Tabela '{}' exportada com sucesso para '{}' 💾",
                                        entry.name, filename
                                    );
                                }
                            }
                        }
                        if ui.button("Exportar Todas (79)").clicked() {
                            let dir = std::path::Path::new("exported_gamedata");
                            let _ = std::fs::create_dir_all(dir);
                            let mut count = 0;
                            for e in &self.gamedata_archive.entries {
                                let p = dir.join(&e.name);
                                if std::fs::write(&p, &e.raw_data).is_ok() {
                                    count += 1;
                                }
                            }
                            self.status_message = format!(
                                "Exportadas {} tabelas para a pasta 'exported_gamedata/' 💾",
                                count
                            );
                        }
                    });
                });
            });

        // Central Panel: Table Information, Structured Table Grid & Raw Text View
        egui::CentralPanel::default().show_inside(ui, |ui| {
            let entry =
                if let Some(e) = self.gamedata_archive.entries.get(self.gamedata_selected_idx) {
                    e
                } else {
                    return;
                };

            let raw_text = String::from_utf8_lossy(&entry.raw_data).to_string();
            let (headers, rows) = GameDataArchive::parse_csv_rows(&raw_text);

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.heading(&entry.name);
                    ui.label(
                        egui::RichText::new(format!("(Índice {})", entry.index))
                            .color(egui::Color32::GRAY),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Hash (64-bit):");
                    ui.label(
                        egui::RichText::new(format!("0x{:016X}", entry.hash))
                            .monospace()
                            .color(egui::Color32::from_rgb(159, 168, 218)),
                    );
                    ui.separator();
                    ui.label(format!("Linhas: {} | Colunas: {}", rows.len(), headers.len()));
                    ui.separator();
                    ui.label(format!(
                        "Tamanho: {} KB (comp: {} KB)",
                        entry.decompressed_size / 1024,
                        entry.compressed_size / 1024
                    ));
                });
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(!self.gamedata_view_raw, "📊 Tabela Estruturada")
                    .clicked()
                {
                    self.gamedata_view_raw = false;
                }
                if ui
                    .selectable_label(self.gamedata_view_raw, "📝 Texto Raw (CSV / XML)")
                    .clicked()
                {
                    self.gamedata_view_raw = true;
                }
            });

            ui.separator();

            if self.gamedata_view_raw {
                egui::ScrollArea::both().show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut raw_text.as_str())
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                });
            } else {
                ui.horizontal(|ui| {
                    ui.label("🔍 Filtrar linhas:");
                    ui.text_edit_singleline(&mut self.gamedata_row_search);
                    if !self.gamedata_row_search.is_empty() && ui.button("✖").clicked() {
                        self.gamedata_row_search.clear();
                    }
                });
                ui.add_space(4.0);

                let row_search = self.gamedata_row_search.trim().to_lowercase();
                let filtered_rows: Vec<&Vec<String>> = rows
                    .iter()
                    .filter(|r| {
                        if row_search.is_empty() {
                            true
                        } else {
                            r.iter().any(|cell| cell.to_lowercase().contains(&row_search))
                        }
                    })
                    .collect();

                egui::ScrollArea::both().show(ui, |ui| {
                    egui::Grid::new("gamedata_grid")
                        .striped(true)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            // Header row
                            for h in &headers {
                                ui.label(
                                    egui::RichText::new(h)
                                        .strong()
                                        .color(egui::Color32::from_rgb(255, 215, 64)),
                                );
                            }
                            ui.end_row();

                            // Data rows
                            for row in filtered_rows.iter().take(500) {
                                for cell in row.iter() {
                                    ui.label(cell);
                                }
                                ui.end_row();
                            }
                        });
                    if filtered_rows.len() > 500 {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "... exibindo primeiras 500 de {} linhas (use o filtro de linhas para refinar)",
                                filtered_rows.len()
                            ))
                            .italics()
                            .color(egui::Color32::GRAY),
                        );
                    }
                });
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("UOFiddler - Ultima Online Client Viewer & Editor (100% Rust)"),
        ..Default::default()
    };

    eframe::run_native(
        "UOFiddler",
        native_options,
        Box::new(|cc| {
            setup_custom_theme(&cc.egui_ctx);
            Ok(Box::new(UoFiddlerApp::default()))
        }),
    )
}
