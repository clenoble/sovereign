//! A session that exercises every path the editor has (for guarantee 3: if
//! document text escapes anywhere, it escapes during this).

use std::time::Duration;

use sovereign_editor::host::RemoteChange;
use sovereign_editor::input::{Command, ImeEvent, InputEvent, NamedKey, PointerButton, PointerEvent};
use sovereign_editor::{Editor, FieldKind};

use super::*;

/// Runs a full editing session over documents containing `marker`, and returns
/// every string the editor handed out through notices and Debug.
pub fn exercise_everything(marker: &str) -> Vec<String> {
    let mut out = vec![];
    let secret_text = format!("{marker} body {marker}");

    // A document with a token, an unknown block, CRLF and bidi text.
    let content = v2(&[
        text_block(1, &format!("{secret_text}\r\nمرحبا [pii:pii_record:7] שלום")),
        unknown_block(2),
        text_block(3, &format!("tail {marker}")),
    ]);
    let (mut ed, f) = open_focused(&content);
    f.pii.borrow_mut().insert(
        "pii_record:7".into(),
        PiiEntry { label: "\u{2039}email\u{203A}".into(), value: format!("{marker}@example.com"), revealed: false },
    );
    ed.pii_display_changed();
    paint(&mut ed);
    let _ = ed.accessibility_tree();
    caret_at(&mut ed, 1, 3);
    type_text(&mut ed, &format!(" typed {marker} 👩‍👩‍👧‍👦 {} ", token("7")));
    for k in [NamedKey::ArrowLeft, NamedKey::ArrowRight, NamedKey::Home, NamedKey::End, NamedKey::ArrowDown, NamedKey::ArrowUp] {
        press(&mut ed, k);
    }
    ime(&mut ed, ImeEvent::Enabled);
    preedit(&mut ed, &format!("{marker}-preedit"));
    ed.handle_event(InputEvent::Focus(false));
    ed.handle_event(InputEvent::Focus(true));
    preedit(&mut ed, "にほん");
    ime(&mut ed, ImeEvent::Commit(format!("{marker}日本")));
    ime(&mut ed, ImeEvent::Disabled);
    ed.handle_event(InputEvent::Pointer(PointerEvent::Down {
        x: 30.0,
        y: 10.0,
        button: PointerButton::Primary,
        click_count: 2,
        modifiers: sovereign_editor::input::Modifiers::NONE,
    }));
    ed.handle_event(InputEvent::Pointer(PointerEvent::Move { x: 300.0, y: 40.0 }));
    ed.handle_event(InputEvent::Pointer(PointerEvent::Up { x: 300.0, y: 40.0, button: PointerButton::Primary }));
    ctrl(&mut ed, "a");
    ctrl(&mut ed, "c");
    f.pii.borrow_mut().get_mut("pii_record:7").unwrap().revealed = true;
    ed.pii_display_changed();
    ctrl(&mut ed, "c");
    caret_at(&mut ed, 3, 0);
    set_clip_text(&f, format!("pasted {marker} ").repeat(1000));
    ctrl(&mut ed, "v");
    ctrl(&mut ed, "z");
    ctrl(&mut ed, "y");
    ctrl(&mut ed, "x");
    ctrl(&mut ed, "z");
    press(&mut ed, NamedKey::Backspace);
    press(&mut ed, NamedKey::Delete);
    cmd(&mut ed, Command::RemoveBlock(bid(2)));
    ctrl(&mut ed, "z");
    ctrl(&mut ed, "s");
    if let Some(r) = f.saves.borrow().last().cloned() {
        finish(&mut ed, r.id, false);
    }
    advance(&f, Duration::from_secs(120));
    ed.poll();
    if let Some(r) = f.saves.borrow().last().cloned() {
        finish(&mut ed, r.id, true);
    }
    ed.remote_change(RemoteChange { content: doc1(&format!("remote {marker}")), device: "phone".into() });
    type_text(&mut ed, marker);
    ed.editing_moved_to("phone");
    if let Some(r) = f.saves.borrow().last().cloned() {
        finish(&mut ed, r.id, true);
    }
    type_text(&mut ed, marker);
    paint(&mut ed);
    let _ = ed.accessibility_tree();
    let _ = ed.request_close();
    out.extend(ed.take_notices().into_iter().map(|n| n.message));
    out.push(format!("{ed:?}"));
    out.push(format!("{:?}", ed.document()));
    for b in ed.document().blocks() {
        out.push(format!("{b:?}"));
    }
    out.push(format!("{:?}", ed.save_status()));
    out.push(format!("{:?}", ed.read_only_reason()));
    out.push(format!("{:?}", ed.selection()));
    drop(ed);

    // A document over the cap (read-only path).
    let big = doc1(&format!("{marker} ").repeat(1_100_000 / (marker.len() + 1) + 1));
    let (mut ed, _f) = open_focused(&big);
    paint(&mut ed);
    type_text(&mut ed, marker);
    out.extend(ed.take_notices().into_iter().map(|n| n.message));
    out.push(format!("{ed:?}"));
    drop(ed);

    // A newer format version.
    let newer = format!(r#"{{"format":"sovereign.doc","v":9,"blocks":[],"s":"{marker}"}}"#);
    let (mut ed, _f) = open_focused(&newer);
    type_text(&mut ed, marker);
    out.extend(ed.take_notices().into_iter().map(|n| n.message));
    out.push(format!("{ed:?}"));
    drop(ed);

    // A password field.
    let (h, _f) = host();
    let mut field = Editor::open_field(FieldKind::Password, config(), h);
    field.handle_event(InputEvent::Focus(true));
    type_text(&mut field, marker);
    let _ = field.accessibility_tree();
    paint(&mut field);
    out.push(format!("{field:?}"));
    drop(field);

    // Something that is not readable at all.
    let broken = format!(r#"{{"format":"sovereign.doc","v":2,"blocks":"{marker}"}}"#);
    let (h2, _f2) = host();
    match Editor::open(
        sovereign_editor::OpenDocument {
            title: "t".into(),
            content: broken.clone(),
            trust: sovereign_editor::Trust::Owned,
            mode: sovereign_editor::OpenMode::Edit,
        },
        config(),
        h2,
    ) {
        Ok(ed) => out.push(format!("{ed:?}")),
        Err(e) => out.push(format!("{} {:?}", e.message, e)),
    }
    if let Err(e) = sovereign_editor::doc::DocumentV2::read(&broken) {
        out.push(format!("{} {:?}", e.message, e));
    }
    out
}
