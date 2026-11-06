//! The map tab's isometric mode: the view, a radar inset to move around, the tile under the
//! cursor, and the pending edits of each facet.

use super::assets::{Blocks, MapAssets};
use super::editor::text::{t, tf};
use super::editor::{self, MapEditor, Pointer, Request};
use super::ui::picker::Picker;
use super::view::{Hover, IsoView};
use eframe::egui::{self, pos2, vec2, Color32, Rect, Stroke};
use uo_core::map_edit::{save, SaveOptions};
use uo_core::{
    FileIndex, HuesManager, MapEdits, MapManager, RadarColManager, TileDataManager, MAP_DEFS,
};

/// The client data the map needs, borrowed from the app for one frame.
pub struct MapRes<'a> {
    pub maps: &'a mut MapManager,
    pub art: Option<&'a FileIndex>,
    pub textures: Option<&'a FileIndex>,
    pub tiledata: Option<&'a TileDataManager>,
    pub hues: Option<&'a HuesManager>,
    pub radar: Option<&'a RadarColManager>,
}

const RADAR_TILES: u32 = 256;
const RADAR_PX: f32 = 200.0;

pub struct MapPanel {
    pub view: IsoView,
    /// Pending edits, one set per facet.
    pub edits: [MapEdits; 6],
    /// The full-screen editor, and the facet it was last opened on.
    pub editor: MapEditor,
    editor_map: Option<usize>,
    picker: Picker,
    /// The (facet, x, y) the view was last put over, to follow the app's coordinates.
    synced: Option<(usize, u32, u32)>,
    radar: Option<(egui::TextureHandle, usize, u32, u32, u64)>,
    hover: Option<Hover>,
}

impl Default for MapPanel {
    fn default() -> Self {
        Self {
            view: IsoView::default(),
            edits: Default::default(),
            editor: MapEditor::default(),
            editor_map: None,
            picker: Picker::default(),
            synced: None,
            radar: None,
            hover: None,
        }
    }
}

