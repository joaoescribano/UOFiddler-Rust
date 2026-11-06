//! The border sets dialog, opened from the editor bar ("Borders…"): choose a set or start a new
//! one, give it a name and a kind (land, cave "with the black", or floor statics), and fill each
//! corner shape with the tiles that fit, one by one with the picker (the "+") or the brush. Each
//! shape has its diamond with the core corners marked. A draft read from the brush's art fills
//! everything at once, to be corrected by hand. Sets are saved in `map_borders.json`
//! (`storage`), next to the app's settings.

use super::art::thumbnail;
use super::borders::{group_label, Source, GROUPS};
use super::state::MapEditor;
use super::storage;
use super::terrain::{ALL, BOTTOM, LEFT, RIGHT, TOP};
use super::text::{t, tf};
use super::types::{valid_name, BorderKind, BorderSet, MAX_BORDER_VARIANTS};
use crate::map::assets::MapAssets;
use crate::map::ui::{picker, style, widgets};
use eframe::egui::{self, Color32, Margin, RichText, Sense, Stroke};
use egui_phosphor::regular as icon;

/// The picker's callers: a tile for a shape, and the outside land of the draft.
pub(super) const PICK_SLOT: &str = "map-border-slot";
pub(super) const PICK_OUTSIDE: &str = "map-border-outside";

/// The dialog's state.
pub struct Dialog {
    pub open: bool,
    /// The set being edited, and the name it is saved under (`None`: new).
    pub editing: BorderSet,
    pub saved_as: Option<String>,
    /// The last message, and whether it is an error.
    pub message: Option<(String, bool)>,
    /// The shape the picker is filling.
    slot: Option<u8>,
    /// The outside land, for the draft of a land set.
    outside: u16,
    /// `.../draft`: reads the draft as soon as the art is there.
    pub test_draft: bool,
}

impl Default for Dialog {
    fn default() -> Self {
        Self {
            open: false,
            editing: BorderSet::default(),
            saved_as: None,
            message: None,
            slot: None,
            outside: 0x0003,
            test_draft: false,
        }
    }
}

impl Dialog {
    /// Puts a tile in a shape (without repeating, up to the limit).
    pub fn add(&mut self, mask: u8, graphic: u16) {
        let tiles = self.editing.masks.entry(mask).or_default();
        if tiles.contains(&graphic) {
            return;
        }
        if tiles.len() >= MAX_BORDER_VARIANTS {
            self.message = Some((tf("borders.too_many", &[&MAX_BORDER_VARIANTS]), true));
            return;
        }
        tiles.push(graphic);
    }

    /// Takes a tile out of a shape.
    pub fn remove(&mut self, mask: u8, graphic: u16) {
        if let Some(tiles) = self.editing.masks.get_mut(&mask) {
            tiles.retain(|g| *g != graphic);
            if tiles.is_empty() {
                self.editing.masks.remove(&mask);
            }
        }
    }

    /// Starts editing a saved set, or a new one (of the same kind).
    pub fn edit(&mut self, set: Option<&BorderSet>) {
        match set {
            Some(set) => {
                self.editing = set.clone();
                self.saved_as = Some(set.name.clone());
            }
            None => {
                self.editing = BorderSet {
                    kind: self.editing.kind,
                    ..Default::default()
                };
                self.saved_as = None;
            }
        }
        self.message = None;
    }

    /// The set to save, and the old name to drop when it was renamed. Without a valid name or
    /// without the core, only the message.
    pub fn save(&mut self) -> Option<(BorderSet, Option<String>)> {
        let mut set = self.editing.clone();
        set.name = set.name.trim().to_string();
        set.masks.retain(|_, tiles| !tiles.is_empty());
        if !valid_name(&set.name) {
            self.message = Some((t("borders.need_name").into(), true));
            return None;
        }
        if set.tiles(ALL).is_empty() {
            self.message = Some((t("borders.need_center").into(), true));
            return None;
        }
        let old = self.saved_as.clone().filter(|old| *old != set.name);
        Some((set, old))
    }
}

