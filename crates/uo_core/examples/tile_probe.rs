//! Prints land and statics of a few tiles: `cargo run -p uo_core --example tile_probe -- <uo dir> <map> <x> <y> [radius]`.
use std::path::PathBuf;
use uo_core::map_edit::read_block;
use uo_core::{map_files, MapManager};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(&a[1]);
    let (id, x, y): (usize, i32, i32) = (
        a[2].parse().unwrap(),
        a[3].parse().unwrap(),
        a[4].parse().unwrap(),
    );
    let r: i32 = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);
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
    let (m, i, s) = map_files(id, find);
    let mut mm = MapManager::new();
    mm.load_map(id, m, i, s);
    for ty in y - r..=y + r {
        for tx in x - r..=x + r {
            let b = read_block(&mm, id, (tx / 8) as u32, (ty / 8) as u32).unwrap();
            let c = b.cell(tx, ty);
            let st: Vec<String> = b
                .statics_at(tx, ty)
                .map(|s| format!("{:#06x}@{}", s.graphic, s.z))
                .collect();
            println!(
                "{},{} land {:#06x} z {} statics {:?}",
                tx, ty, c.graphic, c.z, st
            );
        }
    }
}
