pub mod auth;
pub mod config;
pub mod content;
pub mod interfaces;
pub mod lifecycle;
pub mod profile;
pub mod security;

/// Cross-platform home directory: checks `USERPROFILE` (Windows) then `HOME` (Unix).
pub fn home_dir() -> std::path::PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home)
}

/// Sovereign data root.
///
/// Resolution:
/// 1. `SOVEREIGN_DATA_DIR` env var if set (mobile entrypoint sets this to
///    `app.path().app_data_dir()` before any sovereign code runs).
/// 2. Desktop default: `~/.sovereign`.
pub fn sovereign_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("SOVEREIGN_DATA_DIR") {
        return std::path::PathBuf::from(dir);
    }
    home_dir().join(".sovereign")
}
