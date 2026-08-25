//! Re-reads every watched folder and rebuilds the index from the files.
//!
//! The database is only a cache of what the tags say, so this restores it from
//! the source of truth at any time.
//!
//! Usage: `cargo run --example rescan`

use std::path::PathBuf;

use pick_n_mix_lib::library::db::Db;
use pick_n_mix_lib::library::scan;

fn main() -> anyhow::Result<()> {
    let data_dir = PathBuf::from(std::env::var("HOME")?)
        .join("Library/Application Support/com.picknmix.app");
    let artwork = data_dir.join("artwork");
    let db = Db::open(&data_dir.join("library.db"))?;

    let folders = db.folders()?;
    if folders.is_empty() {
        println!("no watched folders");
        return Ok(());
    }
    println!("rescanning: {folders:?}");

    let report = scan::scan_folders(&db, &artwork, &folders, |_, _| {})?;
    println!(
        "scanned {}, added {}, updated {}, skipped {}",
        report.scanned, report.added, report.updated, report.skipped
    );
    for error in report.errors.iter().take(10) {
        eprintln!("  {error}");
    }
    println!("library holds {} tracks", db.track_count()?);
    Ok(())
}
