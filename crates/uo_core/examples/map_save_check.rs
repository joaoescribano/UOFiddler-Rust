//! Edits a copy of a real facet, saves it and checks every other block is unchanged:
//! `cargo run -p uo_core --example map_save_check -- <uo dir> <scratch dir> <map id>`.
use std::path::PathBuf;
use uo_core::map_edit::{read_block, save, SaveOptions};
use uo_core::{map_files, MapEdits, MapManager, MapTile, PlacedStatic, RemovedStatic, MAP_DEFS};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dir, out, id) = (
        PathBuf::from(&a[1]),
        PathBuf::from(&a[2]),
        a[3].parse::<usize>().unwrap(),
    );
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
    std::fs::create_dir_all(&out).unwrap();
    let copy = |p: Option<PathBuf>| {
        let p = p.unwrap();
        let to = out.join(p.file_name().unwrap());
        std::fs::copy(&p, &to).unwrap();
        Some(to)
    };
    let (m, i, s) = (copy(m), copy(i), copy(s));
    let mut orig = MapManager::new();
    orig.load_map(id, m.clone(), i.clone(), s.clone());
    let mut maps = MapManager::new();
    maps.load_map(id, m, i, s);

    let (w, h) = (maps.actual_widths[id] / 8, MAP_DEFS[id].height / 8);
    // Find a block with statics to take one away.
    let (mut tx, mut ty, mut taken) = (0, 0, None);
    'f: for bx in w / 2..w {
        for by in h / 2..h {
            let b = read_block(&maps, id, bx, by).unwrap();
            if let Some(st) = b.statics.first() {
                tx = (bx * 8 + st.x as u32) as i32;
                ty = (by * 8 + st.y as u32) as i32;
                taken = Some(*st);
                break 'f;
            }
        }
    }
    let st = taken.unwrap();
    let mut edits = MapEdits::new();
    let mut t = MapTile::fresh(tx, ty);
    t.remove.push(RemovedStatic {
        graphic: st.graphic,
        z: st.z,
    });
    t.add.push(PlacedStatic {
        graphic: 0x0EED,
        z: 0,
        hue: 0,
    });
    t.land = Some(0x0003);
    edits.set(t);
    let r = save(
        &mut maps,
        id,
        &edits,
        &SaveOptions {
            target_dir: None,
            backup: false,
        },
    )
    .unwrap();
    println!("saved {:?}", r);
    let changed = ((tx / 8) as u32, (ty / 8) as u32);
    let mut diffs = 0;
    for bx in 0..w {
        for by in 0..h {
            let (a, b) = (read_block(&orig, id, bx, by), read_block(&maps, id, bx, by));
            if (bx, by) == changed {
                let b = b.unwrap();
                assert_eq!(b.cell(tx, ty).graphic, 3);
                assert!(b.statics_at(tx, ty).any(|s| s.graphic == 0x0EED));
                continue;
            }
            if a != b {
                diffs += 1;
            }
        }
    }
    println!("blocks {} x {}, other blocks differing: {}", w, h, diffs);
    assert_eq!(diffs, 0);
}
