//! Map editing: changes kept per tile over the client map, and writing them back to the client
//! files. A `MapTile` holds what changed on one tile (land graphic, land height, statics placed
//! and statics taken away); a tile with no change is the original one. `save` folds every change
//! into its 8x8 block and writes the facet: land blocks are patched in place (the size of a land
//! block never changes, in `mapN.mul` and in `mapNLegacyMUL.uop` alike), and `staidxN.mul` +
//! `staticsN.mul` are rewritten whole, since a block's statics grow and shrink.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::maps::{MapManager, LAND_BLOCK_BYTES, MAP_DEFS};

pub const BLOCK_SIZE: i32 = 8;
const STATIC_BYTES: usize = 7;
const INDEX_BYTES: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LandCell {
    pub graphic: u16,
    pub z: i8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaticItem {
    pub graphic: u16,
    /// Position inside the block, 0 to 7.
    pub x: u8,
    pub y: u8,
    pub z: i8,
    pub hue: u16,
}

/// One 8x8 block, decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapBlock {
    pub bx: u32,
    pub by: u32,
    /// Cells in row order: `land[y * 8 + x]`.
    pub land: [LandCell; 64],
    pub statics: Vec<StaticItem>,
}

impl MapBlock {
    pub fn cell(&self, x: i32, y: i32) -> LandCell {
        self.land[(y.rem_euclid(BLOCK_SIZE) * BLOCK_SIZE + x.rem_euclid(BLOCK_SIZE)) as usize]
    }

    /// The statics standing on a tile (world coordinates).
    pub fn statics_at(&self, x: i32, y: i32) -> impl Iterator<Item = &StaticItem> {
        let (lx, ly) = (
            x.rem_euclid(BLOCK_SIZE) as u8,
            y.rem_euclid(BLOCK_SIZE) as u8,
        );
        self.statics.iter().filter(move |s| s.x == lx && s.y == ly)
    }
}

/// A static placed by the editor on a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacedStatic {
    pub graphic: u16,
    pub z: i8,
    #[serde(default)]
    pub hue: u16,
}

/// A static of the original map taken away from a tile: its graphic and height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemovedStatic {
    pub graphic: u16,
    pub z: i8,
}

/// What the editor changed on one tile, over the client map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapTile {
    pub x: i32,
    pub y: i32,
    #[serde(default)]
    pub land_z: Option<i8>,
    /// The land graphic replacing the original.
    #[serde(default)]
    pub land: Option<u16>,
    #[serde(default)]
    pub add: Vec<PlacedStatic>,
    #[serde(default)]
    pub remove: Vec<RemovedStatic>,
}

impl MapTile {
    /// A tile with no change.
    pub fn fresh(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            land_z: None,
            land: None,
            add: Vec::new(),
            remove: Vec::new(),
        }
    }

    /// Nothing changed: the tile is the one of the original map.
    pub fn is_empty(&self) -> bool {
        self.land_z.is_none()
            && self.land.is_none()
            && self.add.is_empty()
            && self.remove.is_empty()
    }
}

pub fn decode_land(bytes: &[u8]) -> [LandCell; 64] {
    std::array::from_fn(|i| LandCell {
        graphic: u16::from_le_bytes([bytes[i * 3], bytes[i * 3 + 1]]),
        z: bytes[i * 3 + 2] as i8,
    })
}

pub fn decode_statics(raw: &[u8]) -> Vec<StaticItem> {
    raw.chunks_exact(STATIC_BYTES)
        .map(|r| StaticItem {
            graphic: u16::from_le_bytes([r[0], r[1]]),
            x: r[2] & 7,
            y: r[3] & 7,
            z: r[4] as i8,
            hue: u16::from_le_bytes([r[5], r[6]]),
        })
        .collect()
}

fn encode_statics(statics: &[StaticItem]) -> Vec<u8> {
    let mut out = Vec::with_capacity(statics.len() * STATIC_BYTES);
    for s in statics {
        out.extend_from_slice(&s.graphic.to_le_bytes());
        out.push(s.x);
        out.push(s.y);
        out.push(s.z as u8);
        out.extend_from_slice(&s.hue.to_le_bytes());
    }
    out
}

