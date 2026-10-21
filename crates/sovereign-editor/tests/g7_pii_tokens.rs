//! Guarantee 7 — "Personal-data tokens are atomic: shown as their label (or the
//! value when revealed, ch. 09), selected and deleted as one unit, never partly
//! edited; copying a token copies its label unless revealed." (spec ch. 12,
//! C-TEXT) — and ch. 03: "Personal-data tokens [pii:<…>] (ch. 09) live inside
//! text and are atomic: the editor never lets a token be partly edited."
//!
//! Token form, decided by Céline 2026-10-10: the spec follows the code,
//! `[pii:<record_id>]`, and record ids contain `:` (`pii_record:x7k2`).
//!
//! Also decided 2026-10-10 and tested here:
//! - **the resolver rule** (replaces the first "typed syntax is never a token"
//!   decision): a token is a token only in a document listed among its
//!   record's sources; anywhere else its form is ordinary text. The host
//!   decides (`TokenDisplay::NotAToken`); the editor stores text as typed;
//! - **copy and paste inside Sovereign keep the token** (same id, atomic) and
//!   the editor tells the host to link the record to the target document
//!   (`PiiDisplay::link_to_document`); outside Sovereign the clipboard text
//!   carries the label, or the value when revealed;
//!
//! The fake resolver answers for one document: records registered with the
//! fake are linked to it unless marked `not_linked_here`; unknown ids are
//! ordinary text.
//! - **a copy that includes a revealed value is cleared after 30 s**; ordinary
//!   text is never cleared.

mod common;
use common::*;

use sovereign_editor::input::{Modifiers, NamedKey};
use sovereign_editor::Selection;

const LABEL: &str = "\u{2039}email\u{203A}";
const VALUE: &str = "alice@example.com";

fn setup(text: &str) -> (sovereign_editor::Editor, Fakes) {
    let (mut ed, f) = open_focused(&doc1(text));
    for id in ["pii_record:3", "pii_record:x7k2"] {
        f.pii.borrow_mut().insert(id.into(), PiiEntry { label: LABEL.into(), value: VALUE.into(), revealed: false });
    }
    ed.pii_display_changed();
    (ed, f)
}

const TOKENS: &[&str] = &["[pii:pii_record:3]", "[pii:pii_record:x7k2]"];

/// Every token of `original` is, in `after`, either whole or gone.
fn assert_tokens_whole(original: &str, after: &str, token: &str) {
    let whole = after.matches(token).count();
    assert!(whole <= original.matches(token).count());
    let rest = after.replace(token, "");
    assert!(!rest.contains("pii:"), "a token was partly edited: {after:?}");
    let inner = &token[1..token.len() - 1];
    assert!(!rest.contains(inner), "a token fragment remains: {after:?}");
}

// "selected … as one unit" — Shift+Right from before a token selects all of it.
#[test]
fn g7_shift_arrow_selects_the_whole_token() {
    for t in TOKENS {
        let text = format!("a{t}b");
        let (mut ed, _f) = setup(&text);
        caret_at(&mut ed, 1, 1);
        key(&mut ed, NamedKey::ArrowRight, Modifiers::SHIFT);
        assert_eq!(ed.selection().focus.offset, 1 + t.len(), "{t}");
        key(&mut ed, NamedKey::ArrowLeft, Modifiers::SHIFT);
        assert_eq!(ed.selection().focus.offset, 1, "{t}");
    }
}

// "selected … as one unit" — the caret jumps over a token, never into it.
#[test]
fn g7_arrows_step_over_a_token() {
    for t in TOKENS {
        let text = format!("a{t}b");
        let (mut ed, _f) = setup(&text);
        caret_at(&mut ed, 1, 1);
        press(&mut ed, NamedKey::ArrowRight);
        assert_eq!(caret(&ed).offset, 1 + t.len());
        press(&mut ed, NamedKey::ArrowLeft);
        assert_eq!(caret(&ed).offset, 1);
    }
}

// "never partly edited" — no selection endpoint can rest inside a token.
#[test]
fn g7_selection_endpoints_never_rest_inside_a_token() {
    for t in TOKENS {
        let text = format!("ab {t} cd {t}");
        let (mut ed, _f) = setup(&text);
        for a in 0..=text.len() {
            for b in [0, 4, 9, text.len(), a] {
                ed.set_selection(Selection { anchor: pos(1, a), focus: pos(1, b) });
                let s = ed.selection();
                assert!(!strictly_inside_token(&text, s.anchor.offset), "anchor {a} -> {}", s.anchor.offset);
                assert!(!strictly_inside_token(&text, s.focus.offset), "focus {b} -> {}", s.focus.offset);
            }
        }
    }
}

