use tracing_subscriber::EnvFilter;

/// Initialize tracing with env filter support.
///
/// Set `RUST_LOG=debug` for verbose output, defaults to `info`.
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,rfd::backend::xdg_desktop_portal=off")),
        )
        .init();
}

pub fn log_startup() {
    tracing::info!("Sovereign OS starting up");
}

pub fn log_shutdown() {
    tracing::info!("Sovereign OS shutting down");
}
