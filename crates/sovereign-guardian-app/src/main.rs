//! Desktop entrypoint. The app itself — commands, serving loop, Tauri
//! builder — lives in `lib.rs`, so the same code builds as a desktop `bin`
//! today and an Android `cdylib` (`sovereign_guardian_app::run`) later.

fn main() -> anyhow::Result<()> {
    sovereign_guardian_app::run()
}
