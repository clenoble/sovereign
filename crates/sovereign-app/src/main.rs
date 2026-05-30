//! Desktop binary entrypoint. The actual app — module declarations,
//! CLI dispatch, Tauri builder, the lot — lives in `src/lib.rs`. This
//! shim keeps `src/main.rs` minimal so the same code can build as
//! both a `cdylib` (consumed by the Android JNI loader via
//! `sovereign_app::run`) and a `bin` (this `main()`).

fn main() -> anyhow::Result<()> {
    sovereign_app::run_cli()
}
