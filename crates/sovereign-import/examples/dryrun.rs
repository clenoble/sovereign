//! Dry-run the bulk-import planner on a folder: prints exactly what WOULD be
//! imported (tiers, folder→lane mapping, what's skipped and why) and **writes
//! nothing**. This runs the same `plan()` + `render_manifest()` the real
//! `sovereign-tauri import --dir <path>` produces (minus `--execute`), but
//! builds only the lightweight import crate — no Tauri, no auth, no database.
//!
//! Run:
//!   cargo run -p sovereign-import --example dryrun -- "C:\path\to\corpus"
//!   cargo run -p sovereign-import --example dryrun -- "C:\path" --single-thread Archive

use std::path::PathBuf;

use sovereign_import::{plan, render_manifest, ImportOptions, ThreadMode};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut dir: Option<PathBuf> = None;
    let mut single_thread: Option<String> = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dir" => dir = args.next().map(PathBuf::from),
            "--single-thread" => single_thread = args.next(),
            other if !other.starts_with("--") && dir.is_none() => {
                dir = Some(PathBuf::from(other));
            }
            other => eprintln!("(ignoring unknown arg: {other})"),
        }
    }

    let Some(dir) = dir else {
        eprintln!("usage: dryrun <folder> [--single-thread <name>]");
        std::process::exit(2);
    };
    if !dir.is_dir() {
        eprintln!("not a folder: {}", dir.display());
        std::process::exit(2);
    }

    let mut opts = ImportOptions::default();
    if let Some(name) = single_thread {
        opts.thread_mode = ThreadMode::SingleThread(name);
    }

    match plan(&dir, &opts) {
        Ok(manifest) => {
            print!("{}", render_manifest(&manifest));
            println!(
                "\nDry-run only — nothing was written. When the plan looks right, land it \
                 (encrypted) with the real CLI:\n  sovereign-tauri import --dir \"{}\" --execute",
                dir.display()
            );
        }
        Err(e) => {
            eprintln!("import plan failed: {e}");
            std::process::exit(1);
        }
    }
}
