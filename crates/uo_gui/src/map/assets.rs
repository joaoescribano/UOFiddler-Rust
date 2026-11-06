//! What the map view and the editor read: the facet (with the pending edits folded in), the
//! tiledata and the art.

use super::atlas::Image;
use std::collections::HashMap;
use uo_core::map_edit::{read_block, StaticItem};
use uo_core::{
    apply_hue_bytes, decode_land, decode_static, decode_texture, FileIndex, HuesManager,
    ItemTileData, MapBlock, MapEdits, MapManager, TileDataManager, MAP_DEFS,
};

use super::iso::flags;

pub struct MapAssets<'a> {
    pub maps: &'a MapManager,
    pub map_id: usize,
    pub edits: &'a MapEdits,
    pub art: Option<&'a FileIndex>,
    pub textures: Option<&'a FileIndex>,
    pub tiledata: Option<&'a TileDataManager>,
    pub hues: Option<&'a HuesManager>,
}

impl<'a> MapAssets<'a> {
    /// Facet size in tiles.
    pub fn size(&self) -> (i32, i32) {
        (
            self.maps.actual_widths[self.map_id] as i32,
            MAP_DEFS[self.map_id].height as i32,
        )
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        let (w, h) = self.size();
        x >= 0 && y >= 0 && x < w && y < h
    }

    /// A block with the pending edits folded in.
    pub fn block(&self, bx: i32, by: i32) -> Option<MapBlock> {
        if bx < 0 || by < 0 {
            return None;
        }
        let mut block = read_block(self.maps, self.map_id, bx as u32, by as u32)?;
        self.edits.apply(&mut block);
        Some(block)
    }

    /// A block as the client files hold it, without the edits.
    pub fn original_block(&self, bx: i32, by: i32) -> Option<MapBlock> {
        if bx < 0 || by < 0 {
            return None;
        }
        read_block(self.maps, self.map_id, bx as u32, by as u32)
    }

    pub fn item(&self, graphic: u16) -> Option<&'a ItemTileData> {
        self.tiledata?.item_tiles.get(graphic as usize)
    }

    pub fn item_flags(&self, graphic: u16) -> u64 {
        self.item(graphic).map_or(0, |t| t.flags)
    }

    pub fn item_height(&self, graphic: u16) -> u8 {
        self.item(graphic).map_or(0, |t| t.height)
    }

    pub fn item_name(&self, graphic: u16) -> &'a str {
        self.item(graphic).map_or("", |t| t.name.as_str())
    }

    pub fn land_flags(&self, graphic: u16) -> u64 {
        self.tiledata
            .and_then(|t| t.land_tiles.get(graphic as usize))
            .map_or(0, |t| t.flags)
    }

    pub fn land_texture(&self, graphic: u16) -> u16 {
        self.tiledata
            .and_then(|t| t.land_tiles.get(graphic as usize))
            .map_or(0, |t| t.texture_id)
    }

    pub fn land_name(&self, graphic: u16) -> &'a str {
        self.tiledata
            .and_then(|t| t.land_tiles.get(graphic as usize))
            .map_or("", |t| t.name.as_str())
    }

    pub fn land_image(&self, graphic: u16) -> Option<Image> {
        let bmp = decode_land(&self.art?.get_data(graphic as usize)?)?;
        Some(Image {
            width: bmp.width as usize,
            height: bmp.height as usize,
            rgba: bmp.rgba,
        })
    }

    pub fn static_image(&self, graphic: u16, hue: u16) -> Option<Image> {
        let mut bmp = decode_static(&self.art?.get_data(0x4000 + graphic as usize)?)?;
        if hue != 0 {
            if let Some(h) = self.hues.and_then(|h| h.hues.get(hue as usize - 1)) {
                let partial = self.item_flags(graphic) & flags::PARTIAL_HUE != 0;
                apply_hue_bytes(&mut bmp.rgba, &h.get_rgba_palette(), partial);
            }
        }
        Some(Image {
            width: bmp.width as usize,
            height: bmp.height as usize,
            rgba: bmp.rgba,
        })
    }

    pub fn texture_image(&self, texture: u16) -> Option<Image> {
        let index = self.textures?;
        let extra = index.entries.get(texture as usize)?.extra;
        let tex = decode_texture(&index.get_data(texture as usize)?, extra)?;
        Some(Image {
            width: tex.size as usize,
            height: tex.size as usize,
            rgba: tex.rgba,
        })
    }
}

/// Blocks read once and kept for a pass over a region (scene build, brush, picking).
pub struct Blocks<'b, 'a> {
    pub assets: &'b MapAssets<'a>,
    cache: HashMap<(i32, i32), Option<MapBlock>>,
}

impl<'b, 'a> Blocks<'b, 'a> {
    pub fn new(assets: &'b MapAssets<'a>) -> Self {
        Self {
            assets,
            cache: HashMap::new(),
        }
    }

    pub fn block(&mut self, bx: i32, by: i32) -> Option<&MapBlock> {
        let assets = self.assets;
        self.cache
            .entry((bx, by))
            .or_insert_with(|| assets.block(bx, by))
            .as_ref()
    }

    /// Land graphic and height of a tile (edits included).
    pub fn land(&mut self, x: i32, y: i32) -> Option<(u16, i8)> {
        let b = self.block(x.div_euclid(8), y.div_euclid(8))?;
        let c = b.cell(x, y);
        Some((c.graphic, c.z))
    }

    pub fn land_z(&mut self, x: i32, y: i32) -> i8 {
        self.land(x, y).map_or(0, |l| l.1)
    }

    /// The statics of a tile (edits included).
    pub fn statics(&mut self, x: i32, y: i32) -> Vec<StaticItem> {
        match self.block(x.div_euclid(8), y.div_euclid(8)) {
            Some(b) => b.statics_at(x, y).copied().collect(),
            None => Vec::new(),
        }
    }
}
