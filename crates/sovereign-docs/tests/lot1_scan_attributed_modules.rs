//! The shared source scan (`tests/common/scan.rs`) and test-only modules
//! declared with attributes between the `cfg` and the `mod` (2026-10-24).
//!
//! The shell declares test modules as `#[cfg(test)] #[path = "…"] mod …;`
//! (`editor_default_tests`, `lot1_tests`); the app as
//! `#[cfg(all(test, feature = "encryption"))] mod …;`. The scan used to read
//! such files as production code, so a store call inside a test tripped the
//! S5-10 guard (`s5_one_way_to_delete`, found by the docs implementer of
//! v0.0.12 lot 1). These cases keep the fix, and keep the scan catching a
//! production call written right after such a declaration.

mod common;

use std::path::PathBuf;

use common::scan::{calls, production, production_files};

const SAMPLE: &str = r#####"
#[cfg(test)]
#[path = "editor_default_tests.rs"]
mod editor_default_tests;
#[cfg(test)]
#[allow(dead_code)]
#[path = "lot1_tests.rs"]
mod lot1_tests;
#[cfg(all(test, feature = "encryption"))]
mod lot1_onboarding_tests;
#[cfg(all(test, feature = "encryption"))]
fn test_helper(db: &dyn GraphDB) { db.soft_delete_thread(&id); }
fn production_after(db: &dyn GraphDB) {
    db.soft_delete_thread(&id);                   // line 14: a call
}
"#####;

#[test]
fn scan_reads_test_modules_declared_with_attributes() {
    let (prod, mods) = production(SAMPLE);
    assert_eq!(
        mods,
        vec!["editor_default_tests.rs".to_string(), "lot1_tests.rs".to_string(), "lot1_onboarding_tests".to_string()],
        "a #[path] module is named by its path, the others by their name"
    );
    assert_eq!(calls(&prod, "soft_delete_thread"), vec![14], "the test-only helper is skipped, the production call is not");
}

#[test]
fn scan_leaves_out_the_shells_path_declared_test_files() {
    let shell = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("sovereign-shell");
    let files: Vec<PathBuf> = production_files(&shell).into_iter().map(|(f, _)| f).collect();
    assert!(files.iter().any(|f| f.ends_with("app.rs")), "the scan still reads app.rs");
    for name in ["lot1_tests.rs", "editor_default_tests.rs"] {
        assert!(!files.iter().any(|f| f.ends_with(name)), "{name} is a test module, not production code");
    }
}
