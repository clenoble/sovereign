//! Lot 0.5 — development switches.
//!
//! The shell reads a family of `SHELL_*` environment variables (plus
//! `SPIKE_CARDS`) for development: `SHELL_SHOT` renders one frame headlessly to
//! a PNG, `SHELL_NO_AUTH` opens the raw store without the auth gate,
//! `SHELL_AUTH_PROBE` / `SHELL_CHAT_PROBE` exercise the key stack and the chat
//! loop, `SHELL_AUTH_AUTO` drives the gate during a shot, and `SHELL_OPEN_*`,
//! `SHELL_RUN_SKILL`, `SHELL_ACTION`, `SHELL_DRAG`, … stage a screen. Several
//! of them are dangerous in a user's hands (`SHELL_AUTH_PROBE` re-creates
//! `auth.store` in the real data directory; `SHELL_NO_AUTH` writes plaintext).
//!
//! **They exist only in debug builds**, or in a release build compiled with the
//! non-default `dev-switches` feature (for fast headless renders). Every read
//! goes through [`var`] / [`flag`]; in a shipped release build those return
//! nothing without touching the environment, and the probe/shot entry points
//! are not compiled at all. Nothing is logged about them either way.
//!
//! Developers: `cargo run -p sovereign-shell` (debug) honours them as before,
//! e.g. `SHELL_SHOT=out.png SHELL_NO_AUTH=1 SHELL_OPEN_SETTINGS=1`. For a
//! release-speed shot: `cargo run -p sovereign-shell --release --features
//! dev-switches`.

/// Whether this build honours development switches.
pub(crate) const ENABLED: bool = cfg!(any(debug_assertions, feature = "dev-switches"));

/// The value of development switch `name`, or `None` (always `None` in a
/// release build without `dev-switches`).
pub(crate) fn var(name: &str) -> Option<String> {
    gated_var(ENABLED, name)
}

/// Whether development switch `name` is set (always false in a release build
/// without `dev-switches`).
pub(crate) fn flag(name: &str) -> bool {
    var(name).is_some()
}

#[inline(always)]
fn gated_var(enabled: bool, name: &str) -> Option<String> {
    if !enabled {
        return None;
    }
    std::env::var(name).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With the gate closed (a release build), a set switch has no effect.
    #[test]
    fn a_closed_gate_ignores_a_set_switch() {
        let name = "SHELL_LOT05_TEST_SWITCH";
        std::env::set_var(name, "1");
        assert_eq!(gated_var(false, name), None);
        assert_eq!(gated_var(true, name).as_deref(), Some("1"));
        std::env::remove_var(name);
    }

    #[test]
    fn the_gate_is_open_exactly_in_debug_or_dev_switches_builds() {
        assert_eq!(ENABLED, cfg!(debug_assertions) || cfg!(feature = "dev-switches"));
    }

    /// No SHELL_* switch has an effect in a release build: every read of one
    /// goes through this module's gate. A direct `std::env::var("SHELL_…")`
    /// anywhere else in the crate would bypass it.
    #[test]
    fn no_dev_switch_is_read_outside_the_gate() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut scanned = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let file = path.file_name().unwrap().to_string_lossy().to_string();
            if !file.ends_with(".rs") || file == "dev.rs" {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            scanned += 1;
            for (i, line) in src.lines().enumerate() {
                let direct = line.contains("env::var(\"SHELL_")
                    || line.contains("env::var(\"SPIKE_")
                    || line.contains("env::var_os(\"SHELL_");
                assert!(!direct, "{file}:{}: reads a dev switch outside dev.rs: {line}", i + 1);
            }
        }
        assert!(scanned >= 10, "scanned only {scanned} files in {}", dir.display());
    }
}