// "deleted as one unit" — Backspace after, Delete before.
#[test]
fn g7_backspace_and_delete_remove_the_whole_token() {
    for t in TOKENS {
        let text = format!("x{t}y");
        let (mut ed, _f) = setup(&text);
        caret_at(&mut ed, 1, 1 + t.len());
        press(&mut ed, NamedKey::Backspace);
        assert_eq!(text_of(&ed, 1), "xy", "{t}");
        let (mut ed, _f) = setup(&text);
        caret_at(&mut ed, 1, 1);
        press(&mut ed, NamedKey::Delete);
        assert_eq!(text_of(&ed, 1), "xy", "{t}");
    }
}

// "never partly edited" — every editing operation, from every caret position
// and every selection, leaves each token whole or entirely removed.
#[test]
fn g7_no_operation_edits_part_of_a_token() {
    type Op = fn(&mut sovereign_editor::Editor);
    let ops: &[(&str, Op)] = &[
        ("Backspace", |e| press(e, NamedKey::Backspace)),
        ("Delete", |e| press(e, NamedKey::Delete)),
        ("Ctrl+Backspace", |e| {
            key(e, NamedKey::Backspace, Modifiers::CTRL);
        }),
        ("Ctrl+Delete", |e| {
            key(e, NamedKey::Delete, Modifiers::CTRL);
        }),
        ("type", |e| type_text(e, "Z")),
        ("Enter", |e| press(e, NamedKey::Enter)),
        ("cut", |e| {
            ctrl(e, "x");
        }),
        ("paste", |e| {
            ctrl(e, "v");
        }),
        ("ime commit", |e| {
            preedit(e, "に");
            ime(e, sovereign_editor::input::ImeEvent::Commit("日".into()));
        }),
    ];
    for t in TOKENS {
        let text = format!("ab {t}cd{t} e");
        for (name, op) in ops {
            for a in (0..=text.len()).step_by(2) {
                for b in [a, (a + 3).min(text.len()), (a + 9).min(text.len())] {
                    let (mut ed, f) = setup(&text);
                    set_clip_text(&f, "P");
                    ed.set_selection(Selection { anchor: pos(1, a), focus: pos(1, b) });
                    op(&mut ed);
                    let after = text_of(&ed, 1);
                    let r = std::panic::catch_unwind(|| assert_tokens_whole(&text, &after, t));
                    assert!(r.is_ok(), "{name} with selection {a}..{b} on {text:?} gave {after:?}");
                }
            }
        }
    }
}

// "shown as their label" — the label, not the raw token and not the value,
// is what the accessible text holds (what is displayed).
#[test]
fn g7_token_is_shown_as_its_label() {
    let (mut ed, _f) = setup("mail [pii:pii_record:3] now");
    let tree = a11y_tree(&mut ed);
    let ft = focused_text(&tree);
    assert!(ft.text.contains(LABEL), "{:?}", ft.text);
    assert!(!ft.text.contains(VALUE));
    assert!(!ft.text.contains("[pii:"));
}

// "(or the value when revealed, ch. 09)"
#[test]
fn g7_revealed_token_shows_its_value() {
    let (mut ed, f) = setup("mail [pii:pii_record:3] now");
    f.pii.borrow_mut().get_mut("pii_record:3").unwrap().revealed = true;
    ed.pii_display_changed();
    let ft = focused_text(&a11y_tree(&mut ed));
    assert!(ft.text.contains(VALUE), "{:?}", ft.text);
    // revealing changes display, not the document
    assert_eq!(text_of(&ed, 1), "mail [pii:pii_record:3] now");
}

// "copying a token copies its label unless revealed"
#[test]
fn g7_copy_of_unrevealed_token_copies_the_label() {
    for t in TOKENS {
        let text = format!("mail {t} now");
        let (mut ed, f) = setup(&text);
        ctrl(&mut ed, "a");
        ctrl(&mut ed, "c");
        let clip = clip_text(&f).expect("copied");
        assert_eq!(clip, format!("mail {LABEL} now"), "{t}");
        assert!(!clip.contains(VALUE));
    }
}

