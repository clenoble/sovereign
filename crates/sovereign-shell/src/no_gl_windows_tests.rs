//! v0.0.12, no OpenGL backend on Windows (`doc/plans/V0012-no-gl-windows.md`),
//! guarantees W1–W4 (W3b included) and W6. W5 (no adapter is reported as today) and the
//! manual checks (the app starts and draws, the start time) are not tested
//! here: they need a GPU, or the lack of one.
//!
//! Written before the code, by another author: whoever builds this item makes
//! these pass **without editing this file**; a test that looks wrong is
//! reported, not worked around. A child module of `app` (declared in
//! `app.rs`), as the other v0.0.12 test files are. Nothing here creates a wgpu
//! instance or touches a GPU.
//!
//! **Seam this file needs (it does not compile until it exists):** a module
//! `crate::gpu`, in `crates/sovereign-shell/src/gpu.rs`, declared `mod gpu;`
//! in `main.rs`, with exactly:
//!
//! ```ignore
//! #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! pub(crate) enum Platform { Windows, Other }
//!
//! /// The backends the shell asks wgpu for (W1–W3). Pure: `wgpu_backend_env`
//! /// is the value of `WGPU_BACKEND` (`None` when it is not set).
//! pub(crate) fn shell_backends(platform: Platform, wgpu_backend_env: Option<&str>) -> vello::wgpu::Backends;
//!
//! /// `Platform::Windows` under `cfg(windows)`, `Platform::Other` otherwise.
//! pub(crate) fn current_platform() -> Platform;
//!
//! /// The descriptor the shell creates its instance with: what
//! /// `vello::util::RenderContext::new()` (vello 0.9.0, `src/util.rs`) builds,
//! /// with `backends` replaced by `shell_backends(platform, wgpu_backend_env)`.
//! pub(crate) fn instance_descriptor(platform: Platform, wgpu_backend_env: Option<&str>) -> vello::wgpu::InstanceDescriptor;
//!
//! /// The one place the shell builds its `RenderContext` (W4):
//! /// `RenderContext { instance: Instance::new(instance_descriptor(current_platform(),
//! /// <WGPU_BACKEND read with std::env::var>)), devices: Vec::new() }`.
//! pub(crate) fn render_context() -> vello::util::RenderContext;
//! ```
//!
//! `wgpu` is not a direct dependency of the shell; it is reached as
//! `vello::wgpu`, as `app.rs` already does. Both former call sites of
//! `RenderContext::new()` (`App::new`'s `context:` field and `run_shot`) call
//! `render_context()`, and `App::new` keeps holding `TEST_WGPU_INSTANCE_INIT`
//! around that call (W6).

use std::path::{Path, PathBuf};

use vello::wgpu;

use crate::gpu::{current_platform, instance_descriptor, shell_backends, Platform};

/// Values of `WGPU_BACKEND` a developer can set. Each must decide on both
/// platforms (W3), parsed exactly as wgpu parses it. Empty and unknown values
/// are in `ODD_VALUES` below (W3b).
const SET_VALUES: &[&str] = &["gl", "dx12,vulkan", "vulkan", "dx12", "GL", " dx12 , gl ", "vk", "d3d12,opengl", "gles,vulkan"];

// ======================================================================= W1–W3, the pure function

/// W1: on Windows, with `WGPU_BACKEND` not set, exactly DX12 and Vulkan.
#[test]
fn no_gl_windows_unset_gives_exactly_dx12_and_vulkan() {
    let b = shell_backends(Platform::Windows, None);
    assert_eq!(b, wgpu::Backends::DX12 | wgpu::Backends::VULKAN, "Windows, WGPU_BACKEND unset: {b:?}");
    assert!(!b.contains(wgpu::Backends::GL), "Windows must not ask for GL by default: {b:?}");
}

/// W2: elsewhere, with `WGPU_BACKEND` not set, what `RenderContext::new()`
/// asks for today: `Backends::from_env().unwrap_or_default()` with the
/// variable unset, i.e. `Backends::default()`, which is every backend, GL
/// included.
#[test]
fn no_gl_other_platform_unset_is_unchanged() {
    let b = shell_backends(Platform::Other, None);
    assert_eq!(b, wgpu::Backends::default(), "another platform, WGPU_BACKEND unset: {b:?}");
    // What `default()` is in wgpu 29, said so the pin is readable.
    assert_eq!(wgpu::Backends::default(), wgpu::Backends::all());
    assert!(b.contains(wgpu::Backends::GL), "Linux keeps GL: {b:?}");
}

