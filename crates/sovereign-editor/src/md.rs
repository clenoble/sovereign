//! Light markdown formatting (spec ch. 12, C-TEXT behaviours: "markdown source
//! in `text` blocks, shown with light formatting (headings, emphasis, lists,
//! links) while the source stays editable").
//!
//! Nothing here changes the text. The source is what is laid out, edited,
//! selected, saved and read by screen readers, byte for byte; this module only
//! says how each displayed byte is *drawn*:
//!
//! - an **ink** per byte ([`Ink`]): the markup characters (`#`, `*`, `_`,
//!   `` ` ``, `[`, `](url)`, list markers…) dimmed, link text in the link
//!   colour and underlined, strong and emphasis drawn bold and slanted. Inks
//!   are parley brushes: they never change shaping, so line breaks, caret
//!   places and hit testing are exactly those of the plain source (bold and
//!   slant are synthesised when painting, see `view::draw_layout`);
//! - a **heading level** per paragraph (ATX headings, `#` to `######`), the only
//!   formatting that changes the layout (size and weight). It depends on the
//!   paragraph's first bytes only, so every piece of a long paragraph gets the
//!   same one and the pieces still show the lines of the whole paragraph.
//!
//! The rules are a small, local subset of CommonMark, computed per unit (a
//! paragraph, or a piece of a long one): code spans, backslash escapes, links
//! `[text](dest)` and `![alt](src)`, autolinks `<scheme:…>`, bare `http(s)://`
//! URLs, `*`/`_` emphasis and strong with the flanking rules, and line
//! prefixes (headings, block quotes, list markers and task boxes, thematic
//! breaks, code fences). Not drawn: setext headings, the inside of fenced code
//! blocks (a fence line is dimmed, the lines between are not known to be
//! code), emphasis that spans two pieces of a paragraph longer than
//! `text::MAX_UNIT`, reference links, HTML.

/// How a displayed byte is drawn. The parley brush of the editor's layouts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Ink(pub u8);

impl Ink {
    // Role bits 0: ordinary text.
    /// Role bits: markdown markup (dimmed).
    pub(crate) const MARK: u8 = 1;
    /// Role bits: link text or a URL.
    pub(crate) const LINK: u8 = 2;
    const ROLE: u8 = 0x0f;
    pub(crate) const STRONG: u8 = 0x10;
    pub(crate) const EM: u8 = 0x20;
    pub(crate) const UNDERLINE: u8 = 0x40;

    pub(crate) fn role(self) -> u8 {
        self.0 & Ink::ROLE
    }
    pub(crate) fn has(self, flag: u8) -> bool {
        self.0 & flag != 0
    }
}

/// Size factor of a heading of `level` (1–6) relative to body text.
pub(crate) fn heading_scale(level: u8) -> f32 {
    match level {
        1 => 1.6,
        2 => 1.4,
        3 => 1.25,
        4 => 1.1,
        _ => 1.0,
    }
}

/// The ATX heading level (1–6) of a paragraph starting with `para`, or 0.
/// Only the first ten bytes matter.
pub(crate) fn heading_level(para: &[u8]) -> u8 {
    let mut i = 0;
    while i < 3 && para.get(i) == Some(&b' ') {
        i += 1;
    }
    let mut n = 0;
    while para.get(i + n) == Some(&b'#') {
        n += 1;
    }
    if n == 0 || n > 6 {
        return 0;
    }
    match para.get(i + n) {
        None | Some(b' ' | b'\t' | b'\n' | b'\r') => n as u8,
        _ => 0,
    }
}

/// The inks of a unit's displayed text `src`, as `(start, end, ink)` ranges
/// (displayed offsets, ordinary text left out). `body` is where the unit's
/// own text starts (after invisible marks); `line_start`: the unit starts a
/// paragraph, so line prefixes apply. Bytes the caller replaced (tokens,
/// pre-edit, marks) should be ASCII letters, so they take part as text.
pub(crate) fn inks(src: &[u8], body: usize, line_start: bool, out: &mut Vec<(usize, usize, Ink)>) {
    let mut ink = vec![0u8; src.len()];
    // bytes no inline rule may use (code, link destinations, URLs, escaped)
    let mut opaque = vec![false; src.len()];
    let mut pos = body.min(src.len());
    if line_start {
        match line_prefix(src, pos, &mut ink) {
            Prefix::Whole => {
                ranges(&ink, out);
                return;
            }
            Prefix::Rest(p) => pos = p,
        }
    }
    code_and_escapes(src, pos, &mut ink, &mut opaque);
    links(src, pos, &mut ink, &mut opaque);
    emphasis(src, pos, &mut ink, &opaque);
    ranges(&ink, out);
}