// "copying a token copies its label unless revealed" — revealed: the value.
#[test]
fn g7_copy_of_revealed_token_copies_the_value() {
    let (mut ed, f) = setup("mail [pii:pii_record:3] now");
    f.pii.borrow_mut().get_mut("pii_record:3").unwrap().revealed = true;
    ed.pii_display_changed();
    ctrl(&mut ed, "a");
    ctrl(&mut ed, "c");
    assert_eq!(clip_text(&f).unwrap(), format!("mail {VALUE} now"));
}

// "copying a token copies its label" — a selection covering just the token.
#[test]
fn g7_copy_of_a_selection_holding_only_the_token() {
    let (mut ed, f) = setup("a[pii:pii_record:3]b");
    caret_at(&mut ed, 1, 1);
    key(&mut ed, NamedKey::ArrowRight, Modifiers::SHIFT);
    ctrl(&mut ed, "c");
    assert_eq!(clip_text(&f).unwrap(), LABEL);
}

// Cut removes the token whole and puts its label (not the value) on the
// clipboard.
#[test]
fn g7_cut_of_unrevealed_token() {
    let (mut ed, f) = setup("a[pii:pii_record:3]b");
    caret_at(&mut ed, 1, 1);
    key(&mut ed, NamedKey::ArrowRight, Modifiers::SHIFT);
    ctrl(&mut ed, "x");
    assert_eq!(text_of(&ed, 1), "ab");
    let clip = clip_text(&f).unwrap();
    assert!(!clip.contains(VALUE) && !clip.contains("[pii:"), "{clip:?}");
}

// A token the host shows with a placeholder label (ch. 09: an undecryptable
// record "renders as a placeholder") is atomic too.
#[test]
fn g7_placeholder_token_is_atomic_too() {
    let (mut ed, f) = setup("x[pii:pii_record:999]y");
    add_pii(&f, "999", "\u{2039}unavailable\u{203A}", "", false);
    ed.pii_display_changed();
    caret_at(&mut ed, 1, 1 + "[pii:pii_record:999]".len());
    press(&mut ed, NamedKey::Backspace);
    assert_eq!(text_of(&ed, 1), "xy");
}

// Undo of a token deletion restores the token byte for byte.
#[test]
fn g7_undo_restores_a_deleted_token_exactly() {
    let (mut ed, _f) = setup("x[pii:pii_record:x7k2]y");
    let before = model_json(&ed);
    caret_at(&mut ed, 1, 1);
    press(&mut ed, NamedKey::Delete);
    ctrl(&mut ed, "z");
    assert_eq!(model_json(&ed), before);
}

// Saving writes the token as stored, never its label or value.
#[test]
fn g7_save_writes_the_token_not_its_display() {
    let (mut ed, f) = setup("mail [pii:pii_record:3] now");
    f.pii.borrow_mut().get_mut("pii_record:3").unwrap().revealed = true;
    ed.pii_display_changed();
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, ">");
    ctrl(&mut ed, "s");
    let saved = last_save(&f).content;
    assert_eq!(texts_in(&saved), vec![">mail [pii:pii_record:3] now"]);
    assert!(!saved.contains(VALUE) && !saved.contains(LABEL));
}

// ------------------------------------------------------------ the resolver rule
// Decided 2026-10-10 (replaces the first "typed syntax is never a token"
// decision): "a token is shown as a token only in a document listed among its
// personal-data record's sources; anywhere else it is ordinary text." The host
// applies it (`TokenDisplay::NotAToken`); the editor escapes nothing.
// (Typing the exact form of a token linked to this document may become a
// token — accepted, and deliberately not tested either way.)

/// Text in token form that the host says is not a token here is ordinary
/// text in `ed`: shown as written, the caret can rest inside it, Backspace
/// at its end removes one character.
fn assert_ordinary_text(ed: &mut sovereign_editor::Editor, n: u32, literal: &str) {
    let shown = focused_text(&a11y_tree(ed)).text;
    assert!(shown.contains(literal), "shown as written: {shown:?}");
    assert!(!shown.contains(LABEL) && !shown.contains(VALUE), "shown as a token: {shown:?}");
    let t = text_of(ed, n);
    let start = t.find(literal).expect("stored as written");
    let inside = start + 5; // just after "[pii:"
    caret_at(ed, n, inside);
    assert_eq!(caret(ed).offset, inside, "the caret can rest inside it: it is not atomic");
    let end = start + literal.len();
    caret_at(ed, n, end);
    press(ed, NamedKey::Backspace);
    let after = text_of(ed, n);
    assert_eq!(after, format!("{}{}{}", &t[..start], &literal[..literal.len() - 1], &t[end..]), "Backspace removed one character");
}

