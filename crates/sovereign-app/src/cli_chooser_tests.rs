//! FC5 at a terminal (spec ch. 03, *Two threads of the name sought: asked
//! once, remembered*; v0.0.13, item 9): the bulk import asks which thread, at
//! the terminal, for a name two live threads share.
//!
//! Written before the code, by another author; the implementer does not edit
//! this file. What is typed is given as the chooser's input; what it writes is
//! read from its output.

use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

use chrono::TimeZone;
use sovereign_db::schema::{raw_to_thing, Thread};
use sovereign_import::ThreadChooser;

use crate::commands::TerminalChooser;

#[derive(Clone, Default)]
struct Written(Rc<RefCell<Vec<u8>>>);

impl Write for Written {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn web(id: &str, day: u32, month: u32) -> Thread {
    let mut t = Thread::new("Web".into(), String::new());
    t.id = raw_to_thing(id);
    t.created_at = chrono::Utc.with_ymd_and_hms(2026, month, day, 12, 0, 0).unwrap();
    t
}

fn threads() -> Vec<Thread> {
    vec![web("thread:first", 1, 9), web("thread:second", 15, 10)]
}

/// What she types, and what the chooser wrote.
fn ask(typed: &str) -> (Option<String>, String) {
    let out = Written::default();
    let chooser = TerminalChooser::new(Box::new(std::io::Cursor::new(typed.as_bytes().to_vec())), Box::new(out.clone()));
    let chosen = chooser.choose("Web", &threads());
    let written = String::from_utf8(out.0.borrow().clone()).unwrap();
    (chosen, written)
}

/// FC1, FC5: the question names the name, lists the threads numbered from 1,
/// and says the choice will be remembered; a number picks that thread.
#[test]
fn fc5_the_terminal_lists_the_threads_and_a_number_picks_one() {
    let (chosen, written) = ask("2\n");
    assert_eq!(chosen.as_deref(), Some("thread:second"));
    assert!(written.contains("Web"), "the name: {written}");
    let starts: Vec<char> = written.lines().filter_map(|l| l.trim_start().chars().next()).collect();
    assert!(starts.contains(&'1') && starts.contains(&'2'), "each thread on its own numbered line: {written}");
    assert!(written.to_lowercase().contains("remembered"), "says the choice will be remembered: {written}");
    let (chosen, _) = ask("1\n");
    assert_eq!(chosen.as_deref(), Some("thread:first"));
}

/// FC1, FC5: nothing typed (the input ends): no answer, nothing chosen.
#[test]
fn fc5_no_answer_chooses_nothing() {
    assert_eq!(ask("").0, None);
}

/// A number no thread has is not an answer for a thread.
#[test]
fn fc5_a_number_out_of_the_list_chooses_nothing() {
    assert_eq!(ask("9\n").0, None);
    assert_eq!(ask("0\n").0, None);
}
