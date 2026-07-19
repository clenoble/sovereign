//! List the same-title collisions the importer's idempotency guard would skip.
//!
//! The execute() guard skips any imported file whose (thread, title) already
//! landed — where title = file stem (no extension). That correctly collapses
//! format-exports (Charter.md / .pdf / .docx → one doc) but is content-blind,
//! so two genuinely different files sharing a stem in one folder also collapse.
//! This reproduces the guard read-only (via plan(), no DB, no auth) so the
//! skipped set is auditable rather than a silent count.
//!
//!   cargo run -p sovereign-import --example collisions -- "<dir>"

use std::collections::BTreeMap;

use sovereign_import::{plan, ImportOptions, PlannedFile};

fn main() {
    let dir = std::env::args().nth(1).expect("usage: collisions <dir>");
    let m = plan(std::path::Path::new(&dir), &ImportOptions::default()).expect("plan failed");

    // Group imported files by (thread, title), preserving manifest order so the
    // FIRST is the one execute() lands and the rest are what it skips.
    let mut groups: BTreeMap<(String, String), Vec<&PlannedFile>> = BTreeMap::new();
    for f in m.files.iter().filter(|f| f.disposition.is_import()) {
        let thread = f.thread.clone().unwrap_or_default();
        groups.entry((thread, f.title.clone())).or_default().push(f);
    }

    let mut group_n = 0;
    let mut total_skipped = 0;
    for ((thread, title), files) in &groups {
        if files.len() > 1 {
            group_n += 1;
            total_skipped += files.len() - 1;
            let exts: Vec<&str> = files.iter().map(|f| f.ext.as_str()).collect();
            println!(
                "\n[{group_n}] lane='{thread}'  title='{title}'  ({} files: {})",
                files.len(),
                exts.join(", ")
            );
            for (i, f) in files.iter().enumerate() {
                println!(
                    "    {}  {}",
                    if i == 0 { "LANDS" } else { "SKIP " },
                    f.rel_path.display()
                );
            }
        }
    }

    println!("\n=== {group_n} collision groups → {total_skipped} files skipped as same-title ===");
    println!("(LANDS = kept; SKIP = dropped as \"already present\". Different extensions in a group");
    println!(" that are the SAME document exported to multiple formats = benign dedup; a group of");
    println!(" genuinely DIFFERENT documents sharing a stem = a real drop worth a closer look.)");
}
