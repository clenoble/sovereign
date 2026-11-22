//! A release binary never trusts the builder's folder (decided by PANDA2 as
//! release owner, 2026-11-17). `AppConfig::project_root` took, first, the
//! workspace two levels above `CARGO_MANIFEST_DIR` when it held `config/` or
//! `models/`, before the executable's own folder: a release binary run on a
//! machine where the builder's path exists would load that folder's
//! `config/default.toml` and resolve a relative `model_dir` against it. That
//! step is compiled out of release builds; debug builds keep it, so dev runs
//! keep working.
//!
//! Written before the code, by another author: whoever implements it makes
//! these pass **without editing this file**; a test that looks wrong is
//! reported, not worked around.
//!
//! The tests hold the outcome through `project_root_from`, which takes whether
//! the build is a debug one as a parameter (`cfg!(debug_assertions)` cannot be
//! flipped inside one test binary); a source test holds that `project_root`
//! passes it `cfg!(debug_assertions)`. Each test builds, in a fresh temporary
//! folder (`sov-root-…`), a builder's folder (`builder/crates/sovereign-core`,
//! the manifest's) and an install folder (the executable's).

use std::path::{Path, PathBuf};

use super::project_root_from;

/// A builder's folder and an install folder, in a fresh temporary folder
/// removed when dropped.
struct Layout {
    top: PathBuf,
    builder: PathBuf,
    manifest: PathBuf,
    exe_dir: PathBuf,
}

impl Layout {
    /// `marks`: what the builder's folder holds (`"config"`, `"models"`).
    fn new(tag: &str, marks: &[&str]) -> Layout {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let top = std::env::temp_dir().join(format!("sov-root-{tag}-{}-{nanos}", std::process::id()));
        let builder = top.join("builder");
        let manifest = builder.join("crates").join("sovereign-core");
        let exe_dir = top.join("install");
        std::fs::create_dir_all(&manifest).unwrap();
        std::fs::create_dir_all(&exe_dir).unwrap();
        for mark in marks {
            std::fs::create_dir_all(builder.join(mark)).unwrap();
        }
        if marks.contains(&"config") {
            std::fs::write(builder.join("config").join("default.toml"), "# the builder's\n").unwrap();
        }
        Layout { top, builder, manifest, exe_dir }
    }
}

impl Drop for Layout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.top);
    }
}

