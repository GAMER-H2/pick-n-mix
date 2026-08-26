//! Populates the app's real library with a folder of music, for manual testing.
//!
//! Usage: `cargo run --example seed -- /path/to/music`

use std::path::PathBuf;

use pick_n_mix_lib::library::db::Db;
use pick_n_mix_lib::library::scan;
use pick_n_mix_lib::playlist::{self, Playlist};

fn main() -> anyhow::Result<()> {
    let music = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: seed <music folder>"))?;

    // Matches Tauri's app_data_dir on macOS.
    let data_dir =
        PathBuf::from(std::env::var("HOME")?).join("Library/Application Support/com.picknmix.app");
    let artwork = data_dir.join("artwork");
    let playlists = data_dir.join("playlists");
    for dir in [&data_dir, &artwork, &playlists] {
        std::fs::create_dir_all(dir)?;
    }

    let db = Db::open(&data_dir.join("library.db"))?;
    db.add_folder(&music)?;

    let report = scan::scan_folders(&db, &artwork, &[music.clone()], |n, path| {
        println!("  [{n}] {path}");
    })?;
    println!(
        "scanned {}, added {}, updated {}, skipped {}",
        report.scanned, report.added, report.updated, report.skipped
    );
    for error in &report.errors {
        eprintln!("  error: {error}");
    }

    let tracks = db.all_tracks()?;
    let mut list = Playlist {
        name: "Late Night Drive".into(),
        description: "Test playlist seeded for manual checks".into(),
        ..Default::default()
    };
    for track in &tracks {
        list.add_track(track);
    }
    let path = playlists.join(playlist::file_name_for(&list.name, &list.id));
    list.save(&path)?;

    println!("library now holds {} tracks", db.track_count()?);
    println!("wrote playlist {}", path.display());
    Ok(())
}
