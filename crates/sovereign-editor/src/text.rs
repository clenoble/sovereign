//! Text rules shared by the model and the view: grapheme clusters (UAX #29),
//! personal-data tokens, the split of a text block into paragraphs, bidi
//! direction marks for the pieces of a long paragraph, and the mapping between
//! the stored text and what is displayed.
//!
//! Text is stored exactly as typed. Whether text in token form
//! `[pii:<record_id>]` is a token is the host's answer (the resolver rule,
//! decided 2026-10-10: a token only in a document among its record's
//! sources), passed in as `is_token` / `resolve`.

use icu_properties::props::BidiClass;
use icu_properties::CodePointMapData;
use unicode_segmentation::GraphemeCursor;

/// What text in token form starts with (ch. 09).
pub(crate) const TOKEN_OPEN: &str = "[pii:";
/// Longest record id taken as a token.
const MAX_ID: usize = 256;
/// Longest token, `[pii:` + id + `]`.
pub(crate) const MAX_TOKEN: usize = TOKEN_OPEN.len() + MAX_ID + 1;

/// The host's answer: is the text in token form with this record id a token
/// in the open document?
pub(crate) type IsToken<'a> = &'a dyn Fn(&str) -> bool;

/// If text in token form starts at `i`, its end (exclusive).
pub(crate) fn token_form_end(text: &str, i: usize) -> Option<usize> {
    if !text[i..].starts_with(TOKEN_OPEN) {
        return None;
    }
    let from = i + TOKEN_OPEN.len();
    for (j, c) in text[from..].char_indices() {
        if c == ']' {
            return (j > 0).then_some(from + j + 1);
        }
        if j >= MAX_ID || c == '[' || c.is_whitespace() || c.is_control() {
            return None;
        }
    }
    None
}

/// If a token (token form the host says is a token) starts at `i`, its end.
pub(crate) fn token_end_at(text: &str, i: usize, is_token: IsToken<'_>) -> Option<usize> {
    let b = token_form_end(text, i)?;
    is_token(token_id(text, i, b)).then_some(b)
}

/// The record id of the token form `text[a..b]`.
pub(crate) fn token_id(text: &str, a: usize, b: usize) -> &str {
    &text[a + TOKEN_OPEN.len()..b - 1]
}

