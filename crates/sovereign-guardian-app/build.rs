fn main() {
    // Tauri build step — generates the context for tauri::generate_context!()
    // and handles platform icon/version resources.
    tauri_build::build();
}
