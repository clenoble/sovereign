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
