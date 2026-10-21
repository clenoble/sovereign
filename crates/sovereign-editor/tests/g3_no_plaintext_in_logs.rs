//! Guarantee 3 — "No plaintext leaves memory except through the host's save.
//! … no log line containing document text …" (spec ch. 12, C-TEXT)
//!
//! A process-wide `tracing` subscriber and `log` logger, both at TRACE,
//! capture everything any code in this process logs. Each test uses its own
//! marker, so tests running in parallel do not see each other's text.

mod common;
use common::*;

use std::io::Write;
use std::sync::{Arc, Mutex, OnceLock};

static CAPTURED: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();

struct Sink(Arc<Mutex<Vec<u8>>>);
impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct LogCapture(Arc<Mutex<Vec<u8>>>);
impl log::Log for LogCapture {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        let line = format!("{} {} {}\n", record.level(), record.target(), record.args());
        self.0.lock().unwrap().extend_from_slice(line.as_bytes());
    }
    fn flush(&self) {}
}

fn captured() -> Arc<Mutex<Vec<u8>>> {
    CAPTURED
        .get_or_init(|| {
            let buf = Arc::new(Mutex::new(Vec::new()));
            let b = buf.clone();
            let subscriber = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .with_ansi(false)
                .with_writer(move || Sink(b.clone()))
                .finish();
            tracing::subscriber::set_global_default(subscriber).expect("global subscriber");
            log::set_boxed_logger(Box::new(LogCapture(buf.clone()))).expect("logger");
            log::set_max_level(log::LevelFilter::Trace);
            buf
        })
        .clone()
}

fn assert_not_logged(marker: &str) {
    let buf = captured();
    let text = String::from_utf8_lossy(&buf.lock().unwrap()).to_string();
    assert!(!text.contains(marker), "document text reached a log line:\n{text}");
}

// "no log line containing document text"
#[test]
fn g3_a_full_session_logs_no_document_text() {
    let _ = captured();
    let marker = "G3LOGMARKERA";
    let _ = session::exercise_everything(marker);
    assert_not_logged(marker);
}

// "no log line containing document text" — anything the editor hands out to be
// shown or logged (notices, Debug of the editor and its model, errors) is a
// potential log line.
#[test]
fn g3_notices_and_debug_output_contain_no_document_text() {
    let _ = captured();
    let marker = "G3DEBUGMARKERB";
    for s in session::exercise_everything(marker) {
        assert!(!s.contains(marker), "document text in an editor-provided string: {s}");
    }
    assert_not_logged(marker);
}

// "no log line containing document text" — a personal-data value, once
// revealed, is document text too.
#[test]
fn g3_revealed_pii_value_is_not_logged() {
    let _ = captured();
    let marker = "G3PIIMARKERC";
    let (mut ed, f) = open_focused(&doc1("mail [pii:pii_record:1] now"));
    f.pii.borrow_mut().insert(
        "pii_record:1".into(),
        PiiEntry { label: "\u{2039}email\u{203A}".into(), value: format!("{marker}@example.com"), revealed: true },
    );
    ed.pii_display_changed();
    paint(&mut ed);
    ctrl(&mut ed, "a");
    ctrl(&mut ed, "c");
    let _ = ed.accessibility_tree();
    assert!(!format!("{ed:?}").contains(marker));
    assert_not_logged(marker);
}