/// W3: a set `WGPU_BACKEND` decides, on Windows too, parsed by wgpu's own
/// parser (`Backends::from_comma_list`, what `Backends::from_env` calls).
#[test]
fn no_gl_set_variable_decides_on_windows() {
    for &v in SET_VALUES {
        assert_eq!(
            shell_backends(Platform::Windows, Some(v)),
            wgpu::Backends::from_comma_list(v),
            "Windows, WGPU_BACKEND={v:?}"
        );
    }
    // The override that brings GL back on a machine with neither DX12 nor Vulkan.
    assert_eq!(shell_backends(Platform::Windows, Some("gl")), wgpu::Backends::GL);
}

/// W3: a set `WGPU_BACKEND` decides on the other platforms, as today.
#[test]
fn no_gl_set_variable_decides_elsewhere() {
    for &v in SET_VALUES {
        assert_eq!(
            shell_backends(Platform::Other, Some(v)),
            wgpu::Backends::from_comma_list(v),
            "another platform, WGPU_BACKEND={v:?}"
        );
    }
    assert_eq!(shell_backends(Platform::Other, Some("vulkan")), wgpu::Backends::VULKAN);
}

/// Set values that are empty, blank, or name something wgpu does not know
/// (W3b): they decide too.
const ODD_VALUES: &[&str] = &["", " ", "nonsense", "gl,nonsense", ",", "dx11"];

/// W3b (contract commit 8e445e9): a set `WGPU_BACKEND` decides even when it is
/// empty or unknown, parsed by `Backends::from_comma_list` (so, for an empty
/// value, no backend at all), on Windows as elsewhere. No fallback to the
/// platform default: that is what wgpu 29's `Backends::from_env` does today.
#[test]
fn no_gl_empty_or_unknown_variable_decides_too() {
    for platform in [Platform::Windows, Platform::Other] {
        for &v in ODD_VALUES {
            assert_eq!(
                shell_backends(platform, Some(v)),
                wgpu::Backends::from_comma_list(v),
                "{platform:?}, WGPU_BACKEND={v:?}"
            );
        }
        // An empty value is no backend, not the platform's default.
        assert_eq!(shell_backends(platform, Some("")), wgpu::Backends::empty(), "{platform:?}, WGPU_BACKEND=\"\"");
        // Unknown names are skipped, known ones kept.
        assert_eq!(shell_backends(platform, Some("gl,nonsense")), wgpu::Backends::GL, "{platform:?}, gl,nonsense");
    }
    assert_ne!(
        shell_backends(Platform::Windows, Some("")),
        wgpu::Backends::DX12 | wgpu::Backends::VULKAN,
        "Windows, WGPU_BACKEND=\"\" must not fall back to DX12 | VULKAN"
    );
}

/// `current_platform()` follows the target the shell is built for.
#[test]
fn no_gl_current_platform_follows_the_target() {
    let expected = if cfg!(windows) { Platform::Windows } else { Platform::Other };
    assert_eq!(current_platform(), expected);
}

// ======================================================================= W1, W2: the rest of the descriptor

