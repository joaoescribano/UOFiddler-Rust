//! The frame of the full-screen map editor, over the map: the top bar (in two rows, `bar`), the
//! floating palette (objects, lands, templates), the border sets dialog (`border_dialog`), the
//! tile inspector on the right, the status bar at the bottom, the height band and the shortcuts
//! help. The tokens and components are the panel's (`style`, `widgets`).

use super::art::thumbnail;
use super::band::{self, Band, Handle};
use super::bar::PICK_HUE;
use super::state::{read_tile, visible, Changes, MapEditor, PaletteMode, StaticNow, Tool};
use super::text::{t, tf};
use crate::map::assets::MapAssets;
use crate::map::ui::{picker, style, widgets};
use eframe::egui::{self, Color32, Margin, RichText, Sense, Stroke};
use egui_phosphor::regular as icon;
use uo_core::MapTile;

/// The inspector's width, on the right.
pub const INSPECTOR_WIDTH: f32 = 300.0;

/// What the editor asks of whoever holds the client files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Write the pending changes into the facet's files.
    Save,
    /// Write a full copy of the facet's files, with the changes, into another folder.
    SaveAs,
    /// Drop every pending change.
    Discard,
    /// Save the pending changes as JSON.
    Export,
    /// Load pending changes from JSON.
    Import,
    /// Leave the editor.
    Exit,
}

/// The frame of a bar or column of the editor: the same card as the Map tab's toolbar.
pub fn frame() -> egui::Frame {
    egui::Frame::none()
        .fill(style::color::SURFACE)
        .stroke(Stroke::new(1.0_f32, style::color::BORDER))
        .rounding(egui::Rounding::same(style::RADIUS))
        .inner_margin(Margin::symmetric(10.0, 6.0))
}

/// The gap between the editor's cards and the map.
const GAP: f32 = 4.0;

/// The editor's frame inside the Map tab: the bar on top, the status at the bottom and the
/// inspector on the right, each a card like the tab's toolbar; the map goes in what is left
/// (the caller's `CentralPanel::show_inside`). What the actions changed goes into `actions`; what
/// needs the client files, into `requests`.
pub fn chrome(
    ui: &mut egui::Ui,
    state: &mut MapEditor,
    picker: &mut picker::Picker,
    assets: &MapAssets,
    actions: &mut Changes,
    requests: &mut Vec<Request>,
) {
    let ctx = ui.ctx().clone();
    if let Some(hue) = picker.take_result(PICK_HUE) {
        state.hue = hue as u16;
    }
    state.border_picks(picker);
    shortcuts(&ctx, state, assets, actions, requests);
    let around = |m: Margin| egui::Frame::none().inner_margin(m);
    egui::TopBottomPanel::top("map-editor-bar")
        .frame(around(Margin {
            bottom: GAP,
            ..Margin::ZERO
        }))
        .show_separator_line(false)
        .show_inside(ui, |ui| {
            frame()
                .inner_margin(Margin::symmetric(10.0, 2.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    super::bar::bar(ui, state, picker, assets, actions, requests);
                });
        });
    egui::TopBottomPanel::bottom("map-editor-status")
        .frame(around(Margin {
            top: GAP,
            ..Margin::ZERO
        }))
        .show_separator_line(false)
        .show_inside(ui, |ui| {
            frame().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                status(ui, state, assets);
            });
        });
    egui::SidePanel::right("map-editor-inspector")
        .resizable(false)
        .exact_width(INSPECTOR_WIDTH)
        .frame(around(Margin {
            left: GAP,
            ..Margin::ZERO
        }))
        .show_separator_line(false)
        .show_inside(ui, |ui| {
            frame().inner_margin(Margin::same(10.0)).show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                egui::ScrollArea::vertical()
                    .id_salt("map-inspector")
                    .auto_shrink([false, false])
                    .show(ui, |ui| inspector(ui, state, assets, actions));
            });
        });
}

