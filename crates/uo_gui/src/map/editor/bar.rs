//! The editor's top bar, in two rows. On the first, the title and the tools (icon, name and
//! key). On the second, on the left, only the options of the chosen tool; on the right, what
//! always applies (palette, undo and redo, grid, go to, help, save, exit). Nothing wraps into
//! more rows: the options that do not fit go to the "⋯" menu, at the end of the left part. The
//! width of each option is the one of the last frame it showed in the bar (kept in `egui`), and
//! the first time a guess is used. The current brush (`brush_card`) is one of the options of the
//! stamp, terrain and region.
//!
//! Both rows have a fixed height (`ROW`): each is a rectangle reserved before drawing, and what
//! goes inside is clipped to it, so the bar has the same height with every tool and window width.
//! Groups are split by dividers, and icon-only buttons all have the same width.

use super::brush::{Shape, SIZES};
use super::chrome::Request;
use super::state::{parse_xy, Changes, MapEditor, PaletteMode, Tool};
use super::height::Mode;
use super::text::{t, tf};
use crate::map::assets::MapAssets;
use crate::map::ui::{picker, style, widgets};
use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

/// The picker's caller for the stamp's hue.
pub(super) const PICK_HUE: &str = "map-stamp-hue";

/// The height of each row of the bar.
pub(super) const ROW: f32 = 30.0;
/// The height of the buttons inside a row.
const BUTTON: f32 = 26.0;
/// The space between the items of a row.
const GAP: f32 = 6.0;
/// The width of the divider between groups.
const DIVIDER: f32 = 3.0;
/// Below this width, what always applies goes compact (icons only).
const COMPACT_BELOW: f32 = 1200.0;

/// An option of the tool, on the second row (or in the "⋯" menu).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Item {
    /// What the tool does, when it has no options.
    Hint,
    /// The brush: size and shape.
    Brush,
    /// The working Z: empty is automatic.
    WorkingZ,
    /// The stamp's hue.
    Hue,
    /// The current brush (`brush_card`).
    Current,
    /// The height mode and the step.
    HeightMode,
    /// The eraser takes the whole tile.
    EraseAll,
    /// Land varies among similar whole tiles.
    Vary,
    /// Land gets borders with the land outside, and where they come from.
    AutoBorder,
    /// The border sets dialog.
    BorderSets,
    /// Copy, cut, paste, move and delete the region.
    RegionClip,
    /// Fill the region with the stamp.
    RegionFill,
    /// Copying takes the land.
    WithLand,
    /// The templates, in the palette.
    Templates,
}

impl Item {
    /// A guess of the width, before the first time in the bar.
    fn guess(self) -> f32 {
        match self {
            Item::Hint => 100.0,
            Item::Brush => 180.0,
            Item::WorkingZ => 60.0,
            Item::Hue => 110.0,
            Item::Current => 200.0,
            Item::AutoBorder => 200.0,
            Item::HeightMode => 130.0,
            Item::RegionClip => 280.0,
            _ => 90.0,
        }
    }
}

/// Each tool's options, in bar order: the most used first (the last ones go to the "⋯" when room
/// runs out).
pub(super) fn options(tool: Tool) -> &'static [Item] {
    match tool {
        Tool::Select => &[Item::Hint],
        Tool::Stamp => &[Item::Current, Item::Brush, Item::WorkingZ, Item::Hue],
        Tool::Terrain => &[
            Item::Current,
            Item::Brush,
            Item::WorkingZ,
            Item::Vary,
            Item::AutoBorder,
            Item::BorderSets,
        ],
        Tool::Height => &[Item::Brush, Item::HeightMode, Item::WorkingZ],
        Tool::Erase => &[Item::Brush, Item::EraseAll],
        Tool::Region => &[
            Item::RegionClip,
            Item::Current,
            Item::RegionFill,
            Item::WithLand,
            Item::Templates,
        ],
    }
}

/// How many of the widths fit in `room`, in order. If not all fit, the "⋯" button (`more`) has
/// to fit too.
pub(super) fn fit(widths: &[f32], room: f32, gap: f32, more: f32) -> usize {
    let total: f32 = widths.iter().map(|w| w + gap).sum();
    if total <= room {
        return widths.len();
    }
    let mut used = more + gap;
    widths
        .iter()
        .take_while(|w| {
            used += *w + gap;
            used <= room
        })
        .count()
}