fn ranges(ink: &[u8], out: &mut Vec<(usize, usize, Ink)>) {
    let mut i = 0;
    while i < ink.len() {
        let v = ink[i];
        let mut j = i + 1;
        while j < ink.len() && ink[j] == v {
            j += 1;
        }
        if v != 0 {
            out.push((i, j, Ink(v)));
        }
        i = j;
    }
}

fn set_role(ink: &mut [u8], a: usize, b: usize, role: u8) {
    for x in &mut ink[a..b] {
        *x = (*x & !Ink::ROLE) | role;
    }
}

fn add_flags(ink: &mut [u8], a: usize, b: usize, flags: u8) {
    for x in &mut ink[a..b] {
        *x |= flags;
    }
}

enum Prefix {
    /// The whole line is markup (thematic break, code fence).
    Whole,
    /// Inline rules apply from here.
    Rest(usize),
}

fn line_prefix(s: &[u8], start: usize, ink: &mut [u8]) -> Prefix {
    let n = s.len();
    let first = (start..n).find(|&i| s[i] != b' ' && s[i] != b'\t').unwrap_or(n);
    let rest = &s[first..];
    // thematic break: three or more of one of - * _ (spaces allowed), nothing else
    if let Some(&c) = rest.first() {
        if matches!(c, b'-' | b'*' | b'_') && rest.iter().all(|&b| b == c || b == b' ' || b == b'\t') && rest.iter().filter(|&&b| b == c).count() >= 3 {
            set_role(ink, first, n, Ink::MARK);
            return Prefix::Whole;
        }
    }
    // code fence
    if rest.starts_with(b"```") || rest.starts_with(b"~~~") {
        set_role(ink, first, n, Ink::MARK);
        return Prefix::Whole;
    }
    // heading
    let level = heading_level(&s[start..]);
    if level > 0 {
        let h = (start..n).find(|&i| s[i] == b'#').unwrap_or(n);
        let e = h + level as usize;
        set_role(ink, h, e, Ink::MARK);
        // a closing sequence of #s (preceded by a space) is markup too
        let mut t = n;
        while t > e && matches!(s[t - 1], b' ' | b'\t') {
            t -= 1;
        }
        let mut c = t;
        while c > e && s[c - 1] == b'#' {
            c -= 1;
        }
        if c < t && c > e && matches!(s[c - 1], b' ' | b'\t') {
            set_role(ink, c, t, Ink::MARK);
            return Prefix::Rest(e);
        }
        return Prefix::Rest(e);
    }
    // block quotes, then a list marker
    let p = line_parts(s, first);
    for &q in &p.quotes {
        set_role(ink, q, q + 1, Ink::MARK);
    }
    let mut i = p.marker_start;
    if let Some(item) = p.item {
        set_role(ink, item.marker_start, item.marker_end, Ink::MARK);
        add_flags(ink, item.marker_start, item.marker_end, Ink::STRONG);
        i = item.marker_end;
        if let Some((b0, b1, _)) = item.task {
            set_role(ink, b0, b1, Ink::MARK);
            i = b1;
        } else {
            i += item.gap.len();
        }
    }
    Prefix::Rest(i)
}

/// Block quote markers, and a list item, at the start of a line.
struct LineParts {
    /// Offsets of the `>` markers.
    quotes: Vec<usize>,
    /// Where a list marker would start (after indentation and quotes).
    marker_start: usize,
    item: Option<ListItem>,
}

/// A list item at the start of a line: what drawing dims and what Enter
/// continues (spec ch. 12, *Lists continue on Enter*).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ListItem {
    pub marker_start: usize,
    pub marker_end: usize,
    pub kind: ListKind,
    /// The spaces / tabs between the marker and the box or the content.
    pub gap: Vec<u8>,
    /// A task box `[ ]` / `[x]`: its range, and the spaces after it.
    pub task: Option<(usize, usize, Vec<u8>)>,
    /// Where the item's content starts.
    pub content_start: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ListKind {
    /// `-`, `*` or `+`.
    Bullet(u8),
    /// A number (1 to 9 digits) and its delimiter, `.` or `)`.
    Ordered(u64, u8),
}