/// What `vello::util::RenderContext::new()` (vello 0.9.0, `src/util.rs:32-44`)
/// passes to `Instance::new` today, with `backends` given.
fn todays_descriptor(backends: wgpu::Backends) -> wgpu::InstanceDescriptor {
    wgpu::InstanceDescriptor {
        display: None,
        backends,
        flags: wgpu::InstanceFlags::from_build_config().with_env(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::from_env_or_default(),
    }
}

/// W1 and W2: the descriptor's backends are `shell_backends`' answer, and
/// every other field is exactly today's (flags from build config with env,
/// default memory thresholds, backend options from env or default, no
/// display). Compared through `Debug`, since the descriptor and some of its
/// fields have no `PartialEq`.
#[test]
fn no_gl_descriptor_is_todays_but_for_the_backends() {
    let cases: Vec<(Platform, Option<&str>)> = vec![
        (Platform::Windows, None),
        (Platform::Other, None),
        (Platform::Windows, Some("gl")),
        (Platform::Other, Some("dx12,vulkan")),
    ];
    for (platform, env) in cases {
        let d = instance_descriptor(platform, env);
        assert_eq!(d.backends, shell_backends(platform, env), "{platform:?}, {env:?}: backends");
        assert_eq!(d.flags, wgpu::InstanceFlags::from_build_config().with_env(), "{platform:?}, {env:?}: flags");
        assert!(d.display.is_none(), "{platform:?}, {env:?}: display must be None");
        assert_eq!(
            format!("{:?}", d.memory_budget_thresholds),
            format!("{:?}", wgpu::MemoryBudgetThresholds::default()),
            "{platform:?}, {env:?}: memory_budget_thresholds"
        );
        assert_eq!(
            format!("{:?}", d.backend_options),
            format!("{:?}", wgpu::BackendOptions::from_env_or_default()),
            "{platform:?}, {env:?}: backend_options"
        );
        assert_eq!(
            format!("{d:?}"),
            format!("{:?}", todays_descriptor(shell_backends(platform, env))),
            "{platform:?}, {env:?}: the whole descriptor"
        );
    }
}

// ======================================================================= source scans (W4, W6)

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
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

/// The files under `src/` that are compiled only in tests: every `*_tests.rs`,
/// and every file a `#[cfg(test)]` declaration (`mod x;`, with or without a
/// `#[path = "…"]`) brings in.
fn test_only_files(all: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = all
        .iter()
        .filter(|p| p.file_name().unwrap().to_string_lossy().ends_with("_tests.rs"))
        .cloned()
        .collect();
    for f in all {
        let src = std::fs::read_to_string(f).unwrap();
        let lines: Vec<&str> = src.lines().collect();
        let dir = f.parent().unwrap();
        // A module file `foo.rs` declares its children in `foo/`.
        let child_dir = match f.file_stem().unwrap().to_str().unwrap() {
            "main" | "lib" | "mod" => dir.to_path_buf(),
            stem => dir.join(stem),
        };
        let mut i = 0;
        while i < lines.len() {
            if lines[i].trim() == "#[cfg(test)]" {
                let mut path_attr: Option<String> = None;
                let mut j = i + 1;
                while j < lines.len() && lines[j].trim().starts_with("#[") {
                    let t = lines[j].trim();
                    if let Some(rest) = t.strip_prefix("#[path = \"") {
                        path_attr = rest.split('"').next().map(str::to_string);
                    }
                    j += 1;
                }
                if j < lines.len() {
                    let t = lines[j].trim();
                    if let Some(name) = t.strip_prefix("mod ").and_then(|r| r.strip_suffix(';')) {
                        let p = match path_attr {
                            Some(rel) => dir.join(rel),
                            None => child_dir.join(format!("{}.rs", name.trim())),
                        };
                        out.push(p);
                    }
                }
                i = j;
            } else {
                i += 1;
            }
        }
    }
    out
}

/// `src` without its inline `#[cfg(test)] mod x { … }` blocks (the block ends
/// at the first later line that is the `mod` line's indentation followed by
/// `}`, as rustfmt writes it) and without `//` comments.
fn production_code(src: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() == "#[cfg(test)]" {
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim().starts_with("#[") {
                j += 1;
            }
            if j < lines.len() {
                let t = lines[j].trim();
                if t.starts_with("mod ") && t.ends_with('{') {
                    let indent = &lines[j][..lines[j].len() - lines[j].trim_start().len()];
                    let close = format!("{indent}}}");
                    let mut k = j + 1;
                    while k < lines.len() && lines[k].trim_end() != close {
                        k += 1;
                    }
                    i = k + 1;
                    continue;
                }
            }
        }
        out.push_str(lines[i].split("//").next().unwrap());
        out.push('\n');
        i += 1;
    }
    out
}

/// Every production source file of the shell, as (path relative to `src/`,
/// production code).
fn production_sources() -> Vec<(String, String)> {
    let mut all = Vec::new();
    rs_files(&src_dir(), &mut all);
    let tests: Vec<PathBuf> = test_only_files(&all).iter().map(|p| canon(p)).collect();
    let mut out = Vec::new();
    for f in &all {
        if tests.contains(&canon(f)) {
            continue;
        }
        let rel = f.strip_prefix(src_dir()).unwrap().to_string_lossy().replace('\\', "/");
        out.push((rel, production_code(&std::fs::read_to_string(f).unwrap())));
    }
    out
}

fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn production_of(rel: &str) -> String {
    production_sources()
        .into_iter()
        .find(|(r, _)| r == rel)
        .unwrap_or_else(|| panic!("src/{rel} not found among the shell's production sources"))
        .1
}

/// The scan's own footing: it reads the files it should, and skips the ones
/// it should (this file, the other test files, the inline test modules).
#[test]
fn no_gl_scan_reads_production_and_skips_tests() {
    let srcs = production_sources();
    let names: Vec<&str> = srcs.iter().map(|(r, _)| r.as_str()).collect();
    for must in ["main.rs", "app.rs", "gpu.rs", "app/editor_v2.rs", "canvas.rs"] {
        assert!(names.contains(&must), "the scan does not read src/{must}: {names:?}");
    }
    for skip in ["no_gl_windows_tests.rs", "title_tests.rs", "orphans_tests.rs"] {
        assert!(!names.contains(&skip), "the scan reads the test file src/{skip}");
    }
    let canvas = production_of("canvas.rs");
    assert!(!canvas.contains("fn no_local_time_component_is_unwrapped"), "inline test modules are not stripped");
    assert!(canvas.contains("fn x_of_ts("), "production code was stripped with the test module");
    let app = production_of("app.rs");
    assert!(!app.contains("fn action_level_labels_are_distinct_and_nonempty"), "app.rs's taskbar_tests not stripped");
}