/// A bar button: icon, text and key; the active one in gold.
pub(super) fn tool_button(
    ui: &mut egui::Ui,
    on: bool,
    glyph: &str,
    text: &str,
    key: &str,
) -> egui::Response {
    let mut job = egui::text::LayoutJob::default();
    let tint = if on {
        style::color::ACCENT
    } else {
        style::color::TEXT
    };
    let font = style::body();
    let lead = if text.is_empty() {
        glyph.to_string()
    } else {
        format!("{glyph} ")
    };
    job.append(&lead, 0.0, egui::TextFormat::simple(font.clone(), tint));
    job.append(text, 0.0, egui::TextFormat::simple(font, tint));
    if !key.is_empty() {
        job.append(
            key,
            6.0,
            egui::TextFormat::simple(style::small(), style::color::TEXT_DIM),
        );
    }
    let fill = if on {
        style::color::ACCENT_BG
    } else {
        style::color::SURFACE_2
    };
    // Icon-only ones all have the same width.
    let min_width = if text.is_empty() { BUTTON + 4.0 } else { 0.0 };
    ui.add(
        egui::Button::new(job)
            .fill(fill)
            .min_size(egui::vec2(min_width, BUTTON)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// An icon-only framed button, for the bar.
fn icon_tool(ui: &mut egui::Ui, glyph: &str, tip: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(glyph).font(style::icon()))
            .min_size(egui::vec2(BUTTON + 4.0, BUTTON)),
    )
    .on_hover_text(tip)
    .on_disabled_hover_text(tip)
}

/// The divider between two groups of a row: an upright stroke, a bit shorter than the row, the
/// same all over the bar.
fn divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(DIVIDER, ROW - 8.0), egui::Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        egui::Stroke::new(1.0_f32, style::color::BORDER),
    );
}

/// A bar row: a rectangle `ROW` high reserved across the whole width, and what goes inside drawn
/// in it and clipped to it, without growing.
fn row(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, ROW), egui::Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child.spacing_mut().item_spacing.x = GAP;
    add(&mut child);
}

/// The whole bar, in the two fixed-height rows.
pub(super) fn bar(
    ui: &mut egui::Ui,
    state: &mut MapEditor,
    picker: &mut picker::Picker,
    assets: &MapAssets,
    actions: &mut Changes,
    requests: &mut Vec<Request>,
) {
    ui.spacing_mut().item_spacing = egui::vec2(GAP, 4.0);
    // Row 1: the title and the tools; the facet's save buttons on the right.
    row(ui, |ui| {
        ui.label(
            RichText::new(format!("{}  {}", icon::MAP_TRIFOLD, t("map.title")))
                .font(style::heading())
                .color(style::color::ACCENT),
        );
        divider(ui);
        for tool in Tool::ALL {
            let on = state.tool == tool;
            if tool_button(ui, on, tool.icon(), t(tool.key()), tool.shortcut())
                .on_hover_text(tool.hint())
                .clicked()
            {
                state.choose(tool);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            save_buttons(ui, state, requests);
        });
    });
    // Row 2: the options on the left, what always applies on the right.
    row(ui, |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            always(ui, state, actions, requests);
            divider(ui);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                tool_options(ui, state, picker, assets, actions);
            });
        });
    });
}

/// Save, save as and the pending count, from right to left; export, import and discard in the
/// menu next to them.
fn save_buttons(ui: &mut egui::Ui, state: &mut MapEditor, requests: &mut Vec<Request>) {
    let pending = state.tiles.len();
    ui.menu_button(RichText::new(icon::DOTS_THREE_VERTICAL).font(style::icon()), |ui| {
        if ui
            .button(format!("{}  {}", icon::EXPORT, t("map.export")))
            .on_hover_text(t("map.export_hint"))
            .clicked()
        {
            requests.push(Request::Export);
            ui.close_menu();
        }
        if ui
            .button(format!("{}  {}", icon::DOWNLOAD_SIMPLE, t("map.import")))
            .on_hover_text(t("map.import_hint"))
            .clicked()
        {
            requests.push(Request::Import);
            ui.close_menu();
        }
        ui.separator();
        if ui
            .add_enabled(
                pending > 0,
                egui::Button::new(
                    RichText::new(format!("{}  {}", icon::TRASH, t("map.discard")))
                        .color(style::color::DANGER),
                ),
            )
            .on_hover_text(t("map.discard_hint"))
            .clicked()
        {
            requests.push(Request::Discard);
            ui.close_menu();
        }
    });
    if ui
        .add_enabled(
            pending > 0,
            egui::Button::new(format!("{}  {}", icon::FOLDER_SIMPLE, t("map.save_as")))
                .min_size(egui::vec2(0.0, BUTTON)),
        )
        .on_hover_text(t("map.save_as_hint"))
        .on_disabled_hover_text(t("map.nothing_to_save"))
        .clicked()
    {
        requests.push(Request::SaveAs);
    }
    let save = egui::Button::new(
        RichText::new(format!("{}  {}", icon::FLOPPY_DISK, t("map.save"))).strong(),
    )
    .fill(if pending > 0 {
        style::color::ACCENT_BG
    } else {
        style::color::SURFACE_2
    })
    .min_size(egui::vec2(0.0, BUTTON));
    if ui
        .add_enabled(pending > 0, save)
        .on_hover_text(t("map.save_hint"))
        .on_disabled_hover_text(t("map.nothing_to_save"))
        .clicked()
    {
        requests.push(Request::Save);
    }
    if pending > 0 {
        widgets::chip(ui, &tf("map.pending", &[&pending]), style::color::ACCENT);
    }
}