fn floor_char(text: &str, mut i: usize) -> usize {
    i = i.min(text.len());
    while !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// The token that strictly contains `offset` (`a < offset < b`).
pub(crate) fn token_containing(text: &str, offset: usize, is_token: IsToken<'_>) -> Option<(usize, usize)> {
    if offset == 0 || offset >= text.len() {
        return None;
    }
    let from = floor_char(text, offset.saturating_sub(MAX_TOKEN));
    let window = &text[from..floor_char(text, offset + TOKEN_OPEN.len() - 1)];
    let mut found = None;
    let mut search = 0;
    while let Some(k) = window[search..].find(TOKEN_OPEN) {
        let a = from + search + k;
        if let Some(b) = token_form_end(text, a) {
            if a < offset && offset < b && is_token(token_id(text, a, b)) {
                found = Some((a, b));
            }
        }
        search += k + 1;
    }
    found
}

/// Tokens starting in `from..to`, ending at or before `limit`.
pub(crate) fn tokens_in(text: &str, from: usize, to: usize, limit: usize, is_token: IsToken<'_>) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut i = from;
    while i < to {
        let Some(k) = text[i..to].find(TOKEN_OPEN) else { break };
        let a = i + k;
        match token_end_at(text, a, is_token) {
            Some(b) if b <= limit => {
                out.push((a, b));
                i = b;
            }
            _ => i = a + 1,
        }
    }
    out
}

// ------------------------------------------------------------ graphemes

pub(crate) fn is_grapheme_boundary(text: &str, off: usize) -> bool {
    if off == 0 || off >= text.len() {
        return true;
    }
    if !text.is_char_boundary(off) {
        return false;
    }
    GraphemeCursor::new(off, text.len(), true).is_boundary(text, 0).unwrap_or(true)
}

pub(crate) fn next_grapheme(text: &str, off: usize) -> Option<usize> {
    if off >= text.len() {
        return None;
    }
    let off = floor_char(text, off);
    GraphemeCursor::new(off, text.len(), true).next_boundary(text, 0).ok().flatten()
}

pub(crate) fn prev_grapheme(text: &str, off: usize) -> Option<usize> {
    if off == 0 {
        return None;
    }
    let mut off = off.min(text.len());
    while !text.is_char_boundary(off) {
        off += 1;
    }
    GraphemeCursor::new(off, text.len(), true).prev_boundary(text, 0).ok().flatten()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Snap {
    Floor,
    Ceil,
    Nearest,
}

/// The valid caret position for `off` in a text block: a grapheme cluster
/// boundary, never strictly inside a personal-data token.
pub(crate) fn snap_caret(text: &str, off: usize, snap: Snap, is_token: IsToken<'_>) -> usize {
    let off = off.min(text.len());
    let mut p = if is_grapheme_boundary(text, off) {
        off
    } else {
        let lo = prev_grapheme(text, off).unwrap_or(0);
        let hi = next_grapheme(text, lo).unwrap_or(text.len());
        match snap {
            Snap::Floor => lo,
            Snap::Ceil => hi,
            Snap::Nearest => {
                if off - lo <= hi - off {
                    lo
                } else {
                    hi
                }
            }
        }
    };
    if let Some((a, b)) = token_containing(text, p, is_token) {
        p = match snap {
            Snap::Floor => a,
            Snap::Ceil => b,
            Snap::Nearest => {
                if p - a <= b - p {
                    a
                } else {
                    b
                }
            }
        };
    }
    p
}

/// The next valid caret position after `off`.
pub(crate) fn next_caret(text: &str, off: usize, is_token: IsToken<'_>) -> Option<usize> {
    if off < text.len() && text.is_char_boundary(off) {
        if let Some(b) = token_end_at(text, off, is_token) {
            return Some(b);
        }
    }
    let n = next_grapheme(text, off)?;
    Some(match token_containing(text, n, is_token) {
        Some((_, b)) => b,
        None => n,
    })
}

/// The previous valid caret position before `off`.
pub(crate) fn prev_caret(text: &str, off: usize, is_token: IsToken<'_>) -> Option<usize> {
    let p = prev_grapheme(text, off)?;
    Some(match token_containing(text, p, is_token) {
        Some((a, _)) => a,
        None => p,
    })
}

// ------------------------------------------------------------ paragraphs

/// A part of a text block laid out as one unit: a paragraph (text between
/// line breaks), or a piece of a long one. `start..end` excludes the line
/// break; `term` is its length after `end` (0, 1 for `\n`, 2 for `\r\n`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UnitSpan {
    pub start: usize,
    pub end: usize,
    pub term: usize,
}

/// Paragraphs longer than this are laid out piece by piece, each piece
/// starting where a line of the whole paragraph starts (see `view`).
pub(crate) const MAX_UNIT: usize = 4096;

/// Splits `text[from..to]` into paragraphs. `from` is a paragraph start; `to`
/// is the end of the text or a place right after a line break.
pub(crate) fn split_paragraphs(text: &str, from: usize, to: usize, out: &mut Vec<UnitSpan>) {
    let mut i = from;
    loop {
        let (end, term, next) = match text[i..to].find('\n') {
            Some(k) => {
                let nl = i + k;
                if nl > i && text.as_bytes()[nl - 1] == b'\r' {
                    (nl - 1, 2, nl + 1)
                } else {
                    (nl, 1, nl + 1)
                }
            }
            None => (to, 0, to),
        };
        out.push(UnitSpan { start: i, end, term });
        if term == 0 {
            break;
        }
        i = next;
        if i == to {
            if to == text.len() {
                out.push(UnitSpan { start: to, end: to, term: 0 });
            }
            break;
        }
    }
}

// ------------------------------------------------------------ bidi

/// A strong bidi direction (UAX #9 classes L, R, AL).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Strong {
    L,
    R,
    Al,
}

fn strong_of(c: char) -> Option<Strong> {
    let class = CodePointMapData::<BidiClass>::new().get(c);
    if class == BidiClass::LeftToRight {
        Some(Strong::L)
    } else if class == BidiClass::RightToLeft {
        Some(Strong::R)
    } else if class == BidiClass::ArabicLetter {
        Some(Strong::Al)
    } else {
        None
    }
}

/// The first strong direction in `s` (what sets a paragraph's direction).
pub(crate) fn first_strong(s: &str) -> Option<Strong> {
    s.chars().find_map(strong_of)
}

/// The last strong direction in `s`.
pub(crate) fn last_strong(s: &str) -> Option<Strong> {
    s.chars().rev().find_map(strong_of)
}

/// The invisible marks put before a piece of a long paragraph that is not its
/// first, so that the piece resolves like the same text inside the whole
/// paragraph: one mark giving the paragraph's direction (bidi takes it from
/// the first strong character), one giving the last strong direction before
/// the piece (what neutrals and numbers at its start look back to).
pub(crate) fn piece_marks(paragraph_rtl: bool, before: Option<Strong>) -> String {
    let mut s = String::from(if paragraph_rtl { "\u{200F}" } else { "\u{200E}" });
    match before {
        Some(Strong::L) => s.push('\u{200E}'),
        Some(Strong::R) => s.push('\u{200F}'),
        Some(Strong::Al) => s.push('\u{061C}'),
        None => {}
    }
    s
}

