//! The map editor's UI building blocks, ported from MacroOnline's admin panel:
//! the design tokens and theme (`style`), the generic panel components
//! (`widgets`) and the visual art picker (`picker`).

pub mod picker;
pub mod style;
pub mod widgets;

use eframe::egui;

/// The name the icon font is registered under.
const ICON_FONT: &str = "phosphor";

/// Adds the Phosphor icon font to `fonts`, as a fallback of the proportional
/// family right after its first font, keeping every font already there. Does
/// nothing when it is already in. Use this when the app builds its own
/// `FontDefinitions`, before its `ctx.set_fonts`.
pub fn add_fonts(fonts: &mut egui::FontDefinitions) {
    if fonts.font_data.contains_key(ICON_FONT) {
        return;
    }
    egui_phosphor::add_to_fonts(fonts, egui_phosphor::Variant::Regular);
}

/// Adds the icon font to the context's fonts without dropping the ones it
/// already has. Call it once, at startup (e.g. in the eframe creation
/// closure) and after any `ctx.set_fonts` the app does itself: a later
/// `set_fonts` with definitions that lack the icon font removes it again.
///
/// Before the first frame the context has no fonts built yet, so it starts
/// from egui's defaults (what the app uses unless it called `set_fonts`);
/// afterwards it starts from the fonts in use.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = if ctx.cumulative_pass_nr() > 0 {
        ctx.fonts(|f| f.lock().fonts.definitions().clone())
    } else {
        egui::FontDefinitions::default()
    };
    if fonts.font_data.contains_key(ICON_FONT) {
        return;
    }
    add_fonts(&mut fonts);
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_font_is_added_once_and_keeps_the_others() {
        let mut fonts = egui::FontDefinitions::default();
        let before = fonts.font_data.len();
        let first = fonts.families[&egui::FontFamily::Proportional][0].clone();
        add_fonts(&mut fonts);
        add_fonts(&mut fonts);
        assert_eq!(fonts.font_data.len(), before + 1);
        let proportional = &fonts.families[&egui::FontFamily::Proportional];
        assert_eq!(proportional[0], first);
        assert_eq!(proportional[1], ICON_FONT);
        assert_eq!(proportional.iter().filter(|f| *f == ICON_FONT).count(), 1);
    }
}
