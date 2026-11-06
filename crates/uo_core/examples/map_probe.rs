//! Prints what the map reader finds for each facet: `cargo run -p uo_core --example map_probe -- <uo dir>`.
use std::path::PathBuf;
use uo_core::{map_files, MapManager, MAP_DEFS};

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("uo dir"));
    let entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    let find = |n: &str| {
        entries
            .iter()
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(n)
            })
            .cloned()
    };
    let mut mm = MapManager::new();
    for id in 0..6 {
        let (m, i, s) = map_files(id, find);
        mm.load_map(id, m.clone(), i, s);
        let land = mm.land_file(id);
        println!(
            "{} {:?}: uop={:?} blocks={:?} width={} expected={}x{}",
            id,
            m.map(|p| p.file_name().unwrap().to_owned()),
            land.map(|l| l.is_uop()),
            land.map(|l| l.block_count()),
            mm.actual_widths[id],
            MAP_DEFS[id].width,
            MAP_DEFS[id].height
        );
        if let Some(b) = mm.land_block(id, 1496 / 8, 1624 / 8, false) {
            println!(
                "   britain block first cell: tile={:#06x} z={}",
                u16::from_le_bytes([b[0], b[1]]),
                b[2] as i8
            );
        }
    }
}