/// What always applies, from right to left: Exit, help, go to, grid, undo and redo, and the
/// palette.
fn always(
    ui: &mut egui::Ui,
    state: &mut MapEditor,
    actions: &mut Changes,
    requests: &mut Vec<Request>,
) {
    // In a narrow window, the palette and Exit keep only their icon and go-to shrinks: room is
    // left for the tool's options.
    let compact = ui.available_width() < COMPACT_BELOW;
    let label = |key: &'static str| if compact { "" } else { t(key) };
    let exit = tool_button(
        ui,
        false,
        icon::SIGN_OUT,
        label("map.exit"),
        if compact { "" } else { "F11" },
    );
    if exit.on_hover_text(t("map.exit")).clicked() {
        requests.push(Request::Exit);
    }
    if icon_tool(ui, icon::QUESTION, t("map.shortcuts"), true).clicked() {
        state.help = !state.help;
    }
    divider(ui);
    // Go to an x, y.
    let goto = ui.add(
        egui::TextEdit::singleline(&mut state.goto)
            .hint_text("x, y")
            .desired_width(if compact { 56.0 } else { 80.0 }),
    );
    if goto.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        match parse_xy(&state.goto) {
            Some(at) => {
                state.look_at = Some(at);
                state.message = Some((tf("map.went", &[&at.0, &at.1]), false));
            }
            None => state.message = Some((t("map.bad_goto").into(), true)),
        }
    }
    divider(ui);
    widgets::toggle_chip(ui, &mut state.grid, t("map.grid")).on_hover_text(t("map.grid_hint"));
    divider(ui);
    if icon_tool(
        ui,
        icon::ARROW_U_UP_RIGHT,
        t("map.redo"),
        state.history.can_redo(),
    )
    .clicked()
    {
        actions.extend(state.redo());
    }
    if icon_tool(
        ui,
        icon::ARROW_U_UP_LEFT,
        t("map.undo"),
        state.history.can_undo(),
    )
    .clicked()
    {
        actions.extend(state.undo());
    }
    divider(ui);
    if tool_button(
        ui,
        state.palette,
        icon::SQUARES_FOUR,
        label("map.palette"),
        if compact { "" } else { "I" },
    )
    .on_hover_text(t("map.palette_hint"))
    .clicked()
    {
        state.toggle_palette();
    }
}