impl MapEditor {
    /// Saves a set: it enters the list (replacing a renamed one), and the dialog and the bar
    /// follow the new name.
    pub(super) fn border_saved(&mut self, set: BorderSet, old: Option<String>) {
        if let Some(old) = &old {
            self.border_sets.retain(|s| s.name != *old);
            if self.border_source == Source::Set(old.clone()) {
                self.border_source = Source::Set(set.name.clone());
            }
        }
        let dialog = &mut self.border_dialog;
        dialog.saved_as = Some(set.name.clone());
        dialog.editing = set.clone();
        self.border_sets.retain(|s| s.name != set.name);
        self.border_sets.push(set);
        self.border_sets.sort_by(|a, b| a.name.cmp(&b.name));
        self.border_dialog.message = Some(match storage::save_border_sets(&self.border_sets) {
            Ok(()) => (t("borders.saved").into(), false),
            Err(e) => (e, true),
        });
    }

    /// Deletes a set.
    pub(super) fn border_deleted(&mut self, name: &str) {
        self.border_sets.retain(|s| s.name != name);
        if self.border_source == Source::Set(name.to_string()) {
            self.border_source = Source::Detect;
        }
        if self.border_dialog.saved_as.as_deref() == Some(name) {
            self.border_dialog.edit(None);
            self.border_dialog.message = Some((t("borders.deleted").into(), false));
        }
        if let Err(e) = storage::save_border_sets(&self.border_sets) {
            self.border_dialog.message = Some((e, true));
        }
    }

    /// What the picker chose for the dialog: a tile for the shape that asked, or the outside
    /// land of the draft.
    pub(super) fn border_picks(&mut self, picker: &mut picker::Picker) {
        if let Some(id) = picker.take_result(PICK_SLOT) {
            if let Some(mask) = self.border_dialog.slot.take() {
                self.border_dialog.add(mask, id as u16);
            }
        }
        if let Some(id) = picker.take_result(PICK_OUTSIDE) {
            self.border_dialog.outside = id as u16;
        }
    }

    /// The draft read from the art: the brush's tile inside, and outside the black (cave) or the
    /// chosen land. Returns whether it found one.
    fn border_draft(&mut self, assets: &MapAssets) -> bool {
        let floor = self.border_dialog.editing.kind == BorderKind::Floor;
        let found = match self.border_dialog.editing.kind {
            BorderKind::Cave => self.terrain.void_transitions(assets, self.land),
            BorderKind::Land => {
                self.terrain
                    .transitions(assets, self.land, self.border_dialog.outside)
            }
            BorderKind::Floor => self.terrain.floor_transitions(assets, self.graphic),
        };
        let Some(found) = found else {
            self.border_dialog.message = Some((t("borders.no_draft").into(), true));
            return false;
        };
        if found.masks.is_empty() {
            self.border_dialog.message = Some((t("borders.no_draft").into(), true));
            return false;
        }
        let mut masks = super::borders::from_transitions(&found);
        let brush = if floor { self.graphic } else { self.land };
        masks.entry(ALL).or_insert_with(|| vec![brush]);
        self.border_dialog.editing.masks = masks;
        self.border_dialog.message = Some((t("borders.drafted").into(), false));
        true
    }
}

/// A shape's diamond: the core corners filled in gold; the outside ones empty, or black in a
/// cave set.
fn diagram(ui: &mut egui::Ui, mask: u8, kind: BorderKind) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(48.0, 28.0), Sense::hover());
    let painter = ui.painter_at(rect.expand(4.0));
    let at = |x: f32, y: f32| rect.min + egui::vec2(x, y);
    let corners = [
        (TOP, at(24.0, 3.0)),
        (RIGHT, at(45.0, 14.0)),
        (BOTTOM, at(24.0, 25.0)),
        (LEFT, at(3.0, 14.0)),
    ];
    let fill = if mask == ALL {
        style::color::ACCENT_BG
    } else {
        style::color::SURFACE_2
    };
    painter.add(egui::Shape::convex_polygon(
        corners.iter().map(|(_, p)| *p).collect(),
        fill,
        Stroke::new(1.0_f32, style::color::BORDER),
    ));
    for (bit, p) in corners {
        if mask & bit != 0 {
            painter.circle_filled(p, 3.6, style::color::ACCENT);
        } else if kind == BorderKind::Cave {
            painter.circle(
                p,
                3.6,
                Color32::BLACK,
                Stroke::new(1.0_f32, style::color::TEXT_DIM),
            );
        } else {
            painter.circle_stroke(p, 3.2, Stroke::new(1.2_f32, style::color::TEXT_DIM));
        }
    }
}