impl MapPanel {
    /// Draws the isometric view filling `ui`, following and updating `(x, y)`.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        res: &mut MapRes,
        map_id: usize,
        x: &mut u32,
        y: &mut u32,
    ) {
        let assets = MapAssets {
            maps: res.maps,
            map_id,
            edits: &self.edits[map_id],
            art: res.art,
            textures: res.textures,
            tiledata: res.tiledata,
            hues: res.hues,
            cache: Default::default(),
        };
        if !res.maps.has_map(map_id) {
            ui.label(
                "This facet has no map file (mapN.mul or mapNLegacyMUL.uop) in the client folder.",
            );
            return;
        }
        if self.synced != Some((map_id, *x, *y)) {
            let z = Blocks::new(&assets).land_z(*x as i32, *y as i32) as i32;
            self.view.center_on(*x as i32, *y as i32, z);
            self.synced = Some((map_id, *x, *y));
        }

        let status_h = 22.0;
        let full = ui.available_rect_before_wrap();
        let rect = Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - status_h));
        let (_response, t, hover) = self.view.show(ui, rect, &assets);
        self.hover = hover;

        // Follow the camera back into the app's coordinates.
        let (cx, cy) = self.view.center_tile();
        let (w, h) = assets.size();
        let (cx, cy) = (cx.clamp(0, w - 1) as u32, cy.clamp(0, h - 1) as u32);
        if (cx, cy) != (*x, *y) {
            *x = cx;
            *y = cy;
            self.synced = Some((map_id, cx, cy));
        }

        if let Some(hv) = hover {
            let stroke = Stroke::new(1.5_f32, Color32::from_rgb(255, 220, 90));
            let mut b = Blocks::new(&assets);
            let z = [
                b.land_z(hv.x, hv.y) as i32,
                b.land_z(hv.x + 1, hv.y) as i32,
                b.land_z(hv.x + 1, hv.y + 1) as i32,
                b.land_z(hv.x, hv.y + 1) as i32,
            ];
            ui.painter_at(rect).add(egui::Shape::closed_line(
                t.diamond(hv.x, hv.y, z).to_vec(),
                stroke,
            ));
        }

        let text = self.status_text(&assets);
        if let Some(radar) = res.radar {
            self.radar_inset(ui, rect, res.maps, radar, map_id, x, y);
        }
        if ui.input(|i| i.key_pressed(egui::Key::F11)) && !ui.ctx().wants_keyboard_input() {
            self.open_editor(map_id);
        }

        let status = Rect::from_min_max(pos2(full.min.x, rect.max.y), full.max);
        ui.allocate_rect(status, egui::Sense::hover());
        let (left, right) = text;
        let painter = ui.painter();
        painter.text(
            pos2(status.min.x + 8.0, status.center().y),
            egui::Align2::LEFT_CENTER,
            left,
            super::ui::style::body(),
            super::ui::style::color::TEXT,
        );
        painter.text(
            pos2(status.max.x - 8.0, status.center().y),
            egui::Align2::RIGHT_CENTER,
            right,
            super::ui::style::small(),
            super::ui::style::color::TEXT_DIM,
        );
    }

    /// The status bar: the tile under the cursor on the left, the view on the right.
    fn status_text(&self, assets: &MapAssets) -> (String, String) {
        let (cx, cy) = self.view.center_tile();
        let right = format!(
            "{}  ·  centre {}, {}  ·  zoom {:.0}%  ·  {} statics",
            MAP_DEFS[assets.map_id].name,
            cx,
            cy,
            self.view.zoom * 100.0,
            self.view.statics_drawn(),
        );
        let mut left = String::new();
        if let Some(h) = self.hover {
            if let Some((g, z)) = assets.land(h.x, h.y) {
                left = format!(
                    "{}, {}, {}  ·  {} (0x{:04X})",
                    h.x,
                    h.y,
                    z,
                    assets.land_name(g),
                    g
                );
            }
            if let Some((_, _, g, z, _)) = h.static_hit {
                left += &format!("  ·  {} (0x{:04X}) z {}", assets.item_name(g), g, z);
            }
        }
        (left, right)
    }

    /// The radar around the camera in a corner: click or drag on it to move.
    #[allow(clippy::too_many_arguments)]
    fn radar_inset(
        &mut self,
        ui: &mut egui::Ui,
        view: Rect,
        maps: &MapManager,
        radar: &RadarColManager,
        map_id: usize,
        x: &mut u32,
        y: &mut u32,
    ) {
        let half = RADAR_TILES / 2;
        let (w, h) = (maps.actual_widths[map_id], MAP_DEFS[map_id].height);
        // Re-render when the camera gets a quarter of the radar away from where it was rendered.
        let stale = self
            .radar
            .as_ref()
            .is_none_or(|(_, id, rx, ry, generation)| {
                *id != map_id
                    || x.abs_diff(*rx) > RADAR_TILES / 4
                    || y.abs_diff(*ry) > RADAR_TILES / 4
                    || *generation != self.view.generation
            });
        if stale {
            let bx = x.saturating_sub(half) / 8;
            let by = y.saturating_sub(half) / 8;
            if let Some((iw, ih, rgba)) = maps.render_minimap(
                map_id,
                bx,
                by,
                RADAR_TILES / 8,
                RADAR_TILES / 8,
                radar,
                true,
                false,
            ) {
                let img =
                    egui::ColorImage::from_rgba_unmultiplied([iw as usize, ih as usize], &rgba);
                let tex =
                    ui.ctx()
                        .load_texture("map_radar_inset", img, egui::TextureOptions::NEAREST);
                self.radar = Some((tex, map_id, *x, *y, self.view.generation));
            }
        }
        let Some((tex, _, rx, ry, _)) = &self.radar else {
            return;
        };
        let origin = (
            (rx.saturating_sub(half) / 8) * 8,
            (ry.saturating_sub(half) / 8) * 8,
        );
        let rect = Rect::from_min_size(
            pos2(view.max.x - RADAR_PX - 8.0, view.min.y + 8.0),
            vec2(RADAR_PX, RADAR_PX),
        );
        let scale = RADAR_PX / RADAR_TILES as f32;
        let response = ui.interact(
            rect,
            ui.id().with("radar_inset"),
            egui::Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, Color32::BLACK);
        painter.image(
            tex.id(),
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        // The camera, and roughly what the view shows (a diamond in tile space).
        let to_radar = |tx: f32, ty: f32| {
            pos2(
                rect.min.x + (tx - origin.0 as f32) * scale,
                rect.min.y + (ty - origin.1 as f32) * scale,
            )
        };
        let t = super::view::Transform {
            rect: view,
            camera: self.view.camera,
            zoom: self.view.zoom,
        };
        let corners = [
            view.left_top(),
            view.right_top(),
            view.right_bottom(),
            view.left_bottom(),
        ]
        .map(|p| {
            let wp = t.to_world(p);
            let (tx, ty) = ((wp.x + wp.y) / 44.0, (wp.y - wp.x) / 44.0);
            to_radar(tx, ty)
        });
        painter.add(egui::Shape::closed_line(
            corners.to_vec(),
            Stroke::new(1.0_f32, Color32::WHITE),
        ));
        painter.rect_stroke(rect, 2.0, Stroke::new(1.0_f32, Color32::from_gray(90)));
        if response.clicked() || response.dragged() {
            if let Some(p) = response.interact_pointer_pos() {
                let tx = origin.0 as f32 + (p.x - rect.min.x) / scale;
                let ty = origin.1 as f32 + (p.y - rect.min.y) / scale;
                *x = (tx.max(0.0) as u32).min(w - 1);
                *y = (ty.max(0.0) as u32).min(h - 1);
            }
        }
    }
}

