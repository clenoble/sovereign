//! A small JSON scanner that keeps byte spans. **Not a public API**: shared
//! with the editor (`sovereign-editor`, which moved its provisional model
//! here) and hidden from the docs; it may change without notice.
//!
//! The v2 format asks that unchanged blocks keep their bytes (ch. 03, *Stable
//! serialisation*): key order, whitespace and number formatting as written.
//! A generic JSON library parses into values and writes them back in its own
//! form, so the model keeps the stored text and only *locates* things in it:
//! this module validates JSON and returns where members and elements are.
//! It decodes and encodes strings itself. Errors carry no text.

use std::ops::Range;

/// The input is not valid JSON at some point. Carries no content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Invalid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Object,
    Array,
    String,
    Number,
    Literal,
}

/// One member of an object: decoded key, and the span of its value.
pub struct Member {
    pub key: String,
    pub value: Range<usize>,
    pub kind: Kind,
}

pub fn skip_ws(s: &[u8], mut i: usize) -> usize {
    while i < s.len() && matches!(s[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    i
}

fn kind_at(s: &[u8], i: usize) -> Result<Kind, Invalid> {
    match s.get(i) {
        Some(b'{') => Ok(Kind::Object),
        Some(b'[') => Ok(Kind::Array),
        Some(b'"') => Ok(Kind::String),
        Some(b'-' | b'0'..=b'9') => Ok(Kind::Number),
        Some(b't' | b'f' | b'n') => Ok(Kind::Literal),
        _ => Err(Invalid),
    }
}

/// Skips a string starting at `i` (on the opening quote); returns the index
/// after the closing quote.
fn skip_string(s: &[u8], mut i: usize) -> Result<usize, Invalid> {
    debug_assert_eq!(s.get(i), Some(&b'"'));
    i += 1;
    loop {
        match s.get(i) {
            None => return Err(Invalid),
            Some(b'"') => return Ok(i + 1),
            Some(b'\\') => {
                match s.get(i + 1) {
                    Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => i += 2,
                    Some(b'u') => {
                        let hex = s.get(i + 2..i + 6).ok_or(Invalid)?;
                        if !hex.iter().all(|c| c.is_ascii_hexdigit()) {
                            return Err(Invalid);
                        }
                        i += 6;
                    }
                    _ => return Err(Invalid),
                }
            }
            Some(&c) if c < 0x20 => return Err(Invalid),
            Some(_) => i += 1,
        }
    }
}

fn skip_number(s: &[u8], mut i: usize) -> Result<usize, Invalid> {
    if s.get(i) == Some(&b'-') {
        i += 1;
    }
    match s.get(i) {
        Some(b'0') => i += 1,
        Some(b'1'..=b'9') => {
            while matches!(s.get(i), Some(b'0'..=b'9')) {
                i += 1;
            }
        }
        _ => return Err(Invalid),
    }
    if s.get(i) == Some(&b'.') {
        i += 1;
        if !matches!(s.get(i), Some(b'0'..=b'9')) {
            return Err(Invalid);
        }
        while matches!(s.get(i), Some(b'0'..=b'9')) {
            i += 1;
        }
    }
    if matches!(s.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(s.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if !matches!(s.get(i), Some(b'0'..=b'9')) {
            return Err(Invalid);
        }
        while matches!(s.get(i), Some(b'0'..=b'9')) {
            i += 1;
        }
    }
    Ok(i)
}

fn skip_literal(s: &[u8], i: usize) -> Result<usize, Invalid> {
    for lit in [&b"true"[..], b"false", b"null"] {
        if s.get(i..i + lit.len()) == Some(lit) {
            return Ok(i + lit.len());
        }
    }
    Err(Invalid)
}

/// Skips one value starting at `i` (no leading whitespace); returns the index
/// after it. Iterative, so deep nesting cannot overflow the stack.
pub fn skip_value(s: &[u8], i: usize) -> Result<usize, Invalid> {
    // Stack of open containers: true = object, false = array.
    let mut stack: Vec<bool> = Vec::new();
    let mut i = i;
    loop {
        // Parse one value at i.
        match kind_at(s, i)? {
            Kind::String => i = skip_string(s, i)?,
            Kind::Number => i = skip_number(s, i)?,
            Kind::Literal => i = skip_literal(s, i)?,
            Kind::Array => {
                i = skip_ws(s, i + 1);
                if s.get(i) == Some(&b']') {
                    i += 1;
                } else {
                    stack.push(false);
                    continue;
                }
            }
            Kind::Object => {
                i = skip_ws(s, i + 1);
                if s.get(i) == Some(&b'}') {
                    i += 1;
                } else {
                    if s.get(i) != Some(&b'"') {
                        return Err(Invalid);
                    }
                    i = skip_string(s, i)?;
                    i = skip_ws(s, i);
                    if s.get(i) != Some(&b':') {
                        return Err(Invalid);
                    }
                    i = skip_ws(s, i + 1);
                    stack.push(true);
                    continue;
                }
            }
        }
        // A value ended at i: close containers or move to the next element.
        loop {
            let Some(&is_obj) = stack.last() else {
                return Ok(i);
            };
            i = skip_ws(s, i);
            match s.get(i) {
                Some(b',') => {
                    i = skip_ws(s, i + 1);
                    if is_obj {
                        if s.get(i) != Some(&b'"') {
                            return Err(Invalid);
                        }
                        i = skip_string(s, i)?;
                        i = skip_ws(s, i);
                        if s.get(i) != Some(&b':') {
                            return Err(Invalid);
                        }
                        i = skip_ws(s, i + 1);
                    }
                    break;
                }
                Some(b'}') if is_obj => {
                    stack.pop();
                    i += 1;
                }
                Some(b']') if !is_obj => {
                    stack.pop();
                    i += 1;
                }
                _ => return Err(Invalid),
            }
        }
    }
}

/// Validates that `s` is exactly one JSON value with optional surrounding
/// whitespace; returns the value's span.
pub fn whole_value(s: &str) -> Result<(Kind, Range<usize>), Invalid> {
    let b = s.as_bytes();
    let start = skip_ws(b, 0);
    let kind = kind_at(b, start)?;
    let end = skip_value(b, start)?;
    if skip_ws(b, end) != b.len() {
        return Err(Invalid);
    }
    Ok((kind, start..end))
}

/// The members of the object at `span` (which must be a valid object).
pub fn members(s: &str, span: Range<usize>) -> Result<Vec<Member>, Invalid> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = skip_ws(b, span.start + 1);
    if b.get(i) == Some(&b'}') {
        return Ok(out);
    }
    loop {
        if b.get(i) != Some(&b'"') {
            return Err(Invalid);
        }
        let key_end = skip_string(b, i)?;
        let key = decode_string(&s[i..key_end]).ok_or(Invalid)?;
        i = skip_ws(b, key_end);
        if b.get(i) != Some(&b':') {
            return Err(Invalid);
        }
        i = skip_ws(b, i + 1);
        let kind = kind_at(b, i)?;
        let end = skip_value(b, i)?;
        out.push(Member { key, value: i..end, kind });
        i = skip_ws(b, end);
        match b.get(i) {
            Some(b',') => i = skip_ws(b, i + 1),
            Some(b'}') => return Ok(out),
            _ => return Err(Invalid),
        }
    }
}

/// The elements of the array at `span`: each element's span, plus the
/// whitespace after `[` (`lead`), the separators between elements, and the
/// whitespace before `]` (`trail`).
pub struct Elements {
    pub elems: Vec<(Kind, Range<usize>)>,
    pub lead: Range<usize>,
    pub seps: Vec<Range<usize>>,
    pub trail: Range<usize>,
}

pub fn elements(s: &str, span: Range<usize>) -> Result<Elements, Invalid> {
    let b = s.as_bytes();
    let open = span.start + 1;
    let mut i = skip_ws(b, open);
    let lead = open..i;
    let mut out = Elements { elems: vec![], lead, seps: vec![], trail: i..i };
    if b.get(i) == Some(&b']') {
        out.trail = i..i;
        return Ok(out);
    }
    loop {
        let kind = kind_at(b, i)?;
        let end = skip_value(b, i)?;
        out.elems.push((kind, i..end));
        let j = skip_ws(b, end);
        match b.get(j) {
            Some(b',') => {
                let k = skip_ws(b, j + 1);
                out.seps.push(end..k);
                i = k;
            }
            Some(b']') => {
                out.trail = end..j;
                return Ok(out);
            }
            _ => return Err(Invalid),
        }
    }
}

/// Decodes a JSON string literal (with its quotes). `None` if it is not a
/// valid literal or holds a lone surrogate.
pub fn decode_string(lit: &str) -> Option<String> {
    let b = lit.as_bytes();
    if b.len() < 2 || b[0] != b'"' || b[b.len() - 1] != b'"' {
        return None;
    }
    let inner = &lit[1..lit.len() - 1];
    if !inner.contains('\\') {
        return Some(inner.to_string());
    }
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.char_indices();
    let ib = inner.as_bytes();
    while let Some((i, c)) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let (_, e) = chars.next()?;
        match e {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            '/' => out.push('/'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'u' => {
                let hex = |at: usize| -> Option<u32> {
                    let h = inner.get(at..at + 4)?;
                    u32::from_str_radix(h, 16).ok()
                };
                let hi = hex(i + 2)?;
                for _ in 0..4 {
                    chars.next()?;
                }
                if (0xD800..0xDC00).contains(&hi) {
                    // must be followed by \uDC00..DFFF
                    let at = i + 6;
                    if ib.get(at) != Some(&b'\\') || ib.get(at + 1) != Some(&b'u') {
                        return None;
                    }
                    let lo = hex(at + 2)?;
                    if !(0xDC00..0xE000).contains(&lo) {
                        return None;
                    }
                    for _ in 0..6 {
                        chars.next()?;
                    }
                    let cp = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                    out.push(char::from_u32(cp)?);
                } else if (0xDC00..0xE000).contains(&hi) {
                    return None;
                } else {
                    out.push(char::from_u32(hi)?);
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

/// Encodes `s` as a JSON string literal: `"` and `\` and control characters
/// escaped, everything else written as is (non-ASCII raw, `/` unescaped).
pub fn encode_string(s: &str, out: &mut String) {
    out.reserve(s.len() + 2);
    out.push('"');
    let mut last = 0;
    for (i, c) in s.char_indices() {
        let esc: Option<&str> = match c {
            '"' => Some("\\\""),
            '\\' => Some("\\\\"),
            '\n' => Some("\\n"),
            '\r' => Some("\\r"),
            '\t' => Some("\\t"),
            '\u{8}' => Some("\\b"),
            '\u{c}' => Some("\\f"),
            c if (c as u32) < 0x20 => None,
            _ => continue,
        };
        out.push_str(&s[last..i]);
        match esc {
            Some(e) => out.push_str(e),
            None => {
                let v = c as u32;
                const HEX: &[u8; 16] = b"0123456789abcdef";
                out.push_str("\\u00");
                out.push(HEX[(v >> 4) as usize] as char);
                out.push(HEX[(v & 0xF) as usize] as char);
            }
        }
        last = i + c.len_utf8();
    }
    out.push_str(&s[last..]);
    out.push('"');
}

#[cfg(test)]
pub fn encoded(s: &str) -> String {
    let mut out = String::new();
    encode_string(s, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_round_trip() {
        for s in ["", "a\"b\\c\n\r\t\u{1}é👍/", "\u{1f}"] {
            let e = encoded(s);
            assert_eq!(decode_string(&e).as_deref(), Some(s));
        }
        assert_eq!(decode_string(r#""😀""#).as_deref(), Some("😀"));
        assert_eq!(decode_string(r#""\ud83d""#), None);
        assert_eq!(decode_string(r#""café \/""#).as_deref(), Some("café /"));
    }

    #[test]
    fn values() {
        assert!(whole_value(" {\"a\":[1,2.5e-3,{\"b\":null}],\"c\":\"x\"} ").is_ok());
        assert!(whole_value("{not json").is_err());
        assert!(whole_value("[1,]").is_err());
        assert!(whole_value("01").is_err());
        let deep = "[".repeat(100_000) + &"]".repeat(100_000);
        assert!(whole_value(&deep).is_ok());
    }
}
