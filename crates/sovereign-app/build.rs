fn main() {
    // Embed the app icon as a Windows PE resource so pinned taskbar icons work.
    // Skip when building with Tauri — tauri_build handles icon/version resources.
    #[cfg(all(target_os = "windows", not(feature = "tauri-ui")))]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("icon.ico");
        if let Err(e) = res.compile() {
            eprintln!("Warning: failed to embed Windows icon resource: {e}");
        }
    }

    // Tauri build step — generates context for tauri::generate_context!()
    #[cfg(feature = "tauri-ui")]
    tauri_build::build();
}
