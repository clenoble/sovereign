fn main() {
    // Verify libepoxy is installed (loaded at runtime via dlopen)
    pkg_config::Config::new()
        .cargo_metadata(false)
        .probe("epoxy")
        .expect("libepoxy not found. Install with: sudo apt install libepoxy-dev");
}