impl MapPanel {
    /// The editor is open (the app hands it the whole window).
    pub fn editing(&self) -> bool {
        self.editor.editing
    }

    /// Opens the full-screen editor over the view, on a facet.
    pub fn open_editor(&mut self, map_id: usize) {
        if self.editor_map != Some(map_id) {
            self.editor.reset(&self.edits[map_id]);
            self.editor_map = Some(map_id);
        }
        self.editor.set_editing(true);
    }

    /// The full-screen editor, filling the window: the frame around (`editor::chrome`) and the
    /// map in the middle, with the tools acting on it. `x, y` follow the camera, as in the view.
    pub fn fullscreen(
        &mut self,
        ctx: &egui::Context,
        res: &mut MapRes,
        map_id: usize,
        x: &mut u32,
        y: &mut u32,
    ) {
        if self.editor_map != Some(map_id) {
            self.editor.reset(&self.edits[map_id]);
            self.editor_map = Some(map_id);
        }
        let mut actions = Vec::new();
        let mut requests = Vec::new();
        {
            let assets = MapAssets {
                maps: res.maps,
                map_id,
                edits: &self.edits[map_id],
                art: res.art,
                textures: res.textures,
                tiledata: res.tiledata,
                hues: res.hues,
                cache: Default::default(),
            };
            editor::chrome::chrome(
                ctx,
                &mut self.editor,
                &mut self.picker,
                &assets,
                &mut actions,
                &mut requests,
            );
            egui::CentralPanel::default()
                .frame(egui::Frame::none())
                .show(ctx, |ui| {
                    let rect = ui.max_rect();
                    let over_ui = !ui.rect_contains_pointer(rect) || ctx.is_pointer_over_area();
                    self.editor.camera(ctx, &mut self.view, over_ui);
                    self.view.cut = self.editor.cutaway();
                    self.view.left_drag_pans = false;
                    self.view.show_grid = false;
                    let (response, t, _) = self.view.show(ui, rect, &assets);
                    let hover = response.hover_pos();
                    let (pressed, down, alt, ctrl) = ctx.input(|i| {
                        (
                            i.pointer.primary_pressed(),
                            i.pointer.primary_down(),
                            i.modifiers.alt,
                            i.modifiers.command,
                        )
                    });
                    let tile = hover.map(|p| {
                        let w = t.to_world(p);
                        self.editor
                            .pick(&assets, w.x, w.y, self.editor.hover_z(&assets))
                    });
                    self.editor.cursor_screen = hover;
                    let pointer = Pointer {
                        tile,
                        pressed: pressed && hover.is_some(),
                        down,
                        alt,
                        ctrl,
                    };
                    actions.extend(self.editor.pointer(&assets, pointer));
                    if let Some(overlay) = self.editor.overlay().cloned() {
                        let painter = ui.painter_at(rect);
                        editor::overlay::paint(
                            ctx,
                            &painter,
                            &t,
                            &overlay,
                            &mut self.view,
                            &assets,
                        );
                    }
                });
        }
        if !actions.is_empty() {
            for tile in actions {
                self.edits[map_id].set(tile);
            }
            self.view.generation += 1;
        }
        if self.editor.test_save && self.editor.test_idle() && !self.edits[map_id].is_empty() {
            self.editor.test_save = false;
            requests.push(Request::Save);
        }
        for request in requests {
            self.request(request, res.maps, map_id);
        }
        let (cx, cy) = self.view.center_tile();
        let (w, h) = (
            res.maps.actual_widths[map_id] as i32,
            MAP_DEFS[map_id].height as i32,
        );
        *x = cx.clamp(0, w - 1) as u32;
        *y = cy.clamp(0, h - 1) as u32;
        self.synced = Some((map_id, *x, *y));
    }

