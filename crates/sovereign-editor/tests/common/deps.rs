//! Static checks on the editor crate: its dependency tree (from `cargo
//! metadata`) and its own source.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Names of every package reachable from `sovereign-editor` through normal and
/// build dependencies (dev-dependencies excluded at every level), on every
/// target platform.
pub fn reachable_packages() -> BTreeSet<String> {
    reachable_from("sovereign-editor")
}

fn metadata() -> &'static serde_json::Value {
    static META: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    META.get_or_init(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let out = Command::new(cargo)
            .args(["metadata", "--format-version", "1", "--locked", "--offline"])
            .current_dir(manifest_dir())
            .output()
            .expect("run cargo metadata");
        assert!(out.status.success(), "cargo metadata failed: {}", String::from_utf8_lossy(&out.stderr));
        serde_json::from_slice(&out.stdout).expect("metadata JSON")
    })
}

/// The same walk from any workspace package (used to show the walk is not
/// vacuous).
pub fn reachable_from(root_name: &str) -> BTreeSet<String> {
    reachable_from_avoiding(root_name, &[])
}

/// The same walk, never entering a package named in `avoid`: what is still
/// reachable is reachable by a path that does not go through them.
pub fn reachable_from_avoiding(root_name: &str, avoid: &[&str]) -> BTreeSet<String> {
    let meta = metadata();

    let mut name_of: HashMap<String, String> = HashMap::new();
    for p in meta["packages"].as_array().unwrap() {
        name_of.insert(p["id"].as_str().unwrap().into(), p["name"].as_str().unwrap().into());
    }
    let root = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == root_name)
        .expect("package in the workspace")["id"]
        .as_str()
        .unwrap()
        .to_string();

    let mut edges: HashMap<String, Vec<String>> = HashMap::new();
    for n in meta["resolve"]["nodes"].as_array().expect("resolve graph") {
        let id = n["id"].as_str().unwrap().to_string();
        let mut to = vec![];
        for d in n["deps"].as_array().unwrap() {
            let kinds = d["dep_kinds"].as_array().unwrap();
            let non_dev = kinds.iter().any(|k| k["kind"].is_null() || k["kind"] == "build");
            if non_dev {
                to.push(d["pkg"].as_str().unwrap().to_string());
            }
        }
        edges.insert(id, to);
    }

    let mut seen = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut q = VecDeque::from([root]);
    while let Some(id) = q.pop_front() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if avoid.contains(&name_of[&id].as_str()) {
            continue;
        }
        names.insert(name_of[&id].clone());
        for next in edges.get(&id).into_iter().flatten() {
            q.push_back(next.clone());
        }
    }
    names
}

pub fn matches_any(name: &str, forbidden: &[&str]) -> bool {
    forbidden.iter().any(|f| match f.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == *f,
    })
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The editor crate's own source (`src/` and `build.rs`), comments removed.
pub fn editor_source() -> Vec<(PathBuf, String)> {
    let mut files = vec![];
    rs_files(&manifest_dir().join("src"), &mut files);
    let build = manifest_dir().join("build.rs");
    if build.exists() {
        files.push(build);
    }
    files
        .into_iter()
        .map(|p| {
            let raw = std::fs::read_to_string(&p).unwrap();
            (p, strip_comments(&raw))
        })
        .collect()
}

/// Removes `//…` line comments and `/*…*/` block comments (naively: good
/// enough to keep doc prose out of a pattern scan).
pub fn strip_comments(s: &str) -> String {
    let mut out = String::new();
    let mut in_block = 0usize;
    for line in s.lines() {
        let mut l = line.to_string();
        loop {
            if in_block > 0 {
                match l.find("*/") {
                    Some(i) => {
                        l = l[i + 2..].to_string();
                        in_block -= 1;
                    }
                    None => {
                        l.clear();
                        break;
                    }
                }
            } else if let Some(i) = l.find("/*") {
                let rest = l[i + 2..].to_string();
                l.truncate(i);
                in_block += 1;
                match rest.find("*/") {
                    Some(j) => {
                        l.push_str(&rest[j + 2..]);
                        in_block -= 1;
                    }
                    None => break,
                }
            } else {
                break;
            }
        }
        if let Some(i) = l.find("//") {
            l.truncate(i);
        }
        out.push_str(&l);
        out.push('\n');
    }
    out
}

/// Every (file, pattern) where the source uses a forbidden pattern.
pub fn source_uses(patterns: &[&str]) -> Vec<String> {
    let mut hits = vec![];
    for (p, src) in editor_source() {
        for pat in patterns {
            if src.contains(pat) {
                hits.push(format!("{}: {pat}", p.display()));
            }
        }
    }
    hits
}