/// The width of a shape in the dialog's grid.
const SLOT_WIDTH: f32 = 168.0;

/// A shape: the diamond, its tiles (a click takes one out), the "+" that opens the picker and
/// the button that puts the brush's tile.
fn slot(
    ui: &mut egui::Ui,
    state: &mut MapEditor,
    picker: &mut picker::Picker,
    assets: &MapAssets,
    mask: u8,
) {
    let ctx = ui.ctx().clone();
    let kind = state.border_dialog.editing.kind;
    let floor = kind == BorderKind::Floor;
    let tiles = state.border_dialog.editing.tiles(mask).to_vec();
    widgets::card_frame()
        .inner_margin(Margin::same(6.0))
        .show(ui, |ui| {
            ui.set_width(SLOT_WIDTH);
            // The group's row is horizontal: the shape stacks inside.
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    diagram(ui, mask, kind);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::icon_button(ui, icon::PLUS, t("borders.add")).clicked() {
                            state.border_dialog.slot = Some(mask);
                            let (kind, brush) = if floor {
                                (picker::Kind::Static, state.graphic)
                            } else {
                                (picker::Kind::Land, state.land)
                            };
                            let current = tiles.last().copied().unwrap_or(brush);
                            picker.open(PICK_SLOT, kind, current as u32);
                        }
                        let brush = t("borders.add_brush");
                        if widgets::icon_button(ui, icon::PAINT_BRUSH_BROAD, brush).clicked() {
                            let graphic = if floor { state.graphic } else { state.land };
                            state.border_dialog.add(mask, graphic);
                        }
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
                    if tiles.is_empty() {
                        ui.label(
                            RichText::new(t("borders.empty"))
                                .font(style::small())
                                .color(style::color::TEXT_DIM),
                        );
                    }
                    for &graphic in &tiles {
                        let art = if floor {
                            state.art(&ctx, assets, graphic, 0)
                        } else {
                            state.land_art(&ctx, assets, graphic)
                        };
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(34.0, 34.0), Sense::click());
                        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
                            thumbnail(ui, art.as_ref(), 34.0)
                        });
                        if response.hovered() {
                            ui.painter().rect_stroke(
                                rect,
                                egui::Rounding::same(3.0),
                                Stroke::new(1.0_f32, style::color::DANGER),
                            );
                        }
                        let tip = tf("borders.remove", &[&format!("0x{graphic:04X}")]);
                        if response
                            .on_hover_text(tip)
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            state.border_dialog.remove(mask, graphic);
                        }
                    }
                });
            });
        });
}

/// The suffix of a set's kind in the lists: cave or floor.
fn kind_suffix(kind: Option<BorderKind>) -> &'static str {
    match kind {
        Some(BorderKind::Cave) => t("borders.cave_suffix"),
        Some(BorderKind::Floor) => t("borders.floor_suffix"),
        _ => "",
    }
}

/// The name of a border source, for the bar's selector.
pub(super) fn source_label(state: &MapEditor, source: &Source) -> String {
    match source {
        Source::Detect => t("borders.detect_here").into(),
        Source::Void => t("borders.detect_void").into(),
        Source::Set(name) => {
            let suffix = kind_suffix(state.border_set(name).map(|s| s.kind));
            tf("borders.set_option", &[name, &suffix])
        }
    }
}

/// The bar's selector: where the automatic border takes its borders from.
pub(super) fn source_combo(ui: &mut egui::Ui, state: &mut MapEditor) {
    let mut options = vec![Source::Detect, Source::Void];
    options.extend(
        state
            .border_sets
            .iter()
            .map(|s| Source::Set(s.name.clone())),
    );
    let mut chosen = None;
    egui::ComboBox::from_id_salt("map-border-source")
        .selected_text(source_label(state, &state.border_source))
        .width(190.0)
        .show_ui(ui, |ui| {
            for option in options {
                let on = state.border_source == option;
                if ui
                    .selectable_label(on, source_label(state, &option))
                    .clicked()
                {
                    chosen = Some(option);
                }
            }
        })
        .response
        .on_hover_text(t("borders.source_hint"));
    if let Some(source) = chosen {
        state.choose_border_source(source);
    }
}