/// What floats over the map: the height band on the map's right edge, the palette, the brush
/// thumbnail, the dialogs and the picker. `view` is where the map is drawn.
pub fn floating(
    ctx: &egui::Context,
    view: egui::Rect,
    state: &mut MapEditor,
    picker: &mut picker::Picker,
    assets: &MapAssets,
    actions: &mut Changes,
) {
    egui::Area::new(egui::Id::new("map-editor-band"))
        .pivot(egui::Align2::RIGHT_CENTER)
        .fixed_pos(view.right_center() - egui::vec2(12.0, 0.0))
        .show(ctx, |ui| {
            frame().show(ui, |ui| {
                band_slider(ui, &mut state.band);
            });
        });
    if state.palette {
        palette(ctx, view, state, picker, assets);
    }
    super::brush_card::follower(ctx, state, assets);
    if state.border_dialog.open {
        super::border_dialog::window(ctx, state, picker, assets);
    }
    if state.help {
        help(ctx, &mut state.help);
    }
    if state.roof.is_some() {
        roof_prompt(ctx, state, assets, actions);
    }
    state.typing = ctx.wants_keyboard_input();
    picker.show(ctx, assets);
}

/// The keyboard shortcuts, when no text field has the focus.
fn shortcuts(
    ctx: &egui::Context,
    state: &mut MapEditor,
    assets: &MapAssets,
    actions: &mut Changes,
    requests: &mut Vec<Request>,
) {
    if ctx.wants_keyboard_input() {
        return;
    }
    let (keys, ctrl, shift) = ctx.input(|i| {
        let pressed: Vec<egui::Key> = i
            .events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key {
                    key, pressed: true, ..
                } => Some(*key),
                // Ctrl+C, X and V arrive as clipboard events, not as keys.
                egui::Event::Copy => Some(egui::Key::Copy),
                egui::Event::Cut => Some(egui::Key::Cut),
                egui::Event::Paste(_) => Some(egui::Key::Paste),
                _ => None,
            })
            .collect();
        (pressed, i.modifiers.command, i.modifiers.shift)
    });
    let region = state.tool == Tool::Region && state.region.is_some();
    use egui::Key;
    for key in keys {
        let step = if shift { 5 } else { 1 };
        match key {
            Key::S if ctrl && shift => requests.push(Request::SaveAs),
            Key::S if ctrl => requests.push(Request::Save),
            Key::Z if ctrl && shift => actions.extend(state.redo()),
            Key::Z if ctrl => actions.extend(state.undo()),
            Key::Y if ctrl => actions.extend(state.redo()),
            Key::Copy => state.copy(assets),
            Key::C if ctrl => state.copy(assets),
            Key::Cut => actions.extend(state.cut(assets)),
            Key::X if ctrl => actions.extend(state.cut(assets)),
            Key::Paste => state.start_paste(),
            Key::V if ctrl => state.start_paste(),
            _ if ctrl => {}
            Key::F11 => requests.push(Request::Exit),
            Key::V => state.choose(Tool::Select),
            Key::B => state.choose(Tool::Stamp),
            Key::T => state.choose(Tool::Terrain),
            Key::U => state.choose(Tool::Height),
            Key::E => state.choose(Tool::Erase),
            Key::R => state.choose(Tool::Region),
            Key::C if state.picked.is_some() => state.use_as_brush(assets),
            Key::I => state.toggle_palette(),
            Key::H => state.grid = !state.grid,
            Key::Questionmark | Key::Slash => state.help = !state.help,
            Key::Delete | Key::Backspace if region => actions.extend(state.delete_region(assets)),
            Key::Delete | Key::Backspace => actions.extend(state.delete_picked(assets)),
            Key::ArrowUp if region => actions.extend(state.shift_region(assets, 0, -1)),
            Key::ArrowDown if region => actions.extend(state.shift_region(assets, 0, 1)),
            Key::ArrowLeft if region => actions.extend(state.shift_region(assets, -1, 0)),
            Key::ArrowRight if region => actions.extend(state.shift_region(assets, 1, 0)),
            Key::PageUp if region => actions.extend(state.lift_region(assets, step)),
            Key::PageDown if region => actions.extend(state.lift_region(assets, -step)),
            Key::ArrowUp if state.picked.is_some() => {
                actions.extend(state.nudge_picked(assets, 0, -1))
            }
            Key::ArrowDown if state.picked.is_some() => {
                actions.extend(state.nudge_picked(assets, 0, 1))
            }
            Key::ArrowLeft if state.picked.is_some() => {
                actions.extend(state.nudge_picked(assets, -1, 0))
            }
            Key::ArrowRight if state.picked.is_some() => {
                actions.extend(state.nudge_picked(assets, 1, 0))
            }
            Key::PageUp => actions.extend(state.lift(assets, step)),
            Key::PageDown => actions.extend(state.lift(assets, -step)),
            Key::Escape => {
                if state.help {
                    state.help = false;
                } else if state.roof.is_some() {
                    state.roof = None;
                } else if state.border_dialog.open {
                    state.border_dialog.open = false;
                } else if state.pasting {
                    state.pasting = false;
                    state.moving = None;
                } else if state.palette {
                    state.palette = false;
                } else if state.tool == Tool::Region && state.region.is_some() {
                    state.region = None;
                } else if state.picked.is_some() || state.selected.is_some() {
                    state.picked = None;
                    state.selected = None;
                } else {
                    state.choose(Tool::Select);
                }
            }
            _ => {}
        }
    }
}