/// Two paths name the same folder (both exist), whatever their spelling.
fn same(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn root(layout: &Layout, manifest: bool, exe: bool, debug_build: bool) -> Option<PathBuf> {
    project_root_from(
        manifest.then_some(layout.manifest.as_path()),
        exe.then_some(layout.exe_dir.as_path()),
        debug_build,
    )
}

fn assert_root(got: Option<PathBuf>, want: &Path, layout: &Layout, what: &str) {
    match got {
        Some(got) if same(&got, want) => {}
        Some(got) if same(&got, &layout.builder) => {
            panic!("{what}: the root is the builder's folder {}, not {}", got.display(), want.display())
        }
        other => panic!("{what}: the root is {other:?}, not {}", want.display()),
    }
}

// ---- A release build ----

/// A release build takes the executable's folder, even when the builder's
/// folder exists and holds `config/` (with a `default.toml`).
#[test]
fn release_build_takes_the_exe_folder_even_when_the_builders_folder_holds_config() {
    let l = Layout::new("rel-config", &["config"]);
    assert_root(root(&l, true, true, false), &l.exe_dir, &l, "release build, builder's folder with config/");
}

/// The same when the builder's folder holds `models/` only.
#[test]
fn release_build_takes_the_exe_folder_even_when_the_builders_folder_holds_models() {
    let l = Layout::new("rel-models", &["models"]);
    assert_root(root(&l, true, true, false), &l.exe_dir, &l, "release build, builder's folder with models/");
}

/// With no executable folder, a release build has no root (the caller falls
/// back to the working directory): never the builder's folder.
#[test]
fn release_build_with_no_exe_folder_never_takes_the_builders_folder() {
    let l = Layout::new("rel-noexe", &["config", "models"]);
    let got = root(&l, true, false, false);
    assert!(got.is_none(), "release build, no executable folder: the root is {got:?}, not none");
}

// ---- A debug build: today's lookup stays ----

/// A debug build takes the builder's workspace when it holds `config/`.
#[test]
fn debug_build_takes_the_workspace_when_it_holds_config() {
    let l = Layout::new("dbg-config", &["config"]);
    assert_root(root(&l, true, true, true), &l.builder, &l, "debug build, workspace with config/");
}

/// The same when it holds `models/` only.
#[test]
fn debug_build_takes_the_workspace_when_it_holds_models() {
    let l = Layout::new("dbg-models", &["models"]);
    assert_root(root(&l, true, true, true), &l.builder, &l, "debug build, workspace with models/");
}

/// A debug build whose workspace holds neither takes the executable's folder.
#[test]
fn debug_build_without_the_workspace_layout_takes_the_exe_folder() {
    let l = Layout::new("dbg-bare", &[]);
    assert_root(root(&l, true, true, true), &l.exe_dir, &l, "debug build, workspace without config/ or models/");
}

/// A debug build that does not know its manifest folder takes the
/// executable's.
#[test]
fn debug_build_with_no_manifest_folder_takes_the_exe_folder() {
    let l = Layout::new("dbg-nomanifest", &["config"]);
    assert_root(root(&l, false, true, true), &l.exe_dir, &l, "debug build, no manifest folder");
}

// ---- The call site, in the source ----

/// `text` with its `//` comments removed (a comment naming a call does not
/// make it).
fn without_comments(text: &str) -> String {
    text.lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The body of `fn <name>(` in `src`, braces balanced; with the line where
/// the function starts and the one where it ends (1-based).
fn body_of(src: &str, name: &str) -> Option<(String, usize, usize)> {
    let start = src.find(&format!("fn {name}("))?;
    let open = start + src[start..].find('{')?;
    let mut depth = 0usize;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let end = open + i + 1;
                    let first = src[..start].lines().count() + 1;
                    let last = src[..end].lines().count();
                    return Some((src[open..end].to_string(), first, last));
                }
            }
            _ => {}
        }
    }
    None
}

fn config_source() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("config.rs"))
        .expect("read src/config.rs")
}

/// `AppConfig::project_root` goes through `project_root_from`, telling it
/// whether the build is a debug one by `cfg!(debug_assertions)`, and no
/// longer looks at the workspace's layout itself.
#[test]
fn project_root_passes_the_build_kind_to_project_root_from() {
    let src = without_comments(&config_source());
    let (body, _, _) = body_of(&src, "project_root").expect("config.rs has fn project_root(");
    assert!(
        body.contains("project_root_from("),
        "project_root does not call project_root_from: the release build's root is not the one the tests hold"
    );
    assert!(
        body.contains("cfg!(debug_assertions)"),
        "project_root does not tell project_root_from the build's kind by cfg!(debug_assertions)"
    );
    assert!(
        !body.contains("is_dir()"),
        "project_root still looks at the workspace's layout itself (is_dir), besides project_root_from"
    );
}

// ---- A phone never looks for the builder's folder ----
//
// Decided by PANDA2 as release owner, 2026-11-17: a phone build (target_os
// android or ios) never looks for the builder's folder, debug or release; a
// phone's sandbox never holds it. The published APK is a debug build (Céline,
// 2026-10-17), so the debug lookup alone would reach it. A test binary runs on
// the desktop, so this is held in the source: every read of
// `env!("CARGO_MANIFEST_DIR")` in `project_root` sits under a `#[cfg(…)]`
// (on its statement, or on a block that holds it) that is false on android
// and on ios, in a debug build and in a release one. Then a phone build never
// names the builder's folder, and project_root_from cannot be passed it there.

