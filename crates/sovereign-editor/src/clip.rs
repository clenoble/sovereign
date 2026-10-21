//! The Sovereign clipboard payload (decided 2026-10-10): beside the plain
//! text every application reads, a copy that holds personal-data tokens
//! carries the tokens themselves, so a paste inside Sovereign recreates them
//! — and the editor then asks the host to link those records to the target
//! document (`PiiDisplay::link_to_document`), which is what makes them tokens
//! there under the resolver rule.
//!
//! That makes the payload the one way text can become a token in a document
//! it was not already linked to, and it travels on the system clipboard,
//! where any application can write: a crafted payload would link any record
//! to any document (W17). It is therefore accepted only if **this process
//! wrote it and it is still the clipboard's current item**:
//! - every payload carries a fresh 128-bit random nonce;
//! - the editor keeps, in memory only, the exact payload and plain text of
//!   the last copy made on this thread (all editors of the shell live on its
//!   UI thread; the editor is not `Send`); a copy without tokens forgets it;
//! - a paste uses the payload only when what the clipboard holds now is
//!   byte-for-byte that payload beside that same plain text.
//!
//! Anything else — a payload from another application, an altered one, an
//! older one of ours — is ignored: the plain text is pasted and nothing is
//! linked. Nothing is persisted; no key is involved. The payload never holds
//! a revealed value: only record ids and the text around them.
//!
//! What this does not stop: an application that reads our payload off the
//! clipboard and writes back the identical bytes, while it is still our last
//! copy. That replays the user's own copy (the same records, into wherever
//! the user pastes); it cannot name a record the user did not copy.

use std::cell::RefCell;

use crate::host::ClipboardContent;
use crate::json::{self, Kind};

const MAGIC: &str = "sovereign-clipboard/1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    /// Text, as stored.
    Text(String),
    /// A personal-data token, by record id.
    Token(String),
}

thread_local! {
    static LAST: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
}

/// Builds a payload for `parts` and remembers it with the plain `text` it is
/// written beside.
pub(crate) fn make_payload(parts: &[Part], text: &str) -> String {
    let mut out = String::from(MAGIC);
    out.push(' ');
    out.push_str(&crate::ids::random_hex128());
    out.push(' ');
    out.push('[');
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let (tag, s) = match p {
            Part::Text(s) => ("t", s),
            Part::Token(s) => ("p", s),
        };
        out.push_str("[\"");
        out.push_str(tag);
        out.push_str("\",");
        json::encode_string(s, &mut out);
        out.push(']');
    }
    out.push(']');
    LAST.with(|l| *l.borrow_mut() = Some((out.clone(), text.to_string())));
    out
}

/// A copy without a payload replaces the clipboard item: forget the last
/// payload.
pub(crate) fn forget() {
    LAST.with(|l| *l.borrow_mut() = None);
}

/// The parts of the clipboard's payload, if it is the one this process wrote
/// last and the clipboard still holds it.
pub(crate) fn accept(content: &ClipboardContent) -> Option<Vec<Part>> {
    let payload = content.sovereign.as_deref()?;
    let ours = LAST.with(|l| {
        l.borrow().as_ref().is_some_and(|(p, t)| p == payload && t == &content.text)
    });
    if !ours {
        return None;
    }
    decode(payload)
}

fn decode(payload: &str) -> Option<Vec<Part>> {
    let rest = payload.strip_prefix(MAGIC)?.strip_prefix(' ')?;
    let (_nonce, list) = rest.split_once(' ')?;
    let (kind, span) = json::whole_value(list).ok()?;
    if kind != Kind::Array {
        return None;
    }
    let els = json::elements(list, span).ok()?;
    let mut parts = vec![];
    for (k, s) in els.elems {
        if k != Kind::Array {
            return None;
        }
        let pair = json::elements(list, s).ok()?;
        if pair.elems.len() != 2 || pair.elems.iter().any(|(k, _)| *k != Kind::String) {
            return None;
        }
        let tag = json::decode_string(&list[pair.elems[0].1.clone()])?;
        let val = json::decode_string(&list[pair.elems[1].1.clone()])?;
        parts.push(match tag.as_str() {
            "t" => Part::Text(val),
            "p" => Part::Token(val),
            _ => return None,
        });
    }
    Some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_our_current_payload_is_accepted() {
        let parts = vec![Part::Text("a ".into()), Part::Token("pii_record:1".into())];
        let p = make_payload(&parts, "a <label>");
        let c = ClipboardContent { text: "a <label>".into(), sovereign: Some(p.clone()) };
        assert_eq!(accept(&c), Some(parts));
        let forged = p.replace("pii_record:1", "pii_record:2");
        assert_eq!(accept(&ClipboardContent { text: c.text.clone(), sovereign: Some(forged) }), None);
        assert_eq!(accept(&ClipboardContent { text: "other".into(), sovereign: Some(p) }), None);
    }
}