    /// What the editor asked for: saving, exporting, importing, discarding, leaving.
    fn request(&mut self, request: Request, maps: &mut MapManager, map_id: usize) {
        match request {
            Request::Exit => self.editor.set_editing(false),
            Request::Save | Request::SaveAs => {
                if self.edits[map_id].is_empty() {
                    self.editor.say(t("map.nothing_to_save"), true);
                    return;
                }
                let target_dir = if request == Request::SaveAs {
                    match rfd::FileDialog::new()
                        .set_title(t("map.save_as"))
                        .pick_folder()
                    {
                        Some(dir) => Some(dir),
                        None => return,
                    }
                } else {
                    None
                };
                let options = SaveOptions {
                    target_dir,
                    backup: true,
                };
                match save(maps, map_id, &self.edits[map_id], &options) {
                    Ok(report) => {
                        let place = maps.paths[map_id]
                            .map
                            .as_ref()
                            .and_then(|p| p.parent())
                            .map(|p| p.display().to_string())
                            .unwrap_or_default();
                        self.edits[map_id].clear();
                        self.editor.reset(&self.edits[map_id]);
                        self.editor.say(
                            tf(
                                "map.saved_files",
                                &[&report.land_blocks, &report.static_blocks, &place],
                            ),
                            false,
                        );
                    }
                    Err(e) => self.editor.say(tf("map.save_failed", &[&e]), true),
                }
                self.view.generation += 1;
            }
            Request::Discard => {
                self.edits[map_id].clear();
                self.editor.reset(&self.edits[map_id]);
                self.view.generation += 1;
                self.editor.say(t("map.discarded"), false);
            }
            Request::Export => {
                let name = format!("map{}_changes.json", map_id);
                if let Some(path) = rfd::FileDialog::new()
                    .set_title(t("map.export"))
                    .set_file_name(&name)
                    .add_filter("JSON", &["json"])
                    .save_file()
                {
                    match std::fs::write(&path, self.edits[map_id].to_json()) {
                        Ok(()) => self.editor.say(path.display().to_string(), false),
                        Err(e) => self.editor.say(e.to_string(), true),
                    }
                }
            }
            Request::Import => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title(t("map.import"))
                    .add_filter("JSON", &["json"])
                    .pick_file()
                {
                    let loaded = std::fs::read_to_string(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|text| MapEdits::from_json(&text));
                    match loaded {
                        Ok(edits) => {
                            for tile in edits.tiles() {
                                self.edits[map_id].set(tile.clone());
                            }
                            self.editor.reset(&self.edits[map_id]);
                            self.view.generation += 1;
                            self.editor.say(tf("map.imported", &[&edits.len()]), false);
                        }
                        Err(e) => self.editor.say(e, true),
                    }
                }
            }
        }
    }
}