/// The dialog, when open.
pub(super) fn window(
    ctx: &egui::Context,
    state: &mut MapEditor,
    picker: &mut picker::Picker,
    assets: &MapAssets,
) {
    if state.border_dialog.test_draft && state.border_draft(assets) {
        state.border_dialog.test_draft = false;
    }
    let mut open = true;
    let screen = ctx.screen_rect();
    let size = egui::vec2(
        (screen.width() - 120.0).clamp(420.0, 4.0 * (SLOT_WIDTH + 22.0) + 24.0),
        (screen.height() - 200.0).clamp(300.0, 640.0),
    );
    egui::Window::new(RichText::new(format!("{}  {}", icon::DIAMOND, t("borders.title"))).strong())
        .id(egui::Id::new("map-border-dialog"))
        .frame(egui::Frame::window(&ctx.style()).inner_margin(Margin::same(12.0)))
        .open(&mut open)
        .default_pos([60.0, 120.0])
        .fixed_size(size)
        .collapsible(false)
        .show(ctx, |ui| body(ui, state, picker, assets));
    state.border_dialog.open &= open;
}

fn body(ui: &mut egui::Ui, state: &mut MapEditor, picker: &mut picker::Picker, assets: &MapAssets) {
    let ctx = ui.ctx().clone();
    // The set, the name, the kind, save and delete.
    ui.horizontal_wrapped(|ui| {
        let current = state
            .border_dialog
            .saved_as
            .clone()
            .unwrap_or_else(|| t("borders.new").to_string());
        let mut chosen: Option<Option<String>> = None;
        egui::ComboBox::from_id_salt("map-border-set")
            .selected_text(current)
            .width(170.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(state.border_dialog.saved_as.is_none(), t("borders.new"))
                    .clicked()
                {
                    chosen = Some(None);
                }
                for set in &state.border_sets {
                    let on = state.border_dialog.saved_as.as_deref() == Some(set.name.as_str());
                    let suffix = kind_suffix(Some(set.kind));
                    if ui
                        .selectable_label(on, format!("{}{suffix}", set.name))
                        .clicked()
                    {
                        chosen = Some(Some(set.name.clone()));
                    }
                }
            });
        if let Some(name) = chosen {
            let set = name.and_then(|n| state.border_set(&n).cloned());
            state.border_dialog.edit(set.as_ref());
        }
        ui.add(
            egui::TextEdit::singleline(&mut state.border_dialog.editing.name)
                .hint_text(t("borders.name_hint"))
                .desired_width(170.0),
        );
        widgets::segmented(
            ui,
            &mut state.border_dialog.editing.kind,
            &[
                (BorderKind::Land, t("borders.kind_land")),
                (BorderKind::Cave, t("borders.kind_cave")),
                (BorderKind::Floor, t("borders.kind_floor")),
            ],
            420.0,
        );
        if widgets::action(
            ui,
            icon::FLOPPY_DISK,
            t("items.save"),
            widgets::Kind::Primary,
        )
        .clicked()
        {
            if let Some((set, old)) = state.border_dialog.save() {
                state.border_saved(set, old);
            }
        }
        if let Some(name) = state.border_dialog.saved_as.clone() {
            if widgets::action(ui, icon::TRASH, t("items.delete"), widgets::Kind::Danger).clicked()
            {
                state.border_deleted(&name);
            }
            if ui
                .button(t("borders.use"))
                .on_hover_text(t("borders.use_hint"))
                .clicked()
            {
                state.auto_border = true;
                state.choose_border_source(Source::Set(name));
            }
        }
    });
    ui.add_space(4.0);
    ui.label(
        RichText::new(t("borders.note"))
            .font(style::small())
            .color(style::color::TEXT_DIM),
    );
    ui.add_space(4.0);
    // The draft read from the art: the brush inside, and outside the black or a land.
    ui.horizontal_wrapped(|ui| {
        widgets::label(ui, t("borders.draft"));
        let floor = state.border_dialog.editing.kind == BorderKind::Floor;
        let (inside, brush) = if floor {
            (state.art(&ctx, assets, state.graphic, 0), state.graphic)
        } else {
            (state.land_art(&ctx, assets, state.land), state.land)
        };
        thumbnail(ui, inside.as_ref(), 28.0);
        ui.label(RichText::new(format!("0x{brush:04X}")).color(style::color::TEXT_DIM));
        if !floor {
            widgets::label(ui, t("borders.outside"));
        }
        match state.border_dialog.editing.kind {
            BorderKind::Floor => {}
            BorderKind::Cave => {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), Sense::hover());
                ui.painter().add(egui::Shape::convex_polygon(
                    vec![
                        rect.center_top(),
                        rect.right_center(),
                        rect.center_bottom(),
                        rect.left_center(),
                    ],
                    Color32::BLACK,
                    Stroke::new(1.0_f32, style::color::BORDER),
                ));
                ui.label(RichText::new(t("borders.black")).color(style::color::TEXT_DIM));
            }
            BorderKind::Land => {
                let outside = state.border_dialog.outside;
                let art = state.land_art(&ctx, assets, outside);
                let response = ui
                    .scope(|ui| thumbnail(ui, art.as_ref(), 28.0))
                    .response
                    .interact(Sense::click());
                if response
                    .on_hover_text(t("borders.pick_outside"))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    picker.open(PICK_OUTSIDE, picker::Kind::Land, outside as u32);
                }
                ui.label(RichText::new(format!("0x{outside:04X}")).color(style::color::TEXT_DIM));
            }
        }
        if ui
            .button(format!("{}  {}", icon::MAGIC_WAND, t("borders.detect")))
            .on_hover_text(t("borders.detect_hint"))
            .clicked()
        {
            state.border_draft(assets);
        }
    });
    if let Some((text, error)) = &state.border_dialog.message {
        let tint = if *error {
            style::color::DANGER
        } else {
            style::color::ACCENT
        };
        ui.label(RichText::new(text).color(tint));
    }
    ui.separator();
    // The shapes, by group.
    egui::ScrollArea::vertical()
        .id_salt("map-border-slots")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (key, masks) in GROUPS {
                ui.label(
                    RichText::new(group_label(key))
                        .font(style::heading())
                        .strong(),
                );
                ui.horizontal_wrapped(|ui| {
                    for &mask in masks {
                        slot(ui, state, picker, assets, mask);
                    }
                });
                ui.add_space(6.0);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_take_each_tile_once_and_lose_it_on_click() {
        let mut dialog = Dialog::default();
        dialog.add(TOP, 0x024A);
        dialog.add(TOP, 0x024A);
        dialog.add(TOP, 0x0262);
        assert_eq!(dialog.editing.tiles(TOP), [0x024A, 0x0262]);
        for g in 0..MAX_BORDER_VARIANTS as u16 {
            dialog.add(LEFT, 0x300 + g);
        }
        dialog.add(LEFT, 0x0264);
        assert_eq!(dialog.editing.tiles(LEFT).len(), MAX_BORDER_VARIANTS);
        assert!(dialog.message.as_ref().is_some_and(|(_, e)| *e));
        dialog.remove(TOP, 0x024A);
        dialog.remove(TOP, 0x0262);
        assert!(
            !dialog.editing.masks.contains_key(&TOP),
            "an empty shape goes"
        );
    }

    #[test]
    fn saving_needs_a_name_and_a_center_and_renaming_drops_the_old_name() {
        let mut dialog = Dialog::default();
        dialog.editing.name = "Cave Floor".into();
        assert!(dialog.save().is_none(), "invalid name");
        dialog.editing.name = " cave ".into();
        assert!(dialog.save().is_none(), "no core");
        assert!(dialog.message.as_ref().is_some_and(|(_, e)| *e));
        dialog.add(ALL, 0x0245);
        let (set, old) = dialog.save().unwrap();
        assert_eq!(set.name, "cave");
        assert!(old.is_none());
        dialog.saved_as = Some("old-cave".into());
        let (_, old) = dialog.save().unwrap();
        assert_eq!(old.as_deref(), Some("old-cave"));
    }
}