// A stored document holding the token form of a record whose sources do not
// include this document: ordinary text, copied as its literal text.
#[test]
fn g7_token_form_not_linked_here_is_ordinary_text() {
    let t = token("x7k2");
    let (mut ed, f) = setup(&format!("a{t}b"));
    not_linked_here(&f, "x7k2");
    ed.pii_display_changed();
    ctrl(&mut ed, "a");
    ctrl(&mut ed, "c");
    assert_eq!(clip_text(&f).as_deref(), Some(format!("a{t}b").as_str()), "copied as its literal text");
    assert_eq!(last_write(&f).clear_after, None);
    assert_ordinary_text(&mut ed, 1, &t);
}

// The same record's token IS atomic in a document among its sources (the
// host answers per document) — the g7 atomicity tests above are that case.
#[test]
fn g7_same_token_form_is_atomic_where_linked() {
    let t = token("x7k2");
    let (mut ed, _f) = setup(&format!("a{t}b"));
    caret_at(&mut ed, 1, 1 + t.len());
    press(&mut ed, NamedKey::Backspace);
    assert_eq!(text_of(&ed, 1), "ab");
}

// Typed token form of a record not linked here: ordinary text, stored byte for
// byte as typed (no inserted characters), and nothing is linked.
#[test]
fn g7_typed_token_form_is_stored_byte_for_byte() {
    let t = token("x7k2");
    let (mut ed, f) = setup("");
    not_linked_here(&f, "x7k2");
    ed.pii_display_changed();
    caret_at(&mut ed, 1, 0);
    let typed = format!("see {t} here");
    type_text(&mut ed, &typed);
    assert_eq!(text_of(&ed, 1), typed, "the model holds exactly what was typed");
    ctrl(&mut ed, "s");
    assert_eq!(texts_in(&last_save(&f).content), vec![typed.clone()], "stored byte for byte");
    assert!(f.pii_links.borrow().is_empty(), "typing links nothing: {:?}", f.pii_links.borrow());
    assert_ordinary_text(&mut ed, 1, &t);
}

// Pasted from outside Sovereign (plain text, no Sovereign payload): ordinary
// text, stored as pasted, and nothing is linked.
#[test]
fn g7_token_form_pasted_from_outside_links_nothing() {
    let t = token("x7k2");
    let (mut ed, f) = setup("");
    not_linked_here(&f, "x7k2");
    ed.pii_display_changed();
    set_clip_text(&f, format!("see {t} here"));
    caret_at(&mut ed, 1, 0);
    ctrl(&mut ed, "v");
    assert!(f.pii_links.borrow().is_empty(), "a plain-text paste announced {:?}", f.pii_links.borrow());
    ctrl(&mut ed, "s");
    assert_eq!(texts_in(&last_save(&f).content), vec![format!("see {t} here")]);
    assert_ordinary_text(&mut ed, 1, &t);
}

// A label pasted from outside Sovereign is plain text, not a token.
#[test]
fn g7_pasted_label_from_outside_is_plain_text() {
    let (mut ed, f) = setup("");
    set_clip_text(&f, format!("mail {LABEL} now"));
    caret_at(&mut ed, 1, 0);
    ctrl(&mut ed, "v");
    assert!(!text_of(&ed, 1).contains("[pii:"), "{:?}", text_of(&ed, 1));
    assert_eq!(focused_text(&a11y_tree(&mut ed)).text.trim_end_matches('\n'), format!("mail {LABEL} now"));
}

// ------------------------------------------------------------ copy and paste inside Sovereign
// Decided 2026-10-10: "Internal copy-paste keeps the token: pasting a
// Sovereign clipboard payload containing a token from document A into
// document B yields that token in B, and the editor tells the host to link
// the record to B."