/// The chosen tool's options, those that fit; the rest in the "⋯".
fn tool_options(
    ui: &mut egui::Ui,
    state: &mut MapEditor,
    picker: &mut picker::Picker,
    assets: &MapAssets,
    actions: &mut Changes,
) {
    let items = options(state.tool);
    let width_id = |item: Item| egui::Id::new(("map-bar-option", item));
    // Those without a stored width yet are measured in an invisible pass (without drawing or
    // sensing clicks), so the "⋯" does not keep what fits.
    for &item in items {
        if ui.data(|d| d.get_temp::<f32>(width_id(item))).is_none() {
            let mut probe = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(("map-bar-probe", item))
                    .max_rect(ui.max_rect())
                    .layout(egui::Layout::left_to_right(egui::Align::Center))
                    .sizing_pass()
                    .invisible(),
            );
            let width = probe
                .horizontal(|ui| option(ui, item, state, picker, assets, actions))
                .response
                .rect
                .width();
            ui.data_mut(|d| d.insert_temp(width_id(item), width));
        }
    }
    let widths: Vec<f32> = items
        .iter()
        .map(|&item| {
            ui.data(|d| d.get_temp::<f32>(width_id(item)))
                .unwrap_or(item.guess())
        })
        .collect();
    // Between two options: the space, the divider and the space.
    let gap = 2.0 * GAP + DIVIDER;
    let shown = fit(&widths, ui.available_width(), gap, BUTTON + 4.0);
    // Each option's width (without the divider) is kept for the next frame, in the bar or in the
    // menu.
    let mut measured = |ui: &mut egui::Ui, item: Item| {
        let width = ui
            .horizontal(|ui| option(ui, item, state, picker, assets, actions))
            .response
            .rect
            .width();
        ui.data_mut(|d| d.insert_temp(width_id(item), width));
    };
    for (i, &item) in items[..shown].iter().enumerate() {
        if i > 0 {
            divider(ui);
        }
        measured(ui, item);
    }
    if shown < items.len() {
        if shown > 0 {
            divider(ui);
        }
        ui.menu_button(RichText::new(icon::DOTS_THREE).font(style::icon()), |ui| {
            for &item in &items[shown..] {
                measured(ui, item);
            }
        })
        .response
        .on_hover_text(t("map.more_options"));
    }
}