/// What a `cfg` predicate is on one target; `None` when it names something
/// this evaluator does not know (a feature, a vendor…), which proves nothing.
#[derive(Clone, Copy)]
struct Target {
    os: &'static str,
    family: &'static str,
    debug: bool,
}

fn eval_cfg(p: &str, t: Target) -> Option<bool> {
    let p = p.trim();
    if let Some(inner) = call_args(p, "not") {
        return eval_cfg(inner, t).map(|v| !v);
    }
    if let Some(inner) = call_args(p, "all") {
        let vals: Vec<Option<bool>> = split_top(inner).iter().map(|a| eval_cfg(a, t)).collect();
        return if vals.contains(&Some(false)) {
            Some(false)
        } else if vals.iter().all(|v| *v == Some(true)) {
            Some(true)
        } else {
            None
        };
    }
    if let Some(inner) = call_args(p, "any") {
        let vals: Vec<Option<bool>> = split_top(inner).iter().map(|a| eval_cfg(a, t)).collect();
        return if vals.contains(&Some(true)) {
            Some(true)
        } else if vals.iter().all(|v| *v == Some(false)) {
            Some(false)
        } else {
            None
        };
    }
    if let Some((key, value)) = p.split_once('=') {
        let value = value.trim().trim_matches('"');
        return match key.trim() {
            "target_os" => Some(value == t.os),
            "target_family" => Some(value == t.family),
            _ => None,
        };
    }
    match p {
        "debug_assertions" => Some(t.debug),
        "unix" => Some(t.family == "unix"),
        "windows" => Some(t.family == "windows"),
        "test" => Some(false),
        _ => None,
    }
}

/// `inner` when `p` is `name(inner)`.
fn call_args<'a>(p: &'a str, name: &str) -> Option<&'a str> {
    let rest = p.strip_prefix(name)?.trim_start();
    rest.strip_prefix('(')?.strip_suffix(')')
}

/// The comma-separated items of a predicate list, at its top level.
fn split_top(s: &str) -> Vec<String> {
    let (mut items, mut cur, mut depth) = (Vec::new(), String::new(), 0i32);
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                items.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        items.push(cur);
    }
    items
}

/// The predicates of the `#[cfg(…)]` attributes that govern line `at` of
/// `lines` (comments removed): those on the statement that holds it, and those
/// on every block that encloses it, up to the start of `lines`.
fn governing_cfgs(lines: &[&str], at: usize) -> Vec<String> {
    let mut cfgs = Vec::new();
    // Attributes directly above line `from`, upward, until a non-attribute line.
    let attrs_above = |from: usize, cfgs: &mut Vec<String>| {
        let mut j = from;
        while j > 0 {
            j -= 1;
            let t = lines[j].trim();
            if t.is_empty() {
                continue;
            }
            if let Some(p) = t.strip_prefix("#[cfg(").and_then(|r| r.strip_suffix(")]")) {
                cfgs.push(p.to_string());
            } else if !t.starts_with("#[") {
                break;
            }
        }
    };
    // The statement's own start: up from `at` while the line above continues it.
    let mut start = at;
    while start > 0 {
        let t = lines[start - 1].trim();
        if t.is_empty() || t.starts_with("#[") || t.ends_with(';') || t.ends_with('{') || t.ends_with('}') {
            break;
        }
        start -= 1;
    }
    attrs_above(start, &mut cfgs);
    // An attribute on the statement's own line: `#[cfg(…)] let m = …`.
    if let Some(rest) = lines[start].trim().strip_prefix("#[cfg(") {
        if let Some(end) = rest.find(")]") {
            cfgs.push(rest[..end].to_string());
        }
    }
    // The enclosing blocks: up from the statement, past sibling blocks.
    let mut depth = 0i32;
    let mut j = start;
    while j > 0 {
        j -= 1;
        let t = lines[j];
        depth += t.matches('}').count() as i32 - t.matches('{').count() as i32;
        if depth < 0 {
            // `lines[j]` opens a block that holds the read; its statement may
            // begin a line or more above.
            let mut s = j;
            while s > 0 {
                let u = lines[s - 1].trim();
                if u.is_empty() || u.starts_with("#[") || u.ends_with(';') || u.ends_with('{') || u.ends_with('}') {
                    break;
                }
                s -= 1;
            }
            attrs_above(s, &mut cfgs);
            depth = 0;
            j = s;
        }
    }
    cfgs
}

