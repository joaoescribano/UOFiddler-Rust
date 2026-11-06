//! `UOFIDDLER_SCREENSHOT=<file.png>[,<seconds>]`: saves a capture of the window after a few
//! seconds and quits (used to check the map view without a person at the screen).

use eframe::egui;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

fn start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

pub fn tick(ctx: &egui::Context) {
    let Ok(spec) = std::env::var("UOFIDDLER_SCREENSHOT") else {
        return;
    };
    let mut parts = spec.split(',');
    let path = parts.next().unwrap_or("capture.png").to_string();
    let delay = parts
        .next()
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(5.0);
    let elapsed = start().elapsed();
    ctx.request_repaint_after(Duration::from_millis(100));
    let shot = ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        })
    });
    if let Some(image) = shot {
        let [w, h] = image.size;
        let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
        if let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, rgba) {
            let _ = img.save(&path);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        return;
    }
    static ASKED: OnceLock<()> = OnceLock::new();
    if elapsed.as_secs_f32() >= delay && ASKED.set(()).is_ok() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
    }
}