/// The status bar: the cursor's tile, the last message and the tool's hint.
fn status(ui: &mut egui::Ui, state: &MapEditor, assets: &MapAssets) {
    ui.horizontal(|ui| {
        let here = state.hover.and_then(|(x, y)| read_tile(assets, x, y));
        let cursor = match &here {
            Some(tile) => format!(
                "{}, {}, {}  ·  {}",
                tile.x,
                tile.y,
                tile.land.2,
                if tile.land.1.is_empty() {
                    format!("0x{:04X}", tile.land.0)
                } else {
                    tile.land.1.clone()
                }
            ),
            None => "-".to_string(),
        };
        ui.label(RichText::new(cursor).font(style::body()));
        ui.add_space(16.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let band = if state.band.is_full() {
                String::new()
            } else {
                format!("  ·  Z {}..{}", state.band.from, state.band.to)
            };
            ui.label(
                RichText::new(format!("{}{band}", state.tool.hint())).color(style::color::TEXT_DIM),
            );
            ui.add_space(16.0);
            // The message takes what is left, cut with an ellipsis (the whole text on hover).
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                if let Some((text, error)) = &state.message {
                    let tint = if *error {
                        style::color::DANGER
                    } else {
                        style::color::ACCENT
                    };
                    ui.add(egui::Label::new(RichText::new(text).color(tint)).truncate())
                        .on_hover_text(text);
                }
            });
        });
    });
}