const PHONES: [Target; 4] = [
    Target { os: "android", family: "unix", debug: true },
    Target { os: "android", family: "unix", debug: false },
    Target { os: "ios", family: "unix", debug: true },
    Target { os: "ios", family: "unix", debug: false },
];

const DESKTOPS_DEBUG: [Target; 3] = [
    Target { os: "windows", family: "windows", debug: true },
    Target { os: "linux", family: "unix", debug: true },
    Target { os: "macos", family: "unix", debug: true },
];

/// Whether the statement at `at` is compiled on `t`: `Some(false)` when one
/// of its governing cfgs is false there.
fn compiled_on(cfgs: &[String], t: Target) -> Option<bool> {
    let vals: Vec<Option<bool>> = cfgs.iter().map(|p| eval_cfg(p, t)).collect();
    if vals.contains(&Some(false)) {
        Some(false)
    } else if vals.iter().all(|v| *v == Some(true)) {
        Some(true)
    } else {
        None
    }
}

/// The lines of `project_root`'s body that read the manifest folder, each
/// with the cfgs that govern it.
fn manifest_reads_in_project_root() -> Vec<(String, Vec<String>)> {
    let src = without_comments(&config_source());
    let (body, _, _) = body_of(&src, "project_root").expect("config.rs has fn project_root(");
    let lines: Vec<&str> = body.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("env!(\"CARGO_MANIFEST_DIR\")"))
        .map(|(i, l)| (l.trim().to_string(), governing_cfgs(&lines, i)))
        .collect()
}

/// On android and on ios, debug or release, `project_root` does not read the
/// builder's folder: each read of `env!("CARGO_MANIFEST_DIR")` sits under a
/// cfg that is false there.
#[test]
fn a_phone_build_never_reads_the_builders_folder() {
    let reads = manifest_reads_in_project_root();
    assert!(
        !reads.is_empty(),
        "precondition: project_root reads the manifest folder (a desktop debug build keeps the lookup)"
    );
    for (line, cfgs) in &reads {
        for t in PHONES {
            let kind = if t.debug { "debug" } else { "release" };
            assert_eq!(
                compiled_on(cfgs, t),
                Some(false),
                "a {kind} build for {} still reads the builder's folder in project_root: `{line}` \
                 (its cfgs: {cfgs:?})",
                t.os
            );
        }
    }
}

/// A desktop debug build keeps the lookup: some read of the manifest folder
/// in `project_root` is compiled on windows, linux and macos in a debug build.
#[test]
fn a_desktop_debug_build_still_reads_the_manifest_folder() {
    let reads = manifest_reads_in_project_root();
    for t in DESKTOPS_DEBUG {
        assert!(
            reads.iter().any(|(_, cfgs)| compiled_on(cfgs, t) == Some(true)),
            "a debug build for {} no longer reads the manifest folder in project_root: dev runs lose \
             the workspace lookup (reads: {reads:?})",
            t.os
        );
    }
}

