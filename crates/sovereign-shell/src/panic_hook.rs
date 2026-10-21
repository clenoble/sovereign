//! The shell's panic hook — installed in every build (spec ch. 12, *Crash
//! messages*, decided 2026-10-10; C-TEXT weakness W4).
//!
//! Rust's standard panic message can quote the string it failed on — a bad
//! string slice quotes up to 256 bytes of it — so a slicing bug anywhere in the
//! document editor, parley or any other dependency would put document text on
//! stderr, and into any log or crash report that captures stderr. This hook
//! never prints or logs the panic **payload**. It writes the location
//! (`file:line:column`) and the thread name, then what the standard hook would
//! write after the message: a backtrace when `RUST_BACKTRACE` asks for one
//! (frames name functions and source paths, never data) or the usual hint.
//!
//! "Chains to the existing behaviour minus the message": the previous hook (the
//! standard one, unless something installed another) cannot be called without
//! handing it the payload, so it is not called; its output is reproduced here
//! without the message. Unwinding / aborting after the hook is unchanged.

use std::backtrace::{Backtrace, BacktraceStatus};
use std::io::Write;
use std::panic::PanicHookInfo;

/// What the hook writes for a panic, built from the location and the thread
/// name only. It takes no payload, so it cannot contain one.
pub(crate) fn panic_line(thread: Option<&str>, location: Option<(&str, u32, u32)>) -> String {
    let thread = thread.unwrap_or("<unnamed>");
    let at = match location {
        Some((file, line, col)) => format!("{file}:{line}:{col}"),
        None => "an unknown location".to_string(),
    };
    format!(
        "thread '{thread}' panicked at {at}\n\
         (panic message withheld: it can quote document text)"
    )
}

/// The full report for one panic (line + backtrace part).
fn report(info: &PanicHookInfo<'_>) -> String {
    let cur = std::thread::current();
    let loc = info.location().map(|l| (l.file(), l.line(), l.column()));
    let mut out = panic_line(cur.name(), loc);
    let bt = Backtrace::capture();
    match bt.status() {
        BacktraceStatus::Captured => {
            out.push_str("\nstack backtrace:\n");
            out.push_str(&bt.to_string());
        }
        _ => out.push_str(
            "\nnote: run with `RUST_BACKTRACE=1` environment variable to display a backtrace",
        ),
    }
    out
}

/// Installs the hook. Called first thing in `main`, in every build.
pub(crate) fn install() {
    std::panic::set_hook(Box::new(|info| {
        #[allow(unused_mut)]
        let mut text = report(info);
        // Development only, compiled out of release builds: `SHELL_PANIC_PAYLOAD=1`
        // also prints the payload, to diagnose a crash on a throw-away test
        // workspace. Never set it on a workspace holding real documents.
        #[cfg(any(debug_assertions, feature = "dev-switches"))]
        if crate::dev::flag("SHELL_PANIC_PAYLOAD") {
            let payload = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string payload>".to_string());
            text.push_str(&format!("\n[SHELL_PANIC_PAYLOAD] {payload}"));
        }
        // stderr only, as the standard hook does; a failed write is ignored
        // (there is nowhere else to say it).
        let _ = writeln!(std::io::stderr().lock(), "{text}");
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    const SECRET: &str = "Dear Dr Rigamonti, my AVS number is 756.1234.5678.97";

    #[test]
    fn the_line_names_the_thread_and_location_only() {
        let s = panic_line(Some("main"), Some(("crates/x/src/a.rs", 12, 5)));
        assert!(s.contains("thread 'main'"));
        assert!(s.contains("crates/x/src/a.rs:12:5"));
        let s = panic_line(None, None);
        assert!(s.contains("<unnamed>") && s.contains("unknown location"));
    }

    /// A real panic whose message quotes "document text" — through the same
    /// report the installed hook writes — never carries that text. The hook is
    /// process-global, so the test installs a capturing copy only around one
    /// named thread and restores the previous hook after.
    #[test]
    fn a_panic_payload_never_reaches_the_report() {
        let captured: Arc<Mutex<Vec<String>>> = Arc::default();
        let sink = captured.clone();
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if std::thread::current().name() == Some("panic-hook-test") {
                sink.lock().unwrap().push(report(info));
            }
        }));
        let r = std::thread::Builder::new()
            .name("panic-hook-test".into())
            .spawn(|| {
                let doc = String::from(SECRET);
                // The standard library's own message for this quotes `doc`.
                let _ = &doc[0..doc.len() + 1];
            })
            .unwrap()
            .join();
        std::panic::set_hook(prev);
        assert!(r.is_err(), "the thread panicked");
        let got = captured.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert!(!got[0].contains("Rigamonti") && !got[0].contains("756.1234"), "payload leaked: {}", got[0]);
        assert!(got[0].contains("panic-hook-test"));
        assert!(got[0].contains("panic_hook.rs"), "location missing: {}", got[0]);
    }
}
