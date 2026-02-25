fn main() {
    // Embed the app icon as a Windows PE resource so pinned taskbar icons work.
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("icon.ico");
        if let Err(e) = res.compile() {
            eprintln!("Warning: failed to embed Windows icon resource: {e}");
        }
    }
}