#[test]
fn g7_copy_paste_inside_sovereign_keeps_the_token_and_links_it() {
    for revealed in [false, true] {
        let t = token("x7k2");
        // document A, where the record is a source
        let (mut a, fa) = setup(&format!("mail {t} now"));
        reveal(&fa, "x7k2", revealed);
        a.pii_display_changed();
        ctrl(&mut a, "a");
        ctrl(&mut a, "c");
        let content = fa.clipboard.borrow().content.clone().expect("copied");
        let shown = if revealed { VALUE } else { LABEL };
        assert_eq!(content.text, format!("mail {shown} now"), "outside Sovereign: the displayed text");
        if let Some(p) = &content.sovereign {
            assert!(!p.contains(VALUE), "the Sovereign payload never carries a revealed value");
        }
        // document B, not (yet) among the record's sources
        let (mut b, fb) = setup("");
        not_linked_here(&fb, "x7k2");
        b.pii_display_changed();
        fb.clipboard.borrow_mut().content = Some(content);
        caret_at(&mut b, 1, 0);
        ctrl(&mut b, "v");
        assert_eq!(text_of(&b, 1), format!("mail {t} now"), "same token, same id (revealed: {revealed})");
        let linked: Vec<String> = fb.pii_links.borrow().iter().flatten().cloned().collect();
        assert!(linked.contains(&rid("x7k2")), "the host was told to link the record to B: {linked:?}");
        ctrl(&mut b, "s");
        assert_eq!(texts_in(&last_save(&fb).content), vec![format!("mail {t} now")], "B saves the token as stored");
        // atomic in B at once
        caret_at(&mut b, 1, 5 + t.len());
        press(&mut b, NamedKey::Backspace);
        assert_eq!(text_of(&b, 1), "mail  now", "atomic in B immediately");
    }
}

// The same plain text, from a non-Sovereign source, announces nothing and
// stays ordinary text in B.
#[test]
fn g7_same_text_without_the_sovereign_payload_announces_nothing() {
    let t = token("x7k2");
    let (mut b, fb) = setup("");
    not_linked_here(&fb, "x7k2");
    b.pii_display_changed();
    set_clip_text(&fb, format!("mail {t} now"));
    caret_at(&mut b, 1, 0);
    ctrl(&mut b, "v");
    assert!(fb.pii_links.borrow().is_empty(), "{:?}", fb.pii_links.borrow());
    assert_ordinary_text(&mut b, 1, &t);
}

// Same, inside one editor.
#[test]
fn g7_copy_paste_in_the_same_editor_keeps_the_token() {
    let t = token("x7k2");
    let (mut ed, _f) = setup(&format!("a{t}b"));
    caret_at(&mut ed, 1, 1);
    key(&mut ed, NamedKey::ArrowRight, Modifiers::SHIFT);
    ctrl(&mut ed, "c");
    let end = text_of(&ed, 1).len();
    caret_at(&mut ed, 1, end);
    ctrl(&mut ed, "v");
    assert_eq!(text_of(&ed, 1), format!("a{t}b{t}"));
}

// ------------------------------------------------------------ clearing a copied value
// Decided 2026-10-10: "A copied revealed personal-data value is cleared after
// 30 s … A copy whose content includes a revealed value counts as a revealed
// value." Ordinary text: no automatic clearing.

const THIRTY_S: std::time::Duration = std::time::Duration::from_secs(30);

#[test]
fn g7_copy_including_a_revealed_value_asks_for_clearing_after_30s() {
    let t = token("x7k2");
    let (mut ed, f) = setup(&format!("mail {t} now"));
    reveal(&f, "x7k2", true);
    ed.pii_display_changed();
    // the whole text
    ctrl(&mut ed, "a");
    ctrl(&mut ed, "c");
    assert_eq!(last_write(&f).clear_after, Some(THIRTY_S));
    // the token alone
    caret_at(&mut ed, 1, 5);
    key(&mut ed, NamedKey::ArrowRight, Modifiers::SHIFT);
    ctrl(&mut ed, "c");
    assert_eq!(last_write(&f).text, VALUE);
    assert_eq!(last_write(&f).clear_after, Some(THIRTY_S));
    // cut counts too
    ctrl(&mut ed, "x");
    assert_eq!(last_write(&f).clear_after, Some(THIRTY_S));
}

#[test]
fn g7_copy_without_a_revealed_value_sets_no_expiry() {
    let t = token("x7k2");
    let (mut ed, f) = setup(&format!("mail {t} now"));
    // unrevealed token: the label is copied, no expiry
    ctrl(&mut ed, "a");
    ctrl(&mut ed, "c");
    assert_eq!(last_write(&f).clear_after, None);
    // revealed elsewhere in the document, but not in the selection: no expiry
    reveal(&f, "x7k2", true);
    ed.pii_display_changed();
    caret_at(&mut ed, 1, 0);
    for _ in 0..4 {
        key(&mut ed, NamedKey::ArrowRight, Modifiers::SHIFT);
    }
    ctrl(&mut ed, "c");
    assert_eq!(last_write(&f).text, "mail");
    assert_eq!(last_write(&f).clear_after, None);
}