/// The inspector: the region (with the region tool), or what the chosen tile holds (or the one
/// under the cursor).
fn inspector(ui: &mut egui::Ui, state: &mut MapEditor, assets: &MapAssets, actions: &mut Changes) {
    if state.tool == Tool::Region {
        return region_inspector(ui, state, assets, actions);
    }
    let at = if state.tool == Tool::Select {
        state.selected.or(state.hover)
    } else {
        state.hover
    };
    let Some((x, y)) = at else {
        widgets::empty(ui, icon::CURSOR_CLICK, t("map.click_tile"));
        return;
    };
    let Some(tile) = read_tile(assets, x, y) else {
        widgets::empty(ui, icon::HOURGLASS, t("map.loading"));
        return;
    };
    let ctx = ui.ctx().clone();
    let edit = state.edit_of(x, y);
    let chosen = state.tool == Tool::Select && state.selected == Some((x, y));
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tf("map.tile", &[&x, &y]))
                .font(style::heading())
                .strong(),
        );
        if state.tiles.get(x, y).is_some() {
            widgets::chip(ui, t("map.edited"), style::color::ACCENT);
        }
    });
    if !chosen {
        ui.label(
            RichText::new(t("map.click_to_pick"))
                .font(style::small())
                .color(style::color::TEXT_DIM),
        );
    }
    ui.add_space(6.0);
    // The ground.
    widgets::section(ui, "map-inspector-land", t("map.land"), true, |ui| {
        let art = state.land_art(&ctx, assets, tile.land.0);
        ui.horizontal(|ui| {
            thumbnail(ui, art.as_ref(), 40.0);
            ui.vertical(|ui| {
                ui.label(format!("0x{:04X} {}", tile.land.0, tile.land.1));
                if !state.band.contains(tile.land.2 as i32) {
                    widgets::chip(ui, t("map.land_hidden"), style::color::TEXT_DIM);
                }
                ui.horizontal(|ui| {
                    widgets::label(ui, t("map.land_z"));
                    let mut z = tile.land.2;
                    let changed = ui
                        .add_enabled(chosen, egui::DragValue::new(&mut z).range(-128..=127))
                        .changed();
                    if changed {
                        let mut new = edit.clone();
                        new.land_z = Some(z);
                        actions.extend(state.commit(vec![new]));
                    }
                });
            });
        });
        if chosen
            && (edit.land.is_some() || edit.land_z.is_some())
            && ui.small_button(t("map.restore_land")).clicked()
        {
            let mut new = edit.clone();
            new.land = None;
            new.land_z = None;
            actions.extend(state.commit(vec![new]));
        }
    });
    ui.add_space(6.0);
    // The statics the height band leaves visible, top to bottom; those it hides stay out, only
    // counted.
    let shown: Vec<&StaticNow> = visible(&tile, state.band).collect();
    let hidden = tile.statics.len() - shown.len();
    let title = tf("map.objects", &[&shown.len()]);
    widgets::section(ui, "map-inspector-objects", &title, true, |ui| {
        if shown.is_empty() {
            ui.label(RichText::new(t("map.no_objects")).color(style::color::TEXT_DIM));
        }
        if hidden > 0 {
            ui.label(
                RichText::new(tf("map.hidden_objects", &[&hidden]))
                    .font(style::small())
                    .color(style::color::TEXT_DIM),
            );
        }
        for s in shown.iter().rev().copied() {
            let picked = chosen && state.picked == Some((s.graphic, s.z));
            let art = state.art(&ctx, assets, s.graphic, s.hue);
            let row = egui::Frame::none()
                .fill(if picked {
                    style::color::ACCENT_BG
                } else {
                    Color32::TRANSPARENT
                })
                .rounding(egui::Rounding::same(4.0))
                .inner_margin(Margin::same(3.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        thumbnail(ui, art.as_ref(), 32.0);
                        ui.vertical(|ui| {
                            let name = if s.name.is_empty() {
                                "-"
                            } else {
                                s.name.as_str()
                            };
                            ui.label(RichText::new(name).color(if picked {
                                style::color::ACCENT
                            } else {
                                style::color::TEXT
                            }));
                            ui.label(
                                RichText::new(tf(
                                    "map.static_line",
                                    &[&format!("0x{:04X}", s.graphic), &s.z, &s.hue],
                                ))
                                .font(style::small())
                                .color(style::color::TEXT_DIM),
                            );
                        });
                    });
                });
            let response = ui.interact(
                row.response.rect,
                ui.id().with(("static", s.graphic, s.z)),
                Sense::click(),
            );
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                state.selected = Some((x, y));
                state.picked = Some((s.graphic, s.z));
                state.tool = Tool::Select;
            }
            if picked {
                picked_fields(ui, state, assets, s, actions);
            }
        }
    });
    // What left the original map on this tile, to bring back.
    if !edit.remove.is_empty() {
        ui.add_space(6.0);
        let title = tf("map.removed", &[&edit.remove.len()]);
        widgets::section(ui, "map-inspector-removed", &title, false, |ui| {
            for gone in edit.remove.clone() {
                ui.horizontal(|ui| {
                    let art = state.art(&ctx, assets, gone.graphic, 0);
                    thumbnail(ui, art.as_ref(), 28.0);
                    ui.label(format!("0x{:04X}  z {}", gone.graphic, gone.z));
                    if ui.small_button(t("map.restore")).clicked() {
                        let mut new = edit.clone();
                        new.remove.retain(|r| *r != gone);
                        actions.extend(state.commit(vec![new]));
                    }
                });
            }
        });
    }
    if chosen && !edit.is_empty() {
        ui.add_space(8.0);
        if widgets::action(
            ui,
            icon::ARROW_COUNTER_CLOCKWISE,
            t("map.revert"),
            widgets::Kind::Danger,
        )
        .clicked()
        {
            actions.extend(state.commit(vec![MapTile::fresh(x, y)]));
        }
    }
    ui.add_space(8.0);
    ui.label(
        RichText::new(t("map.keys_hint"))
            .font(style::small())
            .color(style::color::TEXT_DIM),
    );
}