// ------------------------------------------------------------ display mapping

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SegKind {
    /// Shown as stored, byte for byte.
    Plain,
    /// A token (or a masked character): shown as something else, one unit.
    Atomic,
    /// Shown, not stored (input-method pre-edit).
    Preedit,
    /// Shown, not stored, invisible: bidi marks before a piece.
    Mark,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Seg {
    pub m0: usize,
    pub m1: usize,
    pub d0: usize,
    pub d1: usize,
    pub kind: SegKind,
}

/// The displayed text of one unit and how it maps to the stored text.
#[derive(Clone, Debug, Default)]
pub(crate) struct DisplayMap {
    pub text: String,
    pub segs: Vec<Seg>,
    /// The unit's stored range.
    pub m_start: usize,
    pub m_end: usize,
    /// Whether some token is shown in it.
    pub has_tokens: bool,
}

pub(crate) struct DisplayInput<'a> {
    pub text: &'a str,
    pub start: usize,
    pub end: usize,
    /// Shows every cluster as a bullet (password field).
    pub mask: bool,
    /// Pre-edit text shown at a stored offset.
    pub preedit: Option<(usize, &'a str)>,
    /// Invisible marks shown first (`piece_marks`).
    pub marks: &'a str,
}

/// How a token's record id is shown: `Some(label or value)` for a token,
/// `None` when the host says the token form is ordinary text here.
pub(crate) type Resolve<'a> = &'a dyn Fn(&str) -> Option<String>;

impl DisplayMap {
    pub(crate) fn build(input: DisplayInput<'_>, resolve: Resolve<'_>) -> DisplayMap {
        let DisplayInput { text, start, end, mask, preedit, marks } = input;
        let mut map = DisplayMap { m_start: start, m_end: end, ..Default::default() };
        if !marks.is_empty() {
            map.push(SegKind::Mark, start, start, marks);
        }
        let mut pre = preedit.filter(|(p, _)| *p >= start && *p <= end);
        let mut i = start;
        if mask {
            while i < end {
                if let Some((p, s)) = pre {
                    if p == i {
                        map.push(SegKind::Preedit, i, i, s);
                        pre = None;
                    }
                }
                let n = next_grapheme(text, i).unwrap_or(end).min(end);
                map.push(SegKind::Atomic, i, n, "\u{2022}");
                i = n;
            }
            if let Some((_, s)) = pre {
                map.push(SegKind::Preedit, end, end, s);
            }
            return map;
        }
        while i < end {
            // the next token in `i..end`, if any
            let mut search = i;
            let mut token = None;
            while let Some(k) = text[search..end].find(TOKEN_OPEN) {
                let a = search + k;
                if let Some(b) = token_form_end(text, a).filter(|&b| b <= end) {
                    if let Some(shown) = resolve(token_id(text, a, b)) {
                        token = Some((a, b, shown));
                        break;
                    }
                }
                search = a + 1;
            }
            let plain_end = token.as_ref().map(|t| t.0).unwrap_or(end);
            if plain_end > i {
                map.push_plain(text, i, plain_end, &mut pre);
            }
            let Some((a, b, shown)) = token else { break };
            if let Some((p, s)) = pre {
                if p == a {
                    map.push(SegKind::Preedit, a, a, s);
                    pre = None;
                }
            }
            map.push(SegKind::Atomic, a, b, &shown);
            map.has_tokens = true;
            i = b;
        }
        if let Some((_, s)) = pre {
            map.push(SegKind::Preedit, end, end, s);
        }
        if map.segs.iter().all(|s| s.kind == SegKind::Mark) {
            let d = map.text.len();
            map.segs.push(Seg { m0: start, m1: start, d0: d, d1: d, kind: SegKind::Plain });
        }
        map
    }

    fn push(&mut self, kind: SegKind, m0: usize, m1: usize, shown: &str) {
        let d0 = self.text.len();
        self.text.push_str(shown);
        self.segs.push(Seg { m0, m1, d0, d1: self.text.len(), kind });
    }

    fn push_plain(&mut self, text: &str, a: usize, b: usize, pre: &mut Option<(usize, &str)>) {
        if let Some((p, s)) = *pre {
            if p >= a && p < b {
                if p > a {
                    self.push(SegKind::Plain, a, p, &text[a..p]);
                }
                self.push(SegKind::Preedit, p, p, s);
                *pre = None;
                self.push(SegKind::Plain, p, b, &text[p..b]);
                return;
            }
        }
        self.push(SegKind::Plain, a, b, &text[a..b]);
    }

    /// Stored offset → displayed offset (a caret after any marks, before any
    /// pre-edit there).
    pub(crate) fn m2d(&self, m: usize) -> usize {
        for s in &self.segs {
            if m >= s.m0 && m <= s.m1 {
                return match s.kind {
                    SegKind::Plain => s.d0 + (m - s.m0),
                    SegKind::Atomic => {
                        if m <= s.m0 {
                            s.d0
                        } else {
                            s.d1
                        }
                    }
                    SegKind::Preedit => s.d0,
                    SegKind::Mark => continue,
                };
            }
        }
        self.text.len()
    }

    /// Displayed offset → stored offset.
    pub(crate) fn d2m(&self, d: usize) -> usize {
        for s in &self.segs {
            if d >= s.d0 && d < s.d1 {
                return match s.kind {
                    SegKind::Plain => s.m0 + (d - s.d0),
                    SegKind::Atomic => {
                        if d - s.d0 < s.d1 - d {
                            s.m0
                        } else {
                            s.m1
                        }
                    }
                    SegKind::Preedit | SegKind::Mark => s.m0,
                };
            }
        }
        self.m_end
    }

    /// The stored text before this unit grew or shrank by `delta` bytes: the
    /// unit's own text is unchanged, its offsets move.
    pub(crate) fn shift(&mut self, delta: isize) {
        let mv = |x: &mut usize| *x = (*x as isize + delta) as usize;
        mv(&mut self.m_start);
        mv(&mut self.m_end);
        for s in &mut self.segs {
            mv(&mut s.m0);
            mv(&mut s.m1);
        }
    }

    /// The displayed range of the pre-edit, if one is shown.
    pub(crate) fn preedit_range(&self) -> Option<(usize, usize)> {
        self.segs.iter().find(|s| s.kind == SegKind::Preedit).map(|s| (s.d0, s.d1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_follow_the_host() {
        let t = "a[pii:pii_record:3]b";
        let yes: IsToken<'_> = &|_| true;
        let no: IsToken<'_> = &|_| false;
        assert_eq!(token_end_at(t, 1, yes), Some(19));
        assert_eq!(token_end_at(t, 1, no), None);
        assert_eq!(token_containing(t, 5, yes), Some((1, 19)));
        assert_eq!(token_containing(t, 5, no), None);
        assert_eq!(token_containing(t, 1, yes), None);
        assert_eq!(snap_caret(t, 5, Snap::Nearest, yes), 1);
        assert_eq!(snap_caret(t, 5, Snap::Nearest, no), 5);
        assert_eq!(next_caret(t, 1, yes), Some(19));
        assert_eq!(next_caret(t, 1, no), Some(2));
        assert_eq!(prev_caret(t, 19, yes), Some(1));
    }

    #[test]
    fn paragraphs() {
        let mut v = vec![];
        split_paragraphs("a\r\nb\n", 0, 5, &mut v);
        assert_eq!(
            v,
            vec![
                UnitSpan { start: 0, end: 1, term: 2 },
                UnitSpan { start: 3, end: 4, term: 1 },
                UnitSpan { start: 5, end: 5, term: 0 }
            ]
        );
        let mut v = vec![];
        split_paragraphs("", 0, 0, &mut v);
        assert_eq!(v, vec![UnitSpan { start: 0, end: 0, term: 0 }]);
    }

    #[test]
    fn display_map() {
        let t = "m [pii:r:1] [pii:x] n";
        let map = DisplayMap::build(
            DisplayInput { text: t, start: 0, end: t.len(), mask: false, preedit: None, marks: "" },
            &|id| (id == "r:1").then(|| "<L>".to_string()),
        );
        assert_eq!(map.text, "m <L> [pii:x] n");
        assert_eq!(map.m2d(2), 2);
        assert_eq!(map.m2d(11), 5);
        assert_eq!(map.d2m(5), 11);
        let marked = DisplayMap::build(
            DisplayInput { text: "abc", start: 0, end: 3, mask: false, preedit: None, marks: "\u{200E}" },
            &|_| None,
        );
        assert_eq!(marked.m2d(0), 3);
        assert_eq!(marked.d2m(0), 0);
    }

    #[test]
    fn strong_directions() {
        assert_eq!(first_strong("12 abc"), Some(Strong::L));
        assert_eq!(last_strong("abc שלום 42"), Some(Strong::R));
        assert_eq!(last_strong("abc مرحبا"), Some(Strong::Al));
        assert_eq!(first_strong("12 ,"), None);
    }
}
