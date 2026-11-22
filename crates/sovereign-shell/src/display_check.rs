//! Refusing to start with a plain sentence, instead of a panic, when no window
//! can be opened. One form, on stderr, then exit code 1:
//! `Sovereign cannot start: {reason}` (the Fable maille, by delegation of
//! Céline, 2026-11-16).
//!
//! On Linux under X11, winit 0.30 loads `libxkbcommon-x11` through
//! `xkbcommon-dl`, whose loader panics when the library cannot be loaded. When
//! no window system can be reached, `EventLoop::new()` returns an error. `main`
//! checks the first case before it creates the event loop, says the second
//! instead of unwrapping it, and exits in both before any store is opened.

/// The window system winit will use on Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub enum Backend {
    Wayland,
    X11,
    Neither,
}

/// Said when X11 is the window system and its keyboard library cannot be loaded.
pub const X11_KEYBOARD_MISSING: &str = "Sovereign cannot start: the X11 keyboard library \
(libxkbcommon-x11) could not be loaded. Install the package libxkbcommon-x11 \
(libxkbcommon-x11-0 on Debian and Ubuntu), or start Sovereign in a Wayland session.";

/// Said when the event loop fails and no display variable is set: the shell
/// knows this itself, so nothing of the error's text is quoted.
pub const NO_DISPLAY_VARIABLE: &str = "Sovereign cannot start: no graphical session was found \
(neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set).";

/// Said when the event loop cannot be created for any other reason, with its
/// error quoted whole.
pub fn no_graphical_session(error: &dyn std::fmt::Display) -> String {
    format!("Sovereign cannot start: no graphical session could be opened ({error}).")
}

/// The sentence for an event loop that failed. The backend rule only chooses
/// the words; `EventLoop::new()` stays the judge of whether a start is
/// possible (the Fable maille, by delegation of Céline, 2026-11-16).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn event_loop_failure(backend: Backend, error: &dyn std::fmt::Display) -> String {
    match backend {
        Backend::Neither => NO_DISPLAY_VARIABLE.to_string(),
        Backend::Wayland | Backend::X11 => no_graphical_session(error),
    }
}

/// The sentence for this machine's event loop error. Only Linux chooses its
/// words from the display variables, which mean nothing elsewhere.
#[cfg(target_os = "linux")]
pub fn event_loop_failure_here(error: &dyn std::fmt::Display) -> String {
    event_loop_failure(backend_from_env(|name| std::env::var(name).ok()), error)
}

#[cfg(not(target_os = "linux"))]
pub fn event_loop_failure_here(error: &dyn std::fmt::Display) -> String {
    no_graphical_session(error)
}

/// winit 0.30.13's rule (`platform_impl/linux/mod.rs`, `EventLoop::new`): Wayland
/// when `WAYLAND_DISPLAY` or `WAYLAND_SOCKET` is set and not empty, otherwise X11
/// when `DISPLAY` is set and not empty. `get` reads one variable.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn backend_from_env(get: impl Fn(&str) -> Option<String>) -> Backend {
    let set = |name: &str| get(name).is_some_and(|v| !v.is_empty());
    if set("WAYLAND_DISPLAY") || set("WAYLAND_SOCKET") {
        Backend::Wayland
    } else if set("DISPLAY") {
        Backend::X11
    } else {
        Backend::Neither
    }
}

/// The sentence to say before creating the event loop, if any. The probe runs
/// only when X11 will be used.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn refusal(backend: Backend, x11_keyboard_loads: impl FnOnce() -> bool) -> Option<&'static str> {
    match backend {
        Backend::X11 if !x11_keyboard_loads() => Some(X11_KEYBOARD_MISSING),
        _ => None,
    }
}

/// Reads the environment and probes with the loader winit itself uses, which
/// answers `None` where winit's own call panics.
#[cfg(target_os = "linux")]
pub fn refusal_before_event_loop() -> Option<&'static str> {
    let backend = backend_from_env(|name| std::env::var(name).ok());
    refusal(backend, || xkbcommon_dl::x11::xkbcommon_x11_option().is_some())
}

#[cfg(not(target_os = "linux"))]
pub fn refusal_before_event_loop() -> Option<&'static str> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| vars.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
    }

    #[test]
    fn wayland_is_chosen_over_x11_when_both_are_set() {
        assert_eq!(backend_from_env(env(&[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0")])), Backend::Wayland);
    }

    #[test]
    fn a_wayland_socket_alone_means_wayland() {
        assert_eq!(backend_from_env(env(&[("WAYLAND_SOCKET", "3"), ("DISPLAY", ":0")])), Backend::Wayland);
    }

    #[test]
    fn display_alone_means_x11() {
        assert_eq!(backend_from_env(env(&[("DISPLAY", ":0")])), Backend::X11);
    }

    #[test]
    fn an_empty_variable_counts_as_unset() {
        assert_eq!(backend_from_env(env(&[("WAYLAND_DISPLAY", ""), ("WAYLAND_SOCKET", ""), ("DISPLAY", ":0")])), Backend::X11);
        assert_eq!(backend_from_env(env(&[("DISPLAY", "")])), Backend::Neither);
    }

    #[test]
    fn no_variable_means_neither() {
        assert_eq!(backend_from_env(env(&[])), Backend::Neither);
    }

    #[test]
    fn x11_without_its_keyboard_library_is_refused_with_the_sentence() {
        assert_eq!(refusal(Backend::X11, || false), Some(X11_KEYBOARD_MISSING));
    }

    #[test]
    fn x11_with_its_keyboard_library_starts() {
        assert_eq!(refusal(Backend::X11, || true), None);
    }

    #[test]
    fn the_library_is_probed_only_under_x11() {
        for backend in [Backend::Wayland, Backend::Neither] {
            assert_eq!(refusal(backend, || panic!("probed under {backend:?}")), None);
        }
    }

    #[test]
    fn the_x11_sentence_names_the_library_the_packages_and_wayland() {
        for part in ["(libxkbcommon-x11) could not be loaded", "package libxkbcommon-x11 (", "libxkbcommon-x11-0 on Debian and Ubuntu", "Wayland session"] {
            assert!(X11_KEYBOARD_MISSING.contains(part), "missing {part:?}");
        }
    }

    /// An error's text as winit prints it, with the build machine's path in it.
    const WINIT_STYLE_ERROR: &str = "os error at /builder/registry/winit/src/linux/mod.rs:765: no route";

    #[test]
    fn with_no_display_variable_the_shell_says_it_in_its_own_words() {
        assert_eq!(event_loop_failure(Backend::Neither, &WINIT_STYLE_ERROR), NO_DISPLAY_VARIABLE);
        assert!(!NO_DISPLAY_VARIABLE.contains("/builder/"));
    }

    #[test]
    fn any_other_event_loop_error_is_quoted_whole() {
        for backend in [Backend::Wayland, Backend::X11] {
            assert_eq!(
                event_loop_failure(backend, &WINIT_STYLE_ERROR),
                format!("Sovereign cannot start: no graphical session could be opened ({WINIT_STYLE_ERROR}).")
            );
        }
    }

    #[test]
    fn every_sentence_has_the_one_form() {
        assert!(X11_KEYBOARD_MISSING.starts_with("Sovereign cannot start: "));
        assert!(NO_DISPLAY_VARIABLE.starts_with("Sovereign cannot start: "));
        assert!(no_graphical_session(&"x").starts_with("Sovereign cannot start: "));
    }
}