/// The chosen static's fields: height, hue and graphic, apply, use as brush and delete.
fn picked_fields(
    ui: &mut egui::Ui,
    state: &mut MapEditor,
    assets: &MapAssets,
    s: &StaticNow,
    actions: &mut Changes,
) {
    let id = ui.id().with(("picked-fields", s.graphic, s.z));
    let mut fields: (u16, i8, u16) = ui
        .data(|d| d.get_temp(id))
        .unwrap_or((s.graphic, s.z, s.hue));
    ui.indent(id, |ui| {
        widgets::fields(ui, "map-picked-grid", |ui| {
            widgets::label(ui, t("items.graphic"));
            ui.add(egui::DragValue::new(&mut fields.0).hexadecimal(4, false, true));
            ui.end_row();
            widgets::label(ui, "Z");
            ui.add(egui::DragValue::new(&mut fields.1).range(-128..=127));
            ui.end_row();
            widgets::label(ui, t("items.hue"));
            ui.add(egui::DragValue::new(&mut fields.2).range(0..=3000));
            ui.end_row();
        });
        ui.horizontal_wrapped(|ui| {
            let changed = fields != (s.graphic, s.z, s.hue);
            if ui
                .add_enabled(changed, egui::Button::new(t("map.apply")))
                .clicked()
            {
                if let Some(at) = state.selected {
                    actions.extend(state.change_picked(assets, at, fields.1, fields.2, fields.0));
                }
            }
            if ui
                .button(t("map.use_as_brush"))
                .on_hover_text(t("map.use_as_brush_hint"))
                .clicked()
            {
                state.use_as_brush(assets);
            }
            if ui
                .button(RichText::new(t("map.delete")).color(style::color::DANGER))
                .clicked()
            {
                actions.extend(state.delete_picked(assets));
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(id, fields));
}

/// The region tool's inspector: the size and contents of the chosen one, copy, cut, paste, move,
/// delete, fill and save as a template.
fn region_inspector(
    ui: &mut egui::Ui,
    state: &mut MapEditor,
    assets: &MapAssets,
    actions: &mut Changes,
) {
    ui.label(
        RichText::new(t("map.region"))
            .font(style::heading())
            .strong(),
    );
    ui.label(
        RichText::new(t("map.region_inspector_hint"))
            .font(style::small())
            .color(style::color::TEXT_DIM),
    );
    ui.add_space(6.0);
    if state.pasting {
        widgets::card(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let key = if state.moving.is_some() {
                "map.moving"
            } else {
                "map.pasting"
            };
            ui.label(RichText::new(t(key)).color(style::color::ACCENT));
            if ui.button(t("map.cancel")).clicked() {
                state.pasting = false;
                state.moving = None;
            }
        });
        ui.add_space(6.0);
    }
    let Some(area) = state.region else {
        widgets::empty(ui, icon::SELECTION, t("map.drag_region"));
        clipboard_line(ui, state);
        return;
    };
    let count: usize = (area.y0..=area.y1)
        .flat_map(|y| (area.x0..=area.x1).map(move |x| (x, y)))
        .filter_map(|(x, y)| read_tile(assets, x, y))
        .map(|t| {
            t.statics
                .iter()
                .filter(|s| state.band.contains(s.z as i32))
                .count()
        })
        .sum();
    let (w, h) = (area.x1 - area.x0 + 1, area.y1 - area.y0 + 1);
    let title = tf("map.region_size", &[&w, &h]);
    widgets::section(ui, "map-region", &title, true, |ui| {
        ui.label(
            RichText::new(tf(
                "map.region_info",
                &[&area.x0, &area.y0, &area.x1, &area.y1, &count],
            ))
            .color(style::color::TEXT_DIM),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button(t("map.copy")).clicked() {
                state.copy(assets);
            }
            if ui.button(t("map.cut")).clicked() {
                actions.extend(state.cut(assets));
            }
            if ui.button(t("map.paste")).clicked() {
                state.start_paste();
            }
            if ui.button(t("map.move")).clicked() {
                state.start_move(assets);
            }
            if ui
                .button(RichText::new(t("map.delete_region")).color(style::color::DANGER))
                .clicked()
            {
                actions.extend(state.delete_region(assets));
            }
        });
        widgets::toggle_chip(ui, &mut state.with_land, t("map.with_land"))
            .on_hover_text(t("map.with_land_hint"));
    });
    ui.add_space(6.0);
    widgets::section(ui, "map-region-fill", t("map.fill"), true, |ui| {
        ui.horizontal(|ui| {
            let ctx = ui.ctx().clone();
            let art = state.art(&ctx, assets, state.graphic, state.hue);
            thumbnail(ui, art.as_ref(), 32.0);
            ui.add(
                egui::Label::new(
                    RichText::new(t("map.fill_hint"))
                        .font(style::small())
                        .color(style::color::TEXT_DIM),
                )
                .wrap(),
            );
        });
        ui.horizontal(|ui| {
            widgets::label(ui, t("map.density"));
            ui.add(egui::Slider::new(&mut state.density, 1..=100).suffix("%"));
        });
        widgets::toggle_chip(ui, &mut state.empty_only, t("map.empty_only"));
        ui.add_space(4.0);
        if widgets::action(ui, icon::PAINT_BUCKET, t("map.fill"), widgets::Kind::Normal).clicked() {
            actions.extend(state.fill_region(assets));
        }
    });
    ui.add_space(6.0);
    widgets::section(
        ui,
        "map-region-template",
        t("map.save_template"),
        true,
        |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.template_name)
                        .hint_text(t("map.template_name"))
                        .desired_width(150.0),
                );
                if ui.button(t("items.save")).clicked() {
                    state.save_template(assets);
                }
            });
        },
    );
    clipboard_line(ui, state);
}

