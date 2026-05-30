fn main() {
    // Tauri build step — generates context for tauri::generate_context!() and
    // handles Windows icon/version resources.
    tauri_build::build();
}