fn line_parts(s: &[u8], first: usize) -> LineParts {
    let n = s.len();
    let mut quotes = vec![];
    let mut i = first;
    while i < n && s[i] == b'>' {
        quotes.push(i);
        i += 1;
        while i < n && (s[i] == b' ' || s[i] == b'\t') {
            i += 1;
        }
    }
    let marker_start = i;
    let (kind, marker_end) = if i < n && matches!(s[i], b'-' | b'+' | b'*') && (i + 1 == n || matches!(s[i + 1], b' ' | b'\t')) {
        (ListKind::Bullet(s[i]), i + 1)
    } else {
        let d = (i..n).take_while(|&k| s[k].is_ascii_digit()).count();
        let k = i + d;
        if (1..=9).contains(&d) && k < n && matches!(s[k], b'.' | b')') && (k + 1 == n || matches!(s[k + 1], b' ' | b'\t')) {
            let num = std::str::from_utf8(&s[i..k]).ok().and_then(|t| t.parse::<u64>().ok()).unwrap_or(0);
            (ListKind::Ordered(num, s[k]), k + 1)
        } else {
            return LineParts { quotes, marker_start, item: None };
        }
    };
    let mut j = marker_end;
    while j < n && (s[j] == b' ' || s[j] == b'\t') {
        j += 1;
    }
    let gap = s[marker_end..j].to_vec();
    // task box: [ ] or [x]
    let task = if j + 2 < n && s[j] == b'[' && matches!(s[j + 1], b' ' | b'x' | b'X') && s[j + 2] == b']' {
        let b1 = j + 3;
        let mut k = b1;
        while k < n && (s[k] == b' ' || s[k] == b'\t') {
            k += 1;
        }
        let after = s[b1..k].to_vec();
        j = k;
        Some((b1 - 3, b1, after))
    } else {
        None
    };
    LineParts { quotes, marker_start, item: Some(ListItem { marker_start, marker_end, kind, gap, task, content_start: j }) }
}

/// The list item a line starts with, if any: not a heading, a thematic break
/// or a fence (what drawing treats as such), else block quotes then a marker
/// as `line_prefix` sees them.
pub(crate) fn list_item(line: &[u8]) -> Option<ListItem> {
    let n = line.len();
    let first = (0..n).find(|&i| line[i] != b' ' && line[i] != b'\t').unwrap_or(n);
    let rest = &line[first..];
    if let Some(&c) = rest.first() {
        if matches!(c, b'-' | b'*' | b'_')
            && rest.iter().all(|&b| b == c || b == b' ' || b == b'\t')
            && rest.iter().filter(|&&b| b == c).count() >= 3
        {
            return None;
        }
    }
    if rest.starts_with(b"```") || rest.starts_with(b"~~~") || heading_level(line) > 0 {
        return None;
    }
    line_parts(line, first).item
}

/// A code fence line (up to three spaces of indentation, then three or more
/// `` ` `` or `~`): its character and length, and whether something other than
/// spaces follows (an info string; a closing fence has none).
pub(crate) fn fence(line: &[u8]) -> Option<(u8, usize, bool)> {
    let indent = line.iter().take_while(|&&b| b == b' ').count();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let c = *rest.first()?;
    if c != b'`' && c != b'~' {
        return None;
    }
    let run = rest.iter().take_while(|&&b| b == c).count();
    if run < 3 {
        return None;
    }
    let info = rest[run..].iter().any(|&b| !matches!(b, b' ' | b'\t' | b'\r'));
    Some((c, run, info))
}

/// Code spans (content opaque, backticks dimmed) and backslash escapes (the
/// backslash dimmed, the escaped character opaque).
fn code_and_escapes(s: &[u8], from: usize, ink: &mut [u8], opaque: &mut [bool]) {
    let n = s.len();
    let mut i = from;
    while i < n {
        match s[i] {
            b'\\' if i + 1 < n && s[i + 1].is_ascii_punctuation() => {
                set_role(ink, i, i + 1, Ink::MARK);
                opaque[i] = true;
                opaque[i + 1] = true;
                i += 2;
            }
            b'`' => {
                let run = s[i..].iter().take_while(|&&b| b == b'`').count();
                let mut j = i + run;
                let mut close = None;
                while j < n {
                    if s[j] == b'`' {
                        let r = s[j..].iter().take_while(|&&b| b == b'`').count();
                        if r == run {
                            close = Some(j);
                            break;
                        }
                        j += r;
                    } else {
                        j += 1;
                    }
                }
                match close {
                    Some(c) => {
                        set_role(ink, i, i + run, Ink::MARK);
                        set_role(ink, c, c + run, Ink::MARK);
                        for o in &mut opaque[i..c + run] {
                            *o = true;
                        }
                        i = c + run;
                    }
                    None => {
                        for o in &mut opaque[i..i + run] {
                            *o = true;
                        }
                        i += run;
                    }
                }
            }
            _ => i += 1,
        }
    }
}