/// What is copied, below the region inspector.
fn clipboard_line(ui: &mut egui::Ui, state: &MapEditor) {
    if let Some(clip) = &state.clip {
        ui.add_space(8.0);
        ui.label(
            RichText::new(tf("map.clipboard", &[&clip.w, &clip.h, &clip.pieces.len()]))
                .font(style::small())
                .color(style::color::TEXT_DIM),
        );
    }
}

/// The templates in the palette: a click takes one to paste; the bin deletes it.
fn templates(ui: &mut egui::Ui, state: &mut MapEditor) {
    let list: Vec<(String, i32, i32, usize)> = state
        .templates()
        .iter()
        .map(|t| (t.name.clone(), t.clip.w, t.clip.h, t.clip.pieces.len()))
        .collect();
    if list.is_empty() {
        widgets::empty(ui, icon::STACK, t("map.no_templates"));
        return;
    }
    let mut chosen = None;
    let mut gone = None;
    egui::ScrollArea::vertical()
        .id_salt("map-templates")
        .auto_shrink([false, false])
        .max_height(ui.available_height() - 24.0)
        .show(ui, |ui| {
            for (name, w, h, count) in &list {
                ui.horizontal(|ui| {
                    if widgets::icon_button(ui, icon::TRASH, t("map.delete_template")).clicked() {
                        gone = Some(name.clone());
                    }
                    let detail = format!("{w}×{h} · {count}");
                    let label = format!("{}  {name}", icon::STACK);
                    if widgets::list_row(ui, false, &label, Some(&detail)).clicked() {
                        chosen = Some(name.clone());
                    }
                });
            }
        });
    ui.label(
        RichText::new(t("map.template_hint"))
            .font(style::small())
            .color(style::color::TEXT_DIM),
    );
    if let Some(name) = chosen {
        state.use_template(&name);
    }
    if let Some(name) = gone {
        state.delete_template(&name);
    }
}

