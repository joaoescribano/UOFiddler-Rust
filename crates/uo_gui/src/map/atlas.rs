//! Sprite atlas: decoded art packed into a few big egui textures, so a whole scene draws in one
//! or two meshes instead of one texture per sprite.

use eframe::egui;
use std::collections::HashMap;

pub const PAGE: usize = 2048;
const MAX_PAGES: usize = 6;
/// Transparent gap around each sprite, so linear filtering never bleeds a neighbour in.
const PAD: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpriteKey {
    Land(u16),
    Static { graphic: u16, hue: u16 },
    Texture(u16),
}

#[derive(Debug, Clone, Copy)]
pub struct Sprite {
    pub page: usize,
    /// Top-left in page pixels.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Sprite {
    pub fn uv(&self, u: f32, v: f32) -> egui::Pos2 {
        egui::pos2(
            (self.x + u * self.w) / PAGE as f32,
            (self.y + v * self.h) / PAGE as f32,
        )
    }
}

struct Page {
    texture: egui::TextureHandle,
    shelf_x: usize,
    shelf_y: usize,
    shelf_h: usize,
}

#[derive(Default)]
pub struct Atlas {
    pages: Vec<Page>,
    sprites: HashMap<SpriteKey, Option<Sprite>>,
    /// Set when a sprite did not fit anywhere; the scene clears the atlas and builds again.
    pub full: bool,
}

pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl Atlas {
    pub fn texture(&self, page: usize) -> egui::TextureId {
        self.pages[page].texture.id()
    }

    pub fn len(&self) -> usize {
        self.sprites.len()
    }

    /// Forgets every sprite (the textures are reused).
    pub fn reset(&mut self) {
        self.sprites.clear();
        for p in &mut self.pages {
            p.shelf_x = 0;
            p.shelf_y = 0;
            p.shelf_h = 0;
        }
        self.full = false;
    }

    /// The sprite of a key, decoding it with `load` the first time. `None` when there is no art.
    pub fn get(
        &mut self,
        ctx: &egui::Context,
        key: SpriteKey,
        load: impl FnOnce() -> Option<Image>,
    ) -> Option<Sprite> {
        if let Some(known) = self.sprites.get(&key) {
            return *known;
        }
        let sprite = match load() {
            Some(image) if image.width > 0 && image.height > 0 => {
                let s = self.insert(ctx, &image);
                if s.is_none() {
                    self.full = true;
                    return None;
                }
                s
            }
            _ => None,
        };
        self.sprites.insert(key, sprite);
        sprite
    }

    fn insert(&mut self, ctx: &egui::Context, image: &Image) -> Option<Sprite> {
        let (w, h) = (image.width + PAD * 2, image.height + PAD * 2);
        if w > PAGE || h > PAGE {
            return None;
        }
        let mut chosen = None;
        for (i, p) in self.pages.iter_mut().enumerate() {
            if p.shelf_x + w > PAGE {
                p.shelf_y += p.shelf_h;
                p.shelf_x = 0;
                p.shelf_h = 0;
            }
            if p.shelf_y + h <= PAGE {
                chosen = Some(i);
                break;
            }
        }
        let page = match chosen {
            Some(i) => i,
            None if self.pages.len() < MAX_PAGES => {
                let blank = egui::ColorImage::new([PAGE, PAGE], egui::Color32::TRANSPARENT);
                let name = format!("map_atlas_{}", self.pages.len());
                self.pages.push(Page {
                    texture: ctx.load_texture(name, blank, options()),
                    shelf_x: 0,
                    shelf_y: 0,
                    shelf_h: 0,
                });
                self.pages.len() - 1
            }
            None => return None,
        };
        let p = &mut self.pages[page];
        let (x, y) = (p.shelf_x, p.shelf_y);
        p.shelf_x += w;
        p.shelf_h = p.shelf_h.max(h);
        let mut padded = egui::ColorImage::new([w, h], egui::Color32::TRANSPARENT);
        for row in 0..image.height {
            for col in 0..image.width {
                let s = (row * image.width + col) * 4;
                let px = &image.rgba[s..s + 4];
                padded.pixels[(row + PAD) * w + col + PAD] =
                    egui::Color32::from_rgba_unmultiplied(px[0], px[1], px[2], px[3]);
            }
        }
        p.texture.set_partial([x, y], padded, options());
        Some(Sprite {
            page,
            x: (x + PAD) as f32,
            y: (y + PAD) as f32,
            w: image.width as f32,
            h: image.height as f32,
        })
    }
}

fn options() -> egui::TextureOptions {
    egui::TextureOptions {
        magnification: egui::TextureFilter::Nearest,
        minification: egui::TextureFilter::Linear,
        ..Default::default()
    }
}
