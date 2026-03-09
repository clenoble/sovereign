/// Extension trait to convert any `Result<T, E: Display>` into `Result<T, String>`.
/// Replaces the repetitive `.map_err(|e| e.to_string())` pattern in Tauri commands.
pub trait ToStringErr<T> {
    fn str_err(self) -> Result<T, String>;
}

impl<T, E: std::fmt::Display> ToStringErr<T> for Result<T, E> {
    fn str_err(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}