/// The height band: an upright rail, the highest Z on top, with two handles (the end and the
/// start). Dragging moves by 1 (Shift: by storeys); a click picks the nearest handle, and the
/// arrows (↑ ↓) move it by 1; the wheel over the rail moves the handle nearest the cursor
/// `WHEEL_STEP` per notch (Shift: by 1). A double click goes back to the full band.
fn band_slider(ui: &mut egui::Ui, band: &mut Band) {
    let height = 260.0;
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(band.to.to_string()).font(style::small()));
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(34.0, height), Sense::click_and_drag());
        let z_at = |y: f32| -> i32 {
            let f = ((rect.bottom() - y) / rect.height()).clamp(0.0, 1.0);
            band::LOWEST + (f * (band::HIGHEST - band::LOWEST) as f32).round() as i32
        };
        let y_of = |z: i32| -> f32 {
            let f = (z - band::LOWEST) as f32 / (band::HIGHEST - band::LOWEST) as f32;
            rect.bottom() - f * rect.height()
        };
        let painter = ui.painter_at(rect.expand(8.0));
        let track = egui::Rect::from_center_size(rect.center(), egui::vec2(6.0, rect.height()));
        painter.rect_filled(track, egui::Rounding::same(3.0), style::color::SURFACE_2);
        let fill = egui::Rect::from_min_max(
            egui::pos2(track.left(), y_of(band.to)),
            egui::pos2(track.right(), y_of(band.from)),
        );
        painter.rect_filled(fill, egui::Rounding::same(3.0), style::color::ACCENT);
        // The storeys, every 20.
        let mut z = (band::LOWEST / band::FLOOR) * band::FLOOR;
        while z <= band::HIGHEST {
            let y = y_of(z);
            painter.line_segment(
                [
                    egui::pos2(rect.left() + 4.0, y),
                    egui::pos2(rect.left() + 9.0, y),
                ],
                Stroke::new(1.0_f32, style::color::BORDER),
            );
            z += band::FLOOR;
        }
        let shift = ui.input(|i| i.modifiers.shift);
        let id = response.id;
        // The chosen handle: the one the last click or drag took.
        let mut handle: Handle = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        let pointer_z = response.interact_pointer_pos().map(|at| z_at(at.y));
        if response.drag_started() || response.clicked() {
            if let Some(z) = pointer_z {
                handle = band.nearest(z);
            }
            // With the focus, the arrows move the handle (and not the camera nor the object).
            response.request_focus();
        }
        if response.dragged() {
            if let Some(z) = pointer_z {
                *band = band.with(handle, z, shift);
            }
        }
        if response.has_focus() {
            // The arrows stay with the rail (egui would use them to move the focus along).
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    egui::EventFilter {
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
            let (up, down) = ui.input(|i| {
                (
                    i.num_presses(egui::Key::ArrowUp) as i32,
                    i.num_presses(egui::Key::ArrowDown) as i32,
                )
            });
            if up != down {
                *band = band.nudge(handle, up - down);
            }
            // A click off the rail gives the arrows back to the editor.
            if ui.input(|i| i.pointer.any_pressed()) && !response.hovered() {
                response.surrender_focus();
            }
        }
        // The wheel over the rail: the handle nearest the cursor, by notches.
        if response.hovered() {
            let delta = ui.input(|i| {
                let d = i.raw_scroll_delta;
                // With Shift egui sends the wheel as a sideways scroll.
                if d.y != 0.0 {
                    d.y
                } else {
                    d.x
                }
            });
            let notches = band::notches(delta, POINTS_PER_NOTCH);
            if notches != 0 {
                if let Some(at) = ui.input(|i| i.pointer.hover_pos()) {
                    handle = band.nearest(z_at(at.y));
                }
                *band = band.nudge(handle, band::wheel_step(notches, shift));
            }
        }
        ui.data_mut(|d| d.insert_temp(id, handle));
        if response.double_clicked() {
            *band = Band::FULL;
        }
        let focused = response.has_focus();
        for (z, which) in [(band.to, Handle::Top), (band.from, Handle::Bottom)] {
            let center = egui::pos2(rect.center().x, y_of(z));
            // The chosen handle, with the focus, is filled.
            let inside = if focused && which == handle {
                style::color::ACCENT
            } else {
                style::color::SURFACE
            };
            painter.circle(
                center,
                7.0,
                inside,
                Stroke::new(2.0_f32, style::color::ACCENT),
            );
        }
        response.on_hover_text(t("map.band_hint"));
        ui.label(RichText::new(band.from.to_string()).font(style::small()));
    });
}

/// How many points egui reports for one wheel notch.
const POINTS_PER_NOTCH: f32 = 50.0;