/// A block as the client files hold it (no mapdif / stadif patches: the editor works on the
/// base map, which is what modern clients read).
pub fn read_block(maps: &MapManager, map_id: usize, bx: u32, by: u32) -> Option<MapBlock> {
    let land = maps.land_block(map_id, bx, by, false)?;
    let statics = maps
        .static_block(map_id, bx, by, false)
        .map(decode_statics)
        .unwrap_or_default();
    Some(MapBlock {
        bx,
        by,
        land: decode_land(land),
        statics,
    })
}

/// The pending changes of one facet, tile by tile.
#[derive(Debug, Clone, Default)]
pub struct MapEdits {
    tiles: HashMap<(i32, i32), MapTile>,
}

impl MapEdits {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores the change of a tile; an empty change brings the tile back to the original.
    pub fn set(&mut self, tile: MapTile) {
        if tile.is_empty() {
            self.tiles.remove(&(tile.x, tile.y));
        } else {
            self.tiles.insert((tile.x, tile.y), tile);
        }
    }

    pub fn get(&self, x: i32, y: i32) -> Option<&MapTile> {
        self.tiles.get(&(x, y))
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    pub fn clear(&mut self) {
        self.tiles.clear();
    }

    pub fn tiles(&self) -> impl Iterator<Item = &MapTile> {
        self.tiles.values()
    }

    /// The blocks touched by a change.
    pub fn blocks(&self) -> BTreeSet<(u32, u32)> {
        self.tiles
            .keys()
            .filter(|(x, y)| *x >= 0 && *y >= 0)
            .map(|(x, y)| ((x / BLOCK_SIZE) as u32, (y / BLOCK_SIZE) as u32))
            .collect()
    }

    /// Folds the changes of the block's tiles into it.
    pub fn apply(&self, block: &mut MapBlock) {
        if self.tiles.is_empty() {
            return;
        }
        let (x0, y0) = (block.bx as i32 * BLOCK_SIZE, block.by as i32 * BLOCK_SIZE);
        for cy in 0..BLOCK_SIZE {
            for cx in 0..BLOCK_SIZE {
                let Some(tile) = self.tiles.get(&(x0 + cx, y0 + cy)) else {
                    continue;
                };
                let (lx, ly) = (cx as u8, cy as u8);
                let cell = &mut block.land[(cy * BLOCK_SIZE + cx) as usize];
                if let Some(z) = tile.land_z {
                    cell.z = z;
                }
                if let Some(graphic) = tile.land {
                    cell.graphic = graphic;
                }
                for gone in &tile.remove {
                    if let Some(at) = block.statics.iter().position(|s| {
                        s.x == lx && s.y == ly && s.graphic == gone.graphic && s.z == gone.z
                    }) {
                        block.statics.remove(at);
                    }
                }
                block.statics.extend(tile.add.iter().map(|s| StaticItem {
                    graphic: s.graphic,
                    x: lx,
                    y: ly,
                    z: s.z,
                    hue: s.hue,
                }));
            }
        }
    }

    /// A block of the facet with the changes folded in.
    pub fn block(&self, maps: &MapManager, map_id: usize, bx: u32, by: u32) -> Option<MapBlock> {
        let mut block = read_block(maps, map_id, bx, by)?;
        self.apply(&mut block);
        Some(block)
    }

    /// The changes as JSON, to keep unsaved work between sessions.
    pub fn to_json(&self) -> String {
        let mut tiles: Vec<&MapTile> = self.tiles.values().collect();
        tiles.sort_by_key(|t| (t.x, t.y));
        serde_json::to_string_pretty(&tiles).unwrap_or_default()
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let tiles: Vec<MapTile> = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let mut edits = Self::new();
        for tile in tiles {
            edits.set(tile);
        }
        Ok(edits)
    }
}

/// Where `save` writes.
#[derive(Debug, Clone, Default)]
pub struct SaveOptions {
    /// `None` writes over the loaded files; `Some(dir)` writes a full copy of the facet's files
    /// into `dir` and leaves the loaded ones untouched ("Save as").
    pub target_dir: Option<PathBuf>,
    /// Before overwriting a file in place, copy it to `<file>.bak` (once: an existing backup is
    /// kept, so it always holds the files as they were before the first save).
    pub backup: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SaveReport {
    pub land_blocks: usize,
    pub static_blocks: usize,
    pub written: Vec<PathBuf>,
    pub backups: Vec<PathBuf>,
}

fn io(path: &Path, e: std::io::Error) -> String {
    format!("{}: {}", path.display(), e)
}

/// Copies `path` to `<path>.bak` unless a backup is already there.
fn backup(path: &Path, report: &mut SaveReport) -> Result<(), String> {
    let mut name = path.as_os_str().to_owned();
    name.push(".bak");
    let bak = PathBuf::from(name);
    if !bak.exists() {
        fs::copy(path, &bak).map_err(|e| io(path, e))?;
        report.backups.push(bak);
    }
    Ok(())
}

/// Writes a file through a temporary sibling and a rename, so a failure never leaves it half
/// written.
fn replace_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes).map_err(|e| io(&tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| io(path, e))
}

/// The new statics + index files of a facet, with the given blocks replaced.
fn rebuild_statics(
    maps: &MapManager,
    map_id: usize,
    changed: &BTreeMap<usize, Vec<u8>>,
    index_len: usize,
) -> (Vec<u8>, Vec<u8>) {
    let max_by = MAP_DEFS[map_id].height / 8;
    let blocks = (index_len / INDEX_BYTES).max(changed.keys().next_back().map_or(0, |n| n + 1));
    let mut statics = Vec::new();
    let mut index = Vec::with_capacity(blocks * INDEX_BYTES);
    for number in 0..blocks {
        let (bx, by) = ((number as u32) / max_by, (number as u32) % max_by);
        let raw: &[u8] = match changed.get(&number) {
            Some(new) => new,
            None => maps.static_block(map_id, bx, by, false).unwrap_or(&[]),
        };
        if raw.is_empty() {
            index.extend_from_slice(&(-1i32).to_le_bytes());
            index.extend_from_slice(&(-1i32).to_le_bytes());
            index.extend_from_slice(&(-1i32).to_le_bytes());
        } else {
            index.extend_from_slice(&(statics.len() as i32).to_le_bytes());
            index.extend_from_slice(&(raw.len() as i32).to_le_bytes());
            index.extend_from_slice(&0i32.to_le_bytes());
            statics.extend_from_slice(raw);
        }
    }
    (statics, index)
}

/// Writes the pending changes of a facet into its client files. On success the facet is mapped
/// again from what was written (the new files, for "Save as"), and the caller can drop the
/// changes. On failure the facet is mapped again from wherever it was.
pub fn save(
    maps: &mut MapManager,
    map_id: usize,
    edits: &MapEdits,
    options: &SaveOptions,
) -> Result<SaveReport, String> {
    let paths = maps.paths.get(map_id).cloned().ok_or("no such facet")?;
    let map_path = paths.map.clone().ok_or("the facet has no map file")?;
    let land = maps.land_file(map_id).ok_or("the map file is not loaded")?;
    let max_by = MAP_DEFS[map_id].height / 8;

    // What changes, worked out while the files are still mapped.
    let mut land_patches: Vec<(usize, Vec<u8>)> = Vec::new();
    let mut static_blocks: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
    for (bx, by) in edits.blocks() {
        let Some(base) = read_block(maps, map_id, bx, by) else {
            continue;
        };
        let mut block = base.clone();
        edits.apply(&mut block);
        let number = (bx * max_by + by) as usize;
        if block.land != base.land {
            let offset = land
                .block_offset(number)
                .ok_or("block outside the map file")?;
            let mut bytes = Vec::with_capacity(LAND_BLOCK_BYTES - 4);
            for cell in &block.land {
                bytes.extend_from_slice(&cell.graphic.to_le_bytes());
                bytes.push(cell.z as u8);
            }
            land_patches.push((offset + 4, bytes));
        }
        if block.statics != base.statics {
            static_blocks.insert(number, encode_statics(&block.statics));
        }
    }
    let mut report = SaveReport {
        land_blocks: land_patches.len(),
        static_blocks: static_blocks.len(),
        ..SaveReport::default()
    };
    let statics_out = if static_blocks.is_empty() {
        None
    } else {
        let (staidx, stat) = (
            paths.staidx.clone().ok_or("the facet has no staidx file")?,
            paths
                .statics
                .clone()
                .ok_or("the facet has no statics file")?,
        );
        let index_len = fs::metadata(&staidx).map_err(|e| io(&staidx, e))?.len() as usize;
        let (data, index) = rebuild_statics(maps, map_id, &static_blocks, index_len);
        Some((staidx, stat, data, index))
    };

    let target = |path: &Path| match &options.target_dir {
        Some(dir) => dir.join(path.file_name().unwrap_or_default()),
        None => path.to_path_buf(),
    };

    // Unmapped from here on: Windows refuses to replace mapped files.
    maps.unload(map_id);
    let result = (|| -> Result<(), String> {
        if let Some(dir) = &options.target_dir {
            fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
            // A "Save as" is a whole facet: the copies of the loaded files, then the changes.
            for path in [
                Some(&map_path),
                paths.staidx.as_ref(),
                paths.statics.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                let to = target(path);
                if to != *path {
                    fs::copy(path, &to).map_err(|e| io(path, e))?;
                }
            }
        } else if options.backup {
            if !land_patches.is_empty() {
                backup(&map_path, &mut report)?;
            }
            if let Some((staidx, stat, _, _)) = &statics_out {
                backup(staidx, &mut report)?;
                backup(stat, &mut report)?;
            }
        }
        let map_to = target(&map_path);
        if !land_patches.is_empty() {
            let mut file = OpenOptions::new()
                .write(true)
                .open(&map_to)
                .map_err(|e| io(&map_to, e))?;
            for (offset, bytes) in &land_patches {
                file.seek(SeekFrom::Start(*offset as u64))
                    .and_then(|_| file.write_all(bytes))
                    .map_err(|e| io(&map_to, e))?;
            }
            file.sync_all().map_err(|e| io(&map_to, e))?;
            report.written.push(map_to.clone());
        }
        if let Some((staidx, stat, data, index)) = &statics_out {
            let (staidx, stat) = (target(staidx), target(stat));
            replace_file(&stat, data)?;
            replace_file(&staidx, index)?;
            report.written.push(stat);
            report.written.push(staidx);
        }
        Ok(())
    })();
    if result.is_ok() && options.target_dir.is_some() {
        let p = &mut maps.paths[map_id];
        p.map = Some(target(&map_path));
        p.staidx = paths.staidx.as_deref().map(target);
        p.statics = paths.statics.as_deref().map(target);
    }
    maps.reload(map_id);
    result.map(|_| report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uop_convert::write_uop;

    /// Map 4 (Tokuno) is the smallest facet: 181 x 181 blocks.
    const MAP: usize = 4;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("uo_map_edit_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn blocks() -> usize {
        (MAP_DEFS[MAP].width / 8 * (MAP_DEFS[MAP].height / 8)) as usize
    }

    /// A facet where each land cell is (block number % 1000, z 5) and block 0 has one static.
    fn land_bytes() -> Vec<u8> {
        let mut out = Vec::with_capacity(blocks() * LAND_BLOCK_BYTES);
        for n in 0..blocks() {
            out.extend_from_slice(&0u32.to_le_bytes());
            for _ in 0..64 {
                out.extend_from_slice(&((n % 1000) as u16).to_le_bytes());
                out.push(5);
            }
        }
        out
    }

    fn write_statics(dir: &Path) {
        let mut index = Vec::new();
        for n in 0..blocks() {
            let (lookup, len) = if n == 0 { (0i32, 7i32) } else { (-1, -1) };
            index.extend_from_slice(&lookup.to_le_bytes());
            index.extend_from_slice(&len.to_le_bytes());
            index.extend_from_slice(&0i32.to_le_bytes());
        }
        fs::write(dir.join("staidx4.mul"), index).unwrap();
        let s = StaticItem {
            graphic: 0x0EED,
            x: 1,
            y: 2,
            z: 7,
            hue: 0,
        };
        fs::write(dir.join("statics4.mul"), encode_statics(&[s])).unwrap();
    }

    fn load(dir: &Path, map_name: &str) -> MapManager {
        let mut maps = MapManager::new();
        maps.load_map(
            MAP,
            Some(dir.join(map_name)),
            Some(dir.join("staidx4.mul")),
            Some(dir.join("statics4.mul")),
        );
        maps
    }

    fn uop_facet(dir: &Path) {
        let land = land_bytes();
        let chunk = crate::maps::UOP_BLOCKS_PER_CHUNK * LAND_BLOCK_BYTES;
        let entries: Vec<(String, Vec<u8>)> = land
            .chunks(chunk)
            .enumerate()
            .map(|(i, c)| (format!("build/map4legacymul/{:08}.dat", i), c.to_vec()))
            .collect();
        write_uop(&dir.join("map4LegacyMUL.uop"), &entries).unwrap();
        write_statics(dir);
    }

    fn some_edits() -> MapEdits {
        let mut edits = MapEdits::new();
        // Block (100, 50): land and height.
        let mut t = MapTile::fresh(803, 404);
        t.land = Some(0x0003);
        t.land_z = Some(-20);
        edits.set(t);
        // Block 0: take the original static away and place two.
        let mut t = MapTile::fresh(1, 2);
        t.remove.push(RemovedStatic {
            graphic: 0x0EED,
            z: 7,
        });
        t.add.push(PlacedStatic {
            graphic: 0x0F00,
            z: 9,
            hue: 33,
        });
        edits.set(t);
        let mut t = MapTile::fresh(1440, 1440);
        t.add.push(PlacedStatic {
            graphic: 0x1234,
            z: -5,
            hue: 0,
        });
        edits.set(t);
        edits
    }

    fn check_saved(maps: &MapManager) {
        let b = read_block(maps, MAP, 100, 50).unwrap();
        assert_eq!(b.cell(803, 404), LandCell { graphic: 3, z: -20 });
        assert_eq!(
            b.cell(802, 404),
            LandCell {
                graphic: ((100 * 181 + 50) % 1000) as u16,
                z: 5
            }
        );
        let b = read_block(maps, MAP, 0, 0).unwrap();
        assert_eq!(
            b.statics,
            vec![StaticItem {
                graphic: 0x0F00,
                x: 1,
                y: 2,
                z: 9,
                hue: 33
            }]
        );
        let b = read_block(maps, MAP, 180, 180).unwrap();
        assert_eq!(b.statics.len(), 1);
        assert_eq!((b.statics[0].x, b.statics[0].y, b.statics[0].z), (0, 0, -5));
        assert!(read_block(maps, MAP, 5, 5).unwrap().statics.is_empty());
    }

    #[test]
    fn saves_a_uop_facet_in_place_with_a_backup() {
        let dir = scratch("uop");
        uop_facet(&dir);
        let mut maps = load(&dir, "map4LegacyMUL.uop");
        assert!(maps.land_file(MAP).unwrap().is_uop());
        let edits = some_edits();
        let report = save(
            &mut maps,
            MAP,
            &edits,
            &SaveOptions {
                target_dir: None,
                backup: true,
            },
        )
        .unwrap();
        assert_eq!((report.land_blocks, report.static_blocks), (1, 2));
        assert_eq!(report.backups.len(), 3);
        check_saved(&maps);
        // The file was patched, not rebuilt: same size as its backup.
        let size = |p: &str| fs::metadata(dir.join(p)).unwrap().len();
        assert_eq!(size("map4LegacyMUL.uop"), size("map4LegacyMUL.uop.bak"));
        // A fresh load from disk sees the same.
        check_saved(&load(&dir, "map4LegacyMUL.uop"));
        // A second save keeps the first backup.
        let report = save(
            &mut maps,
            MAP,
            &some_edits(),
            &SaveOptions {
                target_dir: None,
                backup: true,
            },
        )
        .unwrap();
        assert!(report.backups.is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_as_leaves_the_loaded_files_alone() {
        let dir = scratch("mul");
        fs::write(dir.join("map4.mul"), land_bytes()).unwrap();
        write_statics(&dir);
        let original = fs::read(dir.join("map4.mul")).unwrap();
        let mut maps = load(&dir, "map4.mul");
        let out = dir.join("out");
        save(
            &mut maps,
            MAP,
            &some_edits(),
            &SaveOptions {
                target_dir: Some(out.clone()),
                backup: true,
            },
        )
        .unwrap();
        assert_eq!(fs::read(dir.join("map4.mul")).unwrap(), original);
        assert!(!dir.join("map4.mul.bak").exists());
        assert_eq!(
            maps.paths[MAP].map.as_deref(),
            Some(out.join("map4.mul").as_path())
        );
        check_saved(&maps);
        check_saved(&load(&out, "map4.mul"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn edits_round_trip_through_json() {
        let edits = some_edits();
        let back = MapEdits::from_json(&edits.to_json()).unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!(back.get(803, 404), edits.get(803, 404));
        assert_eq!(back.blocks(), edits.blocks());
    }
}