/// One option of the tool.
fn option(
    ui: &mut egui::Ui,
    item: Item,
    state: &mut MapEditor,
    picker: &mut picker::Picker,
    assets: &MapAssets,
    actions: &mut Changes,
) {
    match item {
        Item::Hint => {
            ui.add(
                egui::Label::new(RichText::new(state.tool.hint()).color(style::color::TEXT_DIM))
                    .truncate(),
            );
        }
        Item::Brush => {
            widgets::label(ui, t("map.brush"));
            egui::ComboBox::from_id_salt("map-brush-size")
                .selected_text(format!("{0}×{0}", state.brush.size))
                .width(64.0)
                .show_ui(ui, |ui| {
                    for size in SIZES {
                        ui.selectable_value(&mut state.brush.size, size, format!("{size}×{size}"));
                    }
                });
            let shapes = [
                (Shape::Square, icon::SQUARE, "map.square"),
                (Shape::Circle, icon::CIRCLE, "map.circle"),
                (Shape::Diamond, icon::DIAMOND, "map.diamond"),
            ];
            for (shape, glyph, key) in shapes {
                let on = state.brush.shape == shape;
                if tool_button(ui, on, glyph, "", "")
                    .on_hover_text(t(key))
                    .clicked()
                {
                    state.brush.shape = shape;
                }
            }
        }
        Item::WorkingZ => {
            // Empty is automatic: the ghost finds where to land, and painted land keeps the
            // height it has.
            let hint = if state.tool == Tool::Terrain {
                "map.terrain_z_hint"
            } else {
                "map.z_hint"
            };
            widgets::label(ui, "Z");
            match &mut state.working_z {
                Some(z) => {
                    ui.add(egui::DragValue::new(z).range(-128..=127))
                        .on_hover_text(t(hint));
                    if ui
                        .small_button(icon::X)
                        .on_hover_text(t("map.z_auto"))
                        .clicked()
                    {
                        state.working_z = None;
                    }
                }
                None => {
                    if ui
                        .add(egui::Button::new(
                            RichText::new(t("map.auto")).color(style::color::TEXT_DIM),
                        ))
                        .on_hover_text(t(hint))
                        .clicked()
                    {
                        state.working_z = Some(0);
                    }
                }
            }
        }
        Item::Hue => {
            widgets::label(ui, t("items.hue"));
            ui.add(egui::DragValue::new(&mut state.hue).range(0..=3000))
                .on_hover_text(t("map.hue_hint"));
            if let Some(hues) = assets.hues {
                if picker::hue_button(ui, hues, state.hue).clicked() {
                    picker.open_hue(
                        PICK_HUE,
                        state.hue,
                        Some((picker::Kind::Static, state.graphic)),
                    );
                }
            }
        }
        Item::Current => super::brush_card::bar_card(ui, state, assets, BUTTON),
        Item::HeightMode => {
            egui::ComboBox::from_id_salt("map-height-mode")
                .selected_text(state.ground.label())
                .width(90.0)
                .show_ui(ui, |ui| {
                    for mode in Mode::ALL {
                        ui.selectable_value(&mut state.ground, mode, mode.label());
                    }
                });
            if matches!(state.ground, Mode::Raise | Mode::Lower) {
                ui.add(egui::DragValue::new(&mut state.step).range(1..=40))
                    .on_hover_text(t("map.step"));
            }
        }
        Item::EraseAll => {
            widgets::toggle_chip(ui, &mut state.erase_all, t("map.erase_all"));
        }
        Item::Vary => {
            widgets::toggle_chip(ui, &mut state.vary, t("map.vary"))
                .on_hover_text(t("map.vary_hint"));
        }
        Item::AutoBorder => {
            widgets::toggle_chip(ui, &mut state.auto_border, t("map.auto_border"))
                .on_hover_text(t("map.auto_border_hint"));
            if state.auto_border {
                super::border_dialog::source_combo(ui, state);
            }
        }
        Item::BorderSets => {
            if tool_button(
                ui,
                state.border_dialog.open,
                icon::DIAMOND,
                t("borders.button"),
                "",
            )
            .on_hover_text(t("borders.button_hint"))
            .clicked()
            {
                state.border_dialog.open = !state.border_dialog.open;
            }
        }
        Item::RegionClip => {
            let chosen = state.region.is_some();
            if ui
                .add_enabled(chosen, egui::Button::new(t("map.copy")))
                .clicked()
            {
                state.copy(assets);
            }
            if ui
                .add_enabled(chosen, egui::Button::new(t("map.cut")))
                .clicked()
            {
                actions.extend(state.cut(assets));
            }
            let copied = state.clip.as_ref().is_some_and(|c| !c.is_empty());
            if ui
                .add_enabled(copied, egui::Button::new(t("map.paste")))
                .clicked()
            {
                state.start_paste();
            }
            if ui
                .add_enabled(chosen, egui::Button::new(t("map.move")))
                .clicked()
            {
                state.start_move(assets);
            }
            if ui
                .add_enabled(
                    chosen,
                    egui::Button::new(
                        RichText::new(t("map.delete_region")).color(style::color::DANGER),
                    ),
                )
                .clicked()
            {
                actions.extend(state.delete_region(assets));
            }
        }
        Item::RegionFill => {
            if ui
                .add_enabled(
                    state.region.is_some(),
                    egui::Button::new(format!("{}  {}", icon::PAINT_BUCKET, t("map.fill"))),
                )
                .on_hover_text(t("map.fill_hint"))
                .clicked()
            {
                actions.extend(state.fill_region(assets));
            }
        }
        Item::WithLand => {
            widgets::toggle_chip(ui, &mut state.with_land, t("map.with_land"))
                .on_hover_text(t("map.with_land_hint"));
        }
        Item::Templates => {
            if ui
                .button(format!("{}  {}", icon::STACK, t("map.templates_mode")))
                .clicked()
            {
                state.palette = true;
                state.palette_mode = PaletteMode::Templates;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_does_not_fit_goes_to_the_more_menu() {
        // Everything fits: no "⋯".
        assert_eq!(fit(&[100.0, 100.0], 300.0, 4.0, 30.0), 2);
        // It does not fit: the "⋯" needs room too.
        assert_eq!(fit(&[100.0, 100.0, 100.0], 300.0, 4.0, 30.0), 2);
        assert_eq!(fit(&[100.0, 100.0, 100.0], 230.0, 4.0, 30.0), 1);
        assert_eq!(fit(&[100.0], 50.0, 4.0, 30.0), 0);
    }

    #[test]
    fn each_tool_has_its_own_options() {
        for tool in Tool::ALL {
            let items = options(tool);
            assert!(!items.is_empty(), "{tool:?}");
            let mut seen = items.to_vec();
            seen.dedup();
            assert_eq!(seen.len(), items.len(), "{tool:?} repeats");
        }
        assert!(options(Tool::Stamp).contains(&Item::Hue));
        assert!(!options(Tool::Terrain).contains(&Item::Hue));
        assert!(options(Tool::Terrain).contains(&Item::Vary));
        assert!(options(Tool::Terrain).contains(&Item::WorkingZ));
        assert!(!options(Tool::Stamp).contains(&Item::Vary));
        assert!(options(Tool::Height).contains(&Item::HeightMode));
        // The brush card in the tools that place something.
        for tool in [Tool::Stamp, Tool::Terrain, Tool::Region] {
            assert!(options(tool).contains(&Item::Current), "{tool:?}");
        }
        assert!(options(Tool::Terrain).contains(&Item::BorderSets));
        // Only the brush tools have the brush.
        for tool in Tool::ALL {
            assert_eq!(
                options(tool).contains(&Item::Brush),
                tool.brushes(),
                "{tool:?}"
            );
        }
    }
}