/// The evaluator itself, on predicates whose value is known.
#[test]
fn the_cfg_evaluator_reads_the_predicates_it_knows() {
    let android_debug = PHONES[0];
    let windows_debug = DESKTOPS_DEBUG[0];
    let phone_out = r#"not(any(target_os = "android", target_os = "ios"))"#;
    assert_eq!(eval_cfg(phone_out, android_debug), Some(false));
    assert_eq!(eval_cfg(phone_out, windows_debug), Some(true));
    assert_eq!(eval_cfg("debug_assertions", android_debug), Some(true));
    let both = format!("all(debug_assertions, {phone_out})");
    assert_eq!(eval_cfg(&both, android_debug), Some(false));
    assert_eq!(eval_cfg(&both, windows_debug), Some(true));
    assert_eq!(eval_cfg(r#"feature = "x""#, android_debug), None);
    let block = ["fn f() {", "    #[cfg(debug_assertions)]", "    {", "        let a = 1;", "    }", "}"];
    assert_eq!(governing_cfgs(&block, 3), vec!["debug_assertions".to_string()]);
    let stmt = ["fn f() {", "    let b = 0;", "    #[cfg(unix)]", "    let a =", "        2;", "}"];
    assert_eq!(governing_cfgs(&stmt, 4), vec!["unix".to_string()]);
    let none = ["fn f() {", "    #[cfg(unix)]", "    let b = 0;", "    let a = 2;", "}"];
    assert!(governing_cfgs(&none, 3).is_empty(), "a sibling statement's cfg does not govern");
}

// ---- No other shipped code reads the builder's folder ----

/// Every `.rs` file under `dir`, but in `tests/`, `benches/`, `examples/`
/// and `target/` folders.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            if !matches!(name.as_str(), "tests" | "benches" | "examples" | "target" | "node_modules") {
                rust_files(&p, out);
            }
        } else if name.ends_with(".rs") {
            out.push(p);
        }
    }
}

/// The lines of `src` that shipped code reads `env!("CARGO_MANIFEST_DIR")`
/// on (1-based), comments removed: none in a test file (`*_tests.rs`,
/// `tests.rs`), none after a `#[cfg(test)]` that opens a `mod … {`.
fn shipped_reads(path: &Path, src: &str) -> Vec<usize> {
    let name = path.file_name().unwrap().to_string_lossy();
    if name.ends_with("_tests.rs") || name == "tests.rs" {
        return Vec::new();
    }
    let lines: Vec<&str> = src.lines().collect();
    let mut hits = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t.starts_with("#[cfg(test)]") {
            let next = lines[i + 1..].iter().map(|l| l.trim_start()).find(|l| !l.is_empty() && !l.starts_with("#["));
            if next.is_some_and(|n| (n.starts_with("mod ") || n.starts_with("pub mod ")) && n.ends_with('{')) {
                break;
            }
        }
        let code = match line.find("//") {
            Some(k) => &line[..k],
            None => line,
        };
        if code.contains("env!(\"CARGO_MANIFEST_DIR\")") {
            hits.push(i + 1);
        }
    }
    hits
}

/// No shipped code in the workspace's crates reads the builder's folder
/// (`env!("CARGO_MANIFEST_DIR")`), but `project_root` in sovereign-core's
/// `config.rs`, whose release build no longer trusts it (the tests above).
#[test]
fn no_other_shipped_code_reads_the_builders_folder() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("crates/").to_path_buf();
    let mut files = Vec::new();
    rust_files(&crates, &mut files);
    assert!(files.len() > 100, "precondition: the scan finds the workspace's sources ({} files)", files.len());
    let config = crates.join("sovereign-core").join("src").join("config.rs");
    let (_, first, last) = body_of(&config_source(), "project_root").expect("config.rs has fn project_root(");
    let mut found = Vec::new();
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap_or_default();
        for line in shipped_reads(f, &src) {
            let allowed = same(f, &config) && (first..=last).contains(&line);
            if !allowed {
                found.push(format!("{}:{line}", f.display()));
            }
        }
    }
    assert!(found.is_empty(), "shipped code reads the builder's folder:\n{}", found.join("\n"));
}