/// Links, images, autolinks and bare URLs.
fn links(s: &[u8], from: usize, ink: &mut [u8], opaque: &mut [bool]) {
    let n = s.len();
    let mut i = from;
    while i < n {
        if opaque[i] {
            i += 1;
            continue;
        }
        match s[i] {
            b'[' => {
                if let Some((close, dest_end)) = link_at(s, i, opaque) {
                    let bang = i > from && s[i - 1] == b'!' && !opaque[i - 1];
                    let a = if bang { i - 1 } else { i };
                    set_role(ink, a, i + 1, Ink::MARK);
                    set_role(ink, i + 1, close, Ink::LINK);
                    add_flags(ink, i + 1, close, Ink::UNDERLINE);
                    set_role(ink, close, dest_end, Ink::MARK);
                    for o in &mut opaque[close..dest_end] {
                        *o = true;
                    }
                    opaque[a] = true;
                    opaque[i] = true;
                    i = dest_end;
                    continue;
                }
                i += 1;
            }
            b'<' => {
                // <scheme:…> with no space or '<' inside
                let e = (i + 1..n).find(|&k| matches!(s[k], b'>' | b'<' | b' ' | b'\t') || opaque[k]);
                if let Some(e) = e.filter(|&e| s[e] == b'>') {
                    let inner = &s[i + 1..e];
                    let colon = inner.iter().position(|&b| b == b':');
                    let scheme_ok = colon.is_some_and(|c| {
                        (2..=32).contains(&c) && inner[0].is_ascii_alphabetic() && inner[..c].iter().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-'))
                    });
                    if scheme_ok {
                        set_role(ink, i, i + 1, Ink::MARK);
                        set_role(ink, e, e + 1, Ink::MARK);
                        set_role(ink, i + 1, e, Ink::LINK);
                        add_flags(ink, i + 1, e, Ink::UNDERLINE);
                        for o in &mut opaque[i..=e] {
                            *o = true;
                        }
                        i = e + 1;
                        continue;
                    }
                }
                i += 1;
            }
            b'h' if (i == from || matches!(s[i - 1], b' ' | b'\t' | b'(' | b'"' | b'\'')) && (s[i..].starts_with(b"https://") || s[i..].starts_with(b"http://")) => {
                let mut e = (i..n).find(|&k| s[k].is_ascii_whitespace() || s[k] == b'<' || opaque[k]).unwrap_or(n);
                while e > i && matches!(s[e - 1], b'.' | b',' | b':' | b';' | b'!' | b'?' | b')' | b'\'' | b'"' | b'*' | b'_') {
                    e -= 1;
                }
                let scheme_len = if s[i..].starts_with(b"https://") { 8 } else { 7 };
                if e > i + scheme_len {
                    set_role(ink, i, e, Ink::LINK);
                    add_flags(ink, i, e, Ink::UNDERLINE);
                    for o in &mut opaque[i..e] {
                        *o = true;
                    }
                    i = e;
                    continue;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
}

/// `[text](dest)` starting at `open`: the index of `]` and the end of `)`.
fn link_at(s: &[u8], open: usize, opaque: &[bool]) -> Option<(usize, usize)> {
    let n = s.len();
    let mut depth = 0usize;
    let mut k = open + 1;
    let close = loop {
        if k >= n {
            return None;
        }
        if !opaque[k] {
            match s[k] {
                b'[' => depth += 1,
                b']' if depth == 0 => break k,
                b']' => depth -= 1,
                _ => {}
            }
        }
        k += 1;
    };
    if close + 1 >= n || s[close + 1] != b'(' || opaque[close + 1] {
        return None;
    }
    let mut par = 0usize;
    let mut k = close + 2;
    while k < n {
        match s[k] {
            b'(' => par += 1,
            b')' if par == 0 => return Some((close, k + 1)),
            b')' => par -= 1,
            _ => {}
        }
        k += 1;
    }
    None
}

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Space,
    Punct,
    Other,
}

fn class_before(s: &[u8], i: usize) -> Class {
    if i == 0 {
        return Class::Space;
    }
    // the character ending at i (UTF-8: back to its lead byte)
    let mut a = i - 1;
    while a > 0 && (s[a] & 0xC0) == 0x80 {
        a -= 1;
    }
    class_of(&s[a..i])
}

fn class_after(s: &[u8], i: usize) -> Class {
    if i >= s.len() {
        return Class::Space;
    }
    let len = match s[i] {
        b if b < 0x80 => 1,
        b if b >= 0xF0 => 4,
        b if b >= 0xE0 => 3,
        _ => 2,
    };
    class_of(&s[i..(i + len).min(s.len())])
}

fn class_of(ch: &[u8]) -> Class {
    let Some(c) = std::str::from_utf8(ch).ok().and_then(|t| t.chars().next()) else { return Class::Other };
    if c.is_whitespace() {
        Class::Space
    } else if c.is_ascii_punctuation() || is_unicode_punct(c) {
        Class::Punct
    } else {
        Class::Other
    }
}

/// Common Unicode punctuation (General Punctuation, CJK punctuation, fullwidth
/// forms) — enough for the flanking rules.
fn is_unicode_punct(c: char) -> bool {
    matches!(c as u32, 0x2010..=0x2027 | 0x2030..=0x205E | 0x3000..=0x303F | 0xFF01..=0xFF0F | 0xFF1A..=0xFF20 | 0xFF3B..=0xFF40 | 0xFF5B..=0xFF65 | 0x00A1..=0x00BF)
}

struct Delim {
    ch: u8,
    start: usize,
    end: usize,
    /// Unused delimiter characters: `start..start+left` for an opener,
    /// `end-left..end` for a closer.
    left: usize,
    len: usize,
    can_open: bool,
    can_close: bool,
}

/// `*` and `_` emphasis (one character) and strong (two), CommonMark's
/// delimiter-run rules (flanking, the rule of three), within the unit.
fn emphasis(s: &[u8], from: usize, ink: &mut [u8], opaque: &[bool]) {
    let n = s.len();
    let mut stack: Vec<Delim> = vec![];
    let mut i = from;
    while i < n {
        let c = s[i];
        if opaque[i] || (c != b'*' && c != b'_') {
            i += 1;
            continue;
        }
        let mut e = i;
        while e < n && s[e] == c && !opaque[e] {
            e += 1;
        }
        let before = class_before(s, i);
        let after = class_after(s, e);
        let left = after != Class::Space && (after != Class::Punct || before != Class::Other);
        let right = before != Class::Space && (before != Class::Punct || after != Class::Other);
        let (can_open, can_close) = if c == b'*' {
            (left, right)
        } else {
            (left && (!right || before == Class::Punct), right && (!left || after == Class::Punct))
        };
        let mut d = Delim { ch: c, start: i, end: e, left: e - i, len: e - i, can_open, can_close };
        if d.can_close {
            loop {
                if d.left == 0 {
                    break;
                }
                let Some(oi) = stack.iter().rposition(|o| {
                    o.ch == d.ch
                        && o.left > 0
                        && !((o.can_close || d.can_open) && (o.len + d.len).is_multiple_of(3) && !(o.len.is_multiple_of(3) && d.len.is_multiple_of(3)))
                }) else {
                    break;
                };
                let o = &mut stack[oi];
                let used = if o.left >= 2 && d.left >= 2 { 2 } else { 1 };
                let o_inner = o.start + o.left;
                let c_inner = d.end - d.left;
                set_role(ink, o_inner - used, o_inner, Ink::MARK);
                set_role(ink, c_inner, c_inner + used, Ink::MARK);
                add_flags(ink, o_inner, c_inner, if used == 2 { Ink::STRONG } else { Ink::EM });
                o.left -= used;
                d.left -= used;
                // openers between the pair can no longer close anything here
                stack.truncate(oi + 1);
                if stack[oi].left == 0 {
                    stack.pop();
                }
            }
        }
        if d.can_open && d.left > 0 {
            stack.push(d);
        }
        i = e;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The unit's text with each byte's role/flags written as a letter:
    /// '.' text, '/' emphasised, 'B' strong, 'X' both; 'm' markup ('M' when
    /// drawn bold: list markers); 'l' link text (underlined).
    fn show(t: &str) -> String {
        let mut out = vec![];
        inks(t.as_bytes(), 0, true, &mut out);
        let mut v = vec![Ink(0); t.len()];
        for (a, b, k) in out {
            for x in &mut v[a..b] {
                *x = k;
            }
        }
        v.iter()
            .map(|k| match (k.role(), k.has(Ink::STRONG), k.has(Ink::EM)) {
                (Ink::MARK, true, _) => 'M',
                (Ink::MARK, false, _) => 'm',
                (Ink::LINK, _, _) => {
                    assert!(k.has(Ink::UNDERLINE));
                    'l'
                }
                (_, true, true) => 'X',
                (_, true, false) => 'B',
                (_, false, true) => '/',
                _ => '.',
            })
            .collect()
    }

    #[test]
    fn headings_and_their_markers() {
        assert_eq!(heading_level(b"# Title"), 1);
        assert_eq!(heading_level(b"### Three"), 3);
        assert_eq!(heading_level(b"   ## x"), 2);
        assert_eq!(heading_level(b"#"), 1);
        assert_eq!(heading_level(b"####### seven"), 0);
        assert_eq!(heading_level(b"#hashtag"), 0);
        assert_eq!(heading_level(b"    # code"), 0);
        assert_eq!(show("## Title ##"), "mm.......mm");
    }

    #[test]
    fn emphasis_strong_and_their_delimiters() {
        assert_eq!(show("a *b* c"), "..m/m..");
        // delimiters are markup; what they enclose is strong (B) or both (X)
        assert_eq!(show("a **b** c"), "..mmBmm..");
        assert_eq!(show("__x__"), "mmBmm");
        assert_eq!(show("***both***"), "mmmXXXXmmm");
        // intraword underscore is not emphasis; intraword star is
        assert_eq!(show("snake_case_name"), "...............");
        assert_eq!(show("un*frigging*believable"), format!("..m{}m{}", "/".repeat(8), ".".repeat(10)));
        // a lone star is text
        assert_eq!(show("2 * 3 = 6"), ".........");
    }

    #[test]
    fn code_spans_hide_markup() {
        assert_eq!(show("`*not em*` *em*"), "m........m.m//m");
        assert_eq!(show(r"\*not\*"), "m....m.");
    }

    #[test]
    fn links_autolinks_and_urls() {
        assert_eq!(show("[site](https://x.org) ok"), format!("m{}{}...", "l".repeat(4), "m".repeat(16)));
        assert_eq!(show("![alt](a.png)"), format!("mm{}{}", "l".repeat(3), "m".repeat(8)));
        assert_eq!(show("<https://a.b>"), format!("m{}m", "l".repeat(11)));
        assert_eq!(show("see https://a.org/x."), format!("....{}.", "l".repeat(15)));
        // an underscore in a URL is not emphasis
        assert_eq!(show("http://a.b/c_d_e f_g_"), format!("{}.....", "l".repeat(16)));
        // emphasis inside link text
        assert_eq!(show("[*a*](u)"), "mmlmmmmm");
    }

    #[test]
    fn list_markers_quotes_rules_and_fences() {
        assert_eq!(show("- item"), "M.....");
        assert_eq!(show("12. item"), "MMM.....");
        assert_eq!(show("* [x] done"), "M.mmm.....");
        assert_eq!(show("> quote"), "m......");
        assert_eq!(show("---"), "mmm");
        assert_eq!(show("```rust"), "mmmmmmm");
        // not a list: no space after the marker
        assert_eq!(show("-x"), "..");
    }

    // What Enter continues is what drawing dims as a list marker.
    #[test]
    fn list_items_for_continuation() {
        let it = list_item(b"  > 12) [x]  done").expect("item");
        assert_eq!((it.marker_start, it.marker_end), (4, 7));
        assert_eq!(it.kind, ListKind::Ordered(12, b')'));
        assert_eq!(it.gap, b" ");
        assert_eq!(it.task, Some((8, 11, b"  ".to_vec())));
        assert_eq!(it.content_start, 13);
        assert_eq!(list_item(b"- ").map(|i| i.content_start), Some(2));
        for not in [&b"---"[..], b"# - x", b"```", b"1.5 l", b"-x", b"1234567890. ten digits"] {
            assert!(list_item(not).is_none(), "{:?}", std::str::from_utf8(not));
        }
        assert_eq!(fence(b"   ~~~~ info"), Some((b'~', 4, true)));
        assert_eq!(fence(b"    ```"), None);
        assert_eq!(fence(b"``"), None);
    }

    #[test]
    fn a_piece_that_does_not_start_a_paragraph_has_no_line_prefix() {
        let mut out = vec![];
        inks(b"# not a heading here", 0, false, &mut out);
        assert!(out.is_empty());
    }
}