/// W4: no `RenderContext::new(` in the shell's production code, and no other
/// way of making a wgpu instance than the one in `gpu.rs`.
#[test]
fn no_gl_no_render_context_new_in_production() {
    let mut found = Vec::new();
    let mut instance_new = Vec::new();
    for (rel, code) in production_sources() {
        for (n, line) in code.lines().enumerate() {
            if line.contains("RenderContext::new(") {
                found.push(format!("src/{rel}:{}: {}", n + 1, line.trim()));
            }
            if line.contains("Instance::default(") || (line.contains("Instance::new(") && rel != "gpu.rs") {
                instance_new.push(format!("src/{rel}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(found.is_empty(), "RenderContext::new() in production code (W4: use crate::gpu::render_context()):\n{}", found.join("\n"));
    assert!(instance_new.is_empty(), "a wgpu instance made outside src/gpu.rs:\n{}", instance_new.join("\n"));
    let gpu = production_of("gpu.rs");
    assert_eq!(gpu.matches("Instance::new(").count(), 1, "src/gpu.rs creates its instance in exactly one place");
}

/// W4: `render_context()` builds its instance from `instance_descriptor`, for
/// the platform it runs on and the `WGPU_BACKEND` it reads, and nothing in
/// `gpu.rs` brings the backends back by another path.
#[test]
fn no_gl_render_context_goes_through_the_descriptor() {
    let gpu = production_of("gpu.rs");
    assert!(gpu.contains("fn render_context("), "src/gpu.rs has no fn render_context");
    assert!(gpu.matches("instance_descriptor(").count() >= 2, "render_context() does not call instance_descriptor()");
    assert!(gpu.matches("current_platform(").count() >= 2, "render_context() does not call current_platform()");
    assert!(gpu.contains("\"WGPU_BACKEND\""), "src/gpu.rs does not read WGPU_BACKEND");
    for bypass in ["Backends::from_env(", "from_env().unwrap_or_default()"] {
        assert!(!gpu.contains(bypass), "src/gpu.rs chooses backends outside shell_backends(): {bypass}");
    }
}

/// W4: both former sites call `render_context()`: `App::new` and `run_shot`.
#[test]
fn no_gl_both_sites_call_render_context() {
    let app = production_of("app.rs");
    let calls = app.matches("render_context()").count();
    assert!(calls >= 2, "app.rs calls render_context() {calls} time(s); App::new and run_shot both must");
    let shot = app.find("fn run_shot(").expect("fn run_shot not found in app.rs");
    let after = &app[shot..];
    let end = ["\nfn ", "\npub(crate) fn ", "\npub fn "]
        .iter()
        .filter_map(|m| after[1..].find(m))
        .min()
        .map_or(after.len(), |e| e + 1);
    assert!(after[..end].contains("render_context()"), "run_shot does not call render_context()");
    // The App::new site: the `context:` field.
    let ctx = app.find("context: {").expect("App::new's `context: {` field not found");
    let field = &app[ctx..(ctx + 600).min(app.len())];
    assert!(field.contains("render_context()"), "App::new's context field does not call render_context():\n{field}");
}

/// W6: the test lock stays, and `App::new` still holds it while it creates
/// the context.
#[test]
fn no_gl_test_lock_is_kept_around_app_new() {
    let app = production_of("app.rs");
    assert!(app.contains("static TEST_WGPU_INSTANCE_INIT"), "TEST_WGPU_INSTANCE_INIT is gone (W6)");
    let ctx = app.find("context: {").expect("App::new's `context: {` field not found");
    let field = &app[ctx..(ctx + 600).min(app.len())];
    let lock = field.find("TEST_WGPU_INSTANCE_INIT.lock()").unwrap_or_else(|| panic!("App::new no longer takes the lock:\n{field}"));
    let call = field.find("render_context()").unwrap_or_else(|| panic!("App::new's context field does not call render_context():\n{field}"));
    assert!(lock < call, "the lock is taken after the context is created:\n{field}");
    // Held, not dropped, until the call: same block, not bound to `_`, no drop.
    let between = &field[lock..call];
    assert!(!between.contains('}') && !between.contains("drop("), "the lock is released before the call:\n{field}");
    let stmt_start = field[..lock].rfind("let ").unwrap_or_else(|| panic!("the lock is not bound:\n{field}"));
    let binding = field[stmt_start + 4..lock].split('=').next().unwrap().trim();
    assert!(binding != "_", "`let _ = ….lock()` drops the lock at once:\n{field}");
}