/// The floating palette: objects, lands and templates, with search, categories and the visual
/// picker's grid. A click picks and arms the right tool.
fn palette(
    ctx: &egui::Context,
    view: egui::Rect,
    state: &mut MapEditor,
    picker: &mut picker::Picker,
    assets: &MapAssets,
) {
    let mut open = true;
    // A size fixed by the screen: the grid measures itself by the window, and a window that grows
    // with its content would grow with the grid.
    let screen = ctx.screen_rect();
    let size = egui::vec2(
        (screen.width() * 0.42).clamp(380.0, 640.0),
        (screen.height() - 230.0).clamp(260.0, 520.0),
    );
    egui::Window::new(
        RichText::new(format!("{}  {}", icon::SQUARES_FOUR, t("map.palette"))).strong(),
    )
    .id(egui::Id::new("map-editor-palette"))
    .frame(egui::Frame::window(&ctx.style()).inner_margin(Margin::same(10.0)))
    .open(&mut open)
    .default_pos(view.min + egui::vec2(12.0, 12.0))
    .fixed_size(size)
    .collapsible(true)
    .show(ctx, |ui| {
        let width = ui.available_width().min(420.0);
        widgets::segmented(
            ui,
            &mut state.palette_mode,
            &[
                (PaletteMode::Objects, t("map.objects_mode")),
                (PaletteMode::Lands, t("map.lands_mode")),
                (PaletteMode::Templates, t("map.templates_mode")),
            ],
            width,
        );
        ui.add_space(6.0);
        match state.palette_mode {
            PaletteMode::Objects => {
                if let Some(id) = picker.browse(ui, &mut state.statics_browse, assets) {
                    state.pick_static(id as u16);
                    if !matches!(state.tool, Tool::Stamp) {
                        state.choose(Tool::Stamp);
                    }
                }
            }
            PaletteMode::Lands => {
                if let Some(id) = picker.browse(ui, &mut state.lands_browse, assets) {
                    state.pick_land(id as u16);
                    state.choose(Tool::Terrain);
                }
            }
            PaletteMode::Templates => templates(ui, state),
        }
    });
    state.palette = open;
}

/// The question when terrain is painted over the height band (the rock of a cave): use the cave
/// floor, paint anyway, or nothing.
fn roof_prompt(
    ctx: &egui::Context,
    state: &mut MapEditor,
    assets: &MapAssets,
    actions: &mut Changes,
) {
    let count = state.roof.as_ref().map_or(0, |(_, n)| *n);
    let text = if count > 1 {
        tf("map.roof_many", &[&count])
    } else {
        t("map.roof_one").to_string()
    };
    let mut close = false;
    egui::Window::new("map-roof-prompt")
        .title_bar(false)
        .frame(egui::Frame::window(&ctx.style()).inner_margin(Margin::same(16.0)))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .resizable(false)
        .collapsible(false)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_max_width(380.0);
            ui.label(RichText::new(text).font(style::body()));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if widgets::action(
                    ui,
                    icon::STAMP,
                    t("map.use_cave_floor"),
                    widgets::Kind::Primary,
                )
                .clicked()
                {
                    state.use_cave_floor();
                }
                if widgets::action(
                    ui,
                    icon::PAINT_BRUSH_BROAD,
                    t("map.paint_anyway"),
                    widgets::Kind::Normal,
                )
                .clicked()
                {
                    actions.extend(state.roof_anyway(assets));
                }
                if ui.button(t("map.cancel")).clicked() {
                    close = true;
                }
            });
        });
    if close {
        state.roof = None;
    }
}

/// The shortcuts help.
fn help(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new(
        RichText::new(format!("{}  {}", icon::KEYBOARD, t("map.shortcuts"))).strong(),
    )
    .id(egui::Id::new("map-editor-help"))
    .frame(egui::Frame::window(&ctx.style()).inner_margin(Margin::same(14.0)))
    .open(open)
    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
    .resizable(false)
    .collapsible(false)
    .show(ctx, |ui| {
        let rows = [
            ("V B T U E R", "map.keys_tools"),
            ("I · H · ?", "map.keys_palette"),
            ("W A S D", "map.keys_walk"),
            ("↑ ↓ ← →", "map.keys_move"),
            ("PgUp · PgDn", "map.keys_z"),
            ("Del · C", "map.keys_delete"),
            ("Ctrl+Z · Ctrl+Y", "map.keys_undo"),
            ("Ctrl+C · X · V", "map.keys_copy"),
            ("Ctrl+S · Ctrl+Shift+S", "map.keys_save"),
            ("Alt", "map.keys_alt"),
            ("Ctrl + wheel", "map.keys_band"),
            ("Esc", "map.keys_esc"),
            ("F11", "map.keys_exit"),
        ];
        egui::Grid::new("map-help-grid")
            .num_columns(2)
            .spacing([18.0, 8.0])
            .show(ui, |ui| {
                for (keys, what) in rows {
                    ui.label(
                        RichText::new(keys)
                            .font(egui::FontId::monospace(13.0))
                            .color(style::color::ACCENT),
                    );
                    ui.label(t(what));
                    ui.end_row();
                }
            });
        ui.add_space(6.0);
        ui.label(
            RichText::new(t("map.help_note"))
                .font(style::small())
                .color(style::color::TEXT_DIM),
        );
    });
}
