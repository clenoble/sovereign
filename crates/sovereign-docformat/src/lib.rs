//! The stored document format v2 (contract C-ENC, spec ch. 03, *Document
//! format v2 — blocks* and *Document service — contract (C-DOC)*).
//!
//! This is the editor's provisional model (`sovereign-editor`, `src/doc.rs`
//! and `src/json.rs`), moved here so the editor and the document service share
//! one model. The editor re-exports it as `sovereign_editor::doc`. No I/O, no
//! dependencies.
//!
//! What this crate holds to (tested in `crates/sovereign-docs/tests/` and in
//! the editor's suite):
//! - **every stored form is read**: v2, the v1 envelope `{"body","images",
//!   "videos"}`, a raw-text body, the empty string; anything that is not one of
//!   the first two is raw text, never an error and never an empty body;
//! - a v2 document of a **newer** `v` is read for display only and written back
//!   byte for byte; a document claiming `"format":"sovereign.doc"` whose blocks
//!   cannot be read is a [`FormatError`], never raw text;
//! - **one form is written**: v2;
//! - unknown block types, unknown fields and unknown top-level fields survive a
//!   write exactly; unchanged blocks keep their bytes: the model keeps the
//!   stored bytes and only splices a changed value into its block;
//! - **migration is deterministic**: [`DocumentV2::read_seeded`] with the same
//!   seed (the document id) gives the same bytes on every device;
//! - personal-data tokens are `[pii:<record_id>]` (record ids contain `:`),
//!   stored exactly as typed; whether token-form text is a token is decided
//!   elsewhere (the resolver rule);
//! - `Debug` never prints document text.
//!
//! Items marked `#[doc(hidden)]` exist for the editor, which edits blocks in
//! place; they are not part of the format's API.

use std::collections::HashSet;
use std::fmt;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[doc(hidden)]
pub mod json;

use json::Kind;

/// The value of `format` in a v2 document.
pub const FORMAT: &str = "sovereign.doc";

/// The highest format version this crate reads and writes.
pub const KNOWN_VERSION: u64 = 2;

const CANONICAL_HEAD: &str = r#"{"format":"sovereign.doc","v":2,"blocks":["#;
const CANONICAL_TAIL: &str = "]}";

// ------------------------------------------------------------------ ids --

mod ids {
    //! Random numbers from the standard library only: `RandomState` is seeded
    //! from the operating system's random source and changes on every
    //! construction. No file, no network, nothing kept.
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    pub(crate) fn random_u64() -> u64 {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
        h.finish()
    }
}

/// A block's stable 128-bit identifier, written as 32 hex digits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BlockId(pub [u8; 16]);

impl BlockId {
    /// Parses the stored hex form (32 hex digits, either case).
    pub fn from_hex(hex: &str) -> Option<BlockId> {
        let b = hex.as_bytes();
        if b.len() != 32 {
            return None;
        }
        let mut out = [0u8; 16];
        for (i, pair) in b.chunks(2).enumerate() {
            let hi = (pair[0] as char).to_digit(16)?;
            let lo = (pair[1] as char).to_digit(16)?;
            out[i] = (hi * 16 + lo) as u8;
        }
        Some(BlockId(out))
    }

    /// The stored hex form (lowercase).
    pub fn to_hex(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut s = String::with_capacity(32);
        for b in self.0 {
            s.push(HEX[(b >> 4) as usize] as char);
            s.push(HEX[(b & 0xF) as usize] as char);
        }
        s
    }

    /// The id given to the `index`-th block produced by migrating a legacy
    /// document whose migration seed is `seed` (the document id).
    /// Deterministic: same seed and index, same id, on every device and every
    /// version of this crate (FNV-1a, 128 bits, over a fixed domain string,
    /// the seed and the index).
    pub fn derived(seed: &str, index: usize) -> BlockId {
        const OFFSET: u128 = 0x6c62272e07bb014262b821756295c58d;
        const PRIME: u128 = 0x0000000001000000000000000000013B;
        let mut h = OFFSET;
        let mut eat = |bytes: &[u8]| {
            for &b in bytes {
                h ^= b as u128;
                h = h.wrapping_mul(PRIME);
            }
        };
        eat(b"sovereign.doc/block-id/v1\0");
        eat(seed.as_bytes());
        eat(b"\0");
        eat(&(index as u64).to_le_bytes());
        BlockId(h.to_be_bytes())
    }

    /// A fresh random id (the editor's new blocks).
    #[doc(hidden)]
    pub fn random() -> BlockId {
        let a = ids::random_u64().to_le_bytes();
        let b = ids::random_u64().to_le_bytes();
        let mut out = [0u8; 16];
        out[..8].copy_from_slice(&a);
        out[8..].copy_from_slice(&b);
        BlockId(out)
    }
}

/// Which stored form a read found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceForm {
    /// `{"format":"sovereign.doc","v":2,…}`.
    V2,
    /// `{"format":"sovereign.doc","v":N,…}` with N higher than [`KNOWN_VERSION`]:
    /// display only, never rewritten.
    NewerVersion { v: u64 },
    /// The v1 envelope `{"body","images","videos"}`.
    V1Envelope,
    /// Anything else that is not empty: one `text` block holding the whole string.
    RawText,
    /// The empty string.
    Empty,
}

impl SourceForm {
    /// A form written before v2 (v1 envelope, raw text, empty): converted at
    /// the document's next write.
    pub fn is_legacy(&self) -> bool {
        matches!(self, SourceForm::V1Envelope | SourceForm::RawText | SourceForm::Empty)
    }
}

/// A stored string that claims to be a Sovereign document but cannot be read
/// (`"format":"sovereign.doc"` with no version, a version below 2, or blocks
/// that are not a list). Such a document is opened read-only and never
/// rewritten.
#[derive(Clone, PartialEq, Eq)]
pub struct FormatError {
    /// Never contains document text.
    pub message: String,
}

impl fmt::Debug for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FormatError").field("message", &self.message).finish()
    }
}

fn format_error(message: &str) -> FormatError {
    FormatError { message: message.to_string() }
}

/// Why an in-memory change to a document was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditError {
    /// No block with this id.
    NoSuchBlock(BlockId),
    /// The block exists but is not of the kind the change needs (e.g. setting
    /// the text of an image block).
    WrongKind(BlockId),
    /// The document was read from a newer format version: read-only.
    ReadOnly,
}

static NEXT_REV: AtomicU64 = AtomicU64::new(1);

fn next_rev() -> u64 {
    NEXT_REV.fetch_add(1, Ordering::Relaxed)
}

/// The kind of a block, as this version understands it.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Text,
    Image,
    Video,
    Table,
    Unknown,
}

#[derive(Clone)]
struct BlockData {
    /// The block's stored bytes as read (or as created). A changed text is
    /// spliced into it when writing.
    raw: String,
    /// Span of the `text` value (string literal, quotes included) in `raw`.
    raw_text_span: Option<Range<usize>>,
    id: BlockId,
    type_name: String,
    kind: BlockKind,
    /// For a text block: its current markdown source.
    text: String,
    /// The text differs from what `raw` holds.
    text_dirty: bool,
    markup: Option<String>,
    media_path: Option<String>,
    media_sha256: Option<String>,
    image_alt: Option<String>,
    caption: Option<String>,
    needs_description: bool,
    /// Changes whenever the block changes (views cache by it).
    rev: u64,
}

impl BlockData {
    fn write(&self, out: &mut String) {
        if !self.text_dirty {
            out.push_str(&self.raw);
            return;
        }
        match &self.raw_text_span {
            Some(span) => {
                out.push_str(&self.raw[..span.start]);
                json::encode_string(&self.text, out);
                out.push_str(&self.raw[span.end..]);
            }
            None => {
                // A text block without a `text` member: add one before the
                // closing brace.
                let close = self.raw.rfind('}').unwrap_or(self.raw.len());
                let body = self.raw[..close].trim_end();
                out.push_str(body);
                if !body.ends_with('{') {
                    out.push(',');
                }
                out.push_str("\"text\":");
                json::encode_string(&self.text, out);
                out.push_str(&self.raw[close..]);
            }
        }
    }

    fn written_len_hint(&self) -> usize {
        self.raw.len() + if self.text_dirty { self.text.len() + 16 } else { 0 }
    }

    fn written(&self) -> String {
        let mut s = String::with_capacity(self.written_len_hint());
        self.write(&mut s);
        s
    }
}

/// One block of a v2 document.
pub struct Block(Arc<BlockData>);

impl Block {
    /// The block's stable id.
    pub fn id(&self) -> BlockId {
        self.0.id
    }

    /// The block's `type` string (`"text"`, `"image"`, `"video"`, `"table"`,
    /// or one this version does not know).
    pub fn type_name(&self) -> &str {
        &self.0.type_name
    }

    /// Whether this version knows the block type. Unknown blocks are shown as
    /// a read-only placeholder and preserved exactly.
    pub fn is_known(&self) -> bool {
        self.0.kind != BlockKind::Unknown
    }

    /// For a `text` block, its `text` field: markdown source, personal-data
    /// tokens `[pii:<id>]` included as stored. `None` for other blocks.
    pub fn text(&self) -> Option<&str> {
        (self.0.kind == BlockKind::Text).then_some(self.0.text.as_str())
    }

    /// For a `text` block, its `markup` field (`"markdown"` in v2).
    pub fn markup(&self) -> Option<&str> {
        if self.0.kind != BlockKind::Text {
            return None;
        }
        Some(self.0.markup.as_deref().unwrap_or("markdown"))
    }

    /// For an `image` or `video` block, `ref.path`.
    pub fn media_path(&self) -> Option<&str> {
        match self.0.kind {
            BlockKind::Image | BlockKind::Video => self.0.media_path.as_deref(),
            _ => None,
        }
    }

    /// For an `image` or `video` block, `ref.sha256` — absent until the
    /// document service computes it; the editor never reads media files.
    pub fn media_sha256(&self) -> Option<&str> {
        match self.0.kind {
            BlockKind::Image | BlockKind::Video => self.0.media_sha256.as_deref(),
            _ => None,
        }
    }

    /// For an `image` block, its `alt` text.
    pub fn image_alt(&self) -> Option<&str> {
        match self.0.kind {
            BlockKind::Image => self.0.image_alt.as_deref(),
            _ => None,
        }
    }

    /// For an `image` or `video` block, its `caption`, when it has one.
    pub fn caption(&self) -> Option<&str> {
        match self.0.kind {
            BlockKind::Image | BlockKind::Video => self.0.caption.as_deref(),
            _ => None,
        }
    }

    /// For an `image` block migrated from v1: the description is still to be
    /// written. Set by the v1 migration, which produces `alt: ""` with this
    /// flag (stored as `"needs_description":true`); such a block is valid
    /// while the flag is set.
    pub fn needs_description(&self) -> bool {
        self.0.kind == BlockKind::Image && self.0.needs_description
    }

    /// For a `video` block, its `duration_secs` (a number the v1 migration
    /// keeps from a v1 entry), read from the block's stored bytes. `None` for
    /// other blocks or when absent. Composed of the `json` reader only; added
    /// so readers that still hand out v1 fields (adoption step 1) lose nothing.
    pub fn video_duration_secs(&self) -> Option<f64> {
        if self.0.kind != BlockKind::Video {
            return None;
        }
        let raw = &self.0.raw;
        let (_, span) = json::whole_value(raw).ok()?;
        let members = json::members(raw, span).ok()?;
        let m = members.iter().rev().find(|m| m.key == "duration_secs" && m.kind == Kind::Number)?;
        raw[m.value.clone()].parse::<f64>().ok()
    }

    /// For a `video` block, `thumbnail_ref.path` (the v1 `thumbnail_path`, as
    /// the v1 migration stores it), read from the block's stored bytes.
    pub fn video_thumbnail_path(&self) -> Option<String> {
        if self.0.kind != BlockKind::Video {
            return None;
        }
        let raw = &self.0.raw;
        let (_, span) = json::whole_value(raw).ok()?;
        let members = json::members(raw, span).ok()?;
        let r = members.iter().rev().find(|m| m.key == "thumbnail_ref" && m.kind == Kind::Object)?;
        let inner = json::members(raw, r.value.clone()).ok()?;
        let p = inner.iter().rev().find(|m| m.key == "path" && m.kind == Kind::String)?;
        json::decode_string(&raw[p.value.clone()])
    }

    /// The block's stored form as it would be written now.
    pub fn to_json_string(&self) -> String {
        self.0.written()
    }

    /// Another handle on the same block (shared until one of them changes).
    #[doc(hidden)]
    pub fn share(&self) -> Block {
        Block(Arc::clone(&self.0))
    }

    #[doc(hidden)]
    pub fn kind(&self) -> BlockKind {
        self.0.kind
    }

    /// Changes whenever the block changes (views cache by it).
    #[doc(hidden)]
    pub fn rev(&self) -> u64 {
        self.0.rev
    }

    /// The block's text for editing (empty for non-text blocks).
    #[doc(hidden)]
    pub fn text_str(&self) -> &str {
        &self.0.text
    }

    /// Mutable access to a text block's text; marks the block changed.
    #[doc(hidden)]
    pub fn text_mut(&mut self) -> &mut String {
        let d = Arc::make_mut(&mut self.0);
        d.rev = next_rev();
        d.text_dirty = true;
        &mut d.text
    }

    /// A new `text` block.
    #[doc(hidden)]
    pub fn new_text(id: BlockId, text: String) -> Block {
        let mut raw = String::new();
        raw.push_str(r#"{"type":"text","id":""#);
        raw.push_str(&id.to_hex());
        raw.push_str(r#"","markup":"markdown","text":"#);
        let s = raw.len();
        raw.push_str("\"\"");
        let e = raw.len();
        raw.push('}');
        let dirty = !text.is_empty();
        Block(Arc::new(BlockData {
            raw,
            raw_text_span: Some(s..e),
            id,
            type_name: "text".into(),
            kind: BlockKind::Text,
            text,
            text_dirty: dirty,
            markup: Some("markdown".into()),
            media_path: None,
            media_sha256: None,
            image_alt: None,
            caption: None,
            needs_description: false,
            rev: next_rev(),
        }))
    }
}

impl fmt::Debug for Block {
    /// Never prints document text.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.0.kind {
            BlockKind::Text => "text",
            BlockKind::Image => "image",
            BlockKind::Video => "video",
            BlockKind::Table => "table",
            BlockKind::Unknown => "unknown",
        };
        f.debug_struct("Block")
            .field("id", &self.0.id.to_hex())
            .field("kind", &kind)
            .field("text_bytes", &self.0.text.len())
            .finish()
    }
}

/// How the document's envelope is written around its blocks.
enum Frame {
    /// A v2 envelope: everything before the blocks array's first element,
    /// the separators as read, and everything after.
    V2 {
        head: String,
        lead: String,
        seps: Vec<String>,
        trail: String,
        tail: String,
        /// The array's inner whitespace if it was read empty.
        empty_inner: Option<String>,
    },
    /// A document of a newer format: written back exactly as read.
    Verbatim(String),
}

/// Where the ids of blocks that have none of their own come from.
enum Ids<'a> {
    /// Random (display only: never written by the service).
    Random(HashSet<BlockId>),
    /// Derived from a seed, so every device gets the same bytes.
    Seeded { seed: &'a str, seen: HashSet<BlockId> },
}

impl Ids<'_> {
    fn seen(&mut self) -> &mut HashSet<BlockId> {
        match self {
            Ids::Random(s) => s,
            Ids::Seeded { seen, .. } => seen,
        }
    }

    /// The id of the `index`-th block produced by migrating a legacy form.
    fn migrated(&mut self, index: usize) -> BlockId {
        match self {
            Ids::Random(seen) => fresh_id(seen),
            Ids::Seeded { seed, seen } => {
                let id = BlockId::derived(seed, index);
                seen.insert(id);
                id
            }
        }
    }

    /// An in-memory id for a stored block whose own id is missing, invalid or
    /// a duplicate (never written: the block's bytes are kept as read).
    fn substitute(&mut self, index: usize) -> BlockId {
        match self {
            Ids::Random(seen) => fresh_id(seen),
            Ids::Seeded { seed, seen } => {
                let mut n = 0usize;
                loop {
                    let id = BlockId::derived(&format!("{seed}#stored-block-{index}"), n);
                    if seen.insert(id) {
                        return id;
                    }
                    n += 1;
                }
            }
        }
    }
}

/// A document held in memory, in format v2.
pub struct DocumentV2 {
    blocks: Vec<Block>,
    frame: Arc<Frame>,
}

impl DocumentV2 {
    /// Reads a stored content string in any of the forms of [`SourceForm`].
    /// Blocks produced by migrating a legacy form get **random** in-memory ids
    /// (display only). The document service writes migrations with
    /// [`read_seeded`](Self::read_seeded), never with this.
    pub fn read(stored: &str) -> Result<(DocumentV2, SourceForm), FormatError> {
        read_with(stored, Ids::Random(HashSet::new()))
    }

    /// Like [`read`](Self::read), but blocks produced by migrating a legacy
    /// form (v1 envelope, raw text) get ids [`BlockId::derived`] from `seed`,
    /// so the written v2 bytes are the same on every device. A v2 or newer
    /// document is read exactly as by `read` (its ids are its own).
    pub fn read_seeded(stored: &str, seed: &str) -> Result<(DocumentV2, SourceForm), FormatError> {
        read_with(stored, Ids::Seeded { seed, seen: HashSet::new() })
    }

    /// A document of one `text` block holding `text` (markdown), its id
    /// derived from `seed`. Used for content created from plain text (import,
    /// the assistant, a skill's output on a one-block document).
    pub fn from_text(text: &str, seed: &str) -> DocumentV2 {
        DocumentV2::canonical(vec![Block::new_text(BlockId::derived(seed, 0), text.to_string())])
    }

    fn canonical(blocks: Vec<Block>) -> DocumentV2 {
        DocumentV2 {
            blocks,
            frame: Arc::new(Frame::V2 {
                head: CANONICAL_HEAD.to_string(),
                lead: String::new(),
                seps: vec![],
                trail: String::new(),
                tail: CANONICAL_TAIL.to_string(),
                empty_inner: None,
            }),
        }
    }

    /// A single-block document for a text field (never written anywhere).
    #[doc(hidden)]
    pub fn field() -> DocumentV2 {
        DocumentV2::canonical(vec![Block::new_text(BlockId::random(), String::new())])
    }

    /// The blocks, in document order.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// The blocks, for the editor's in-place edits.
    #[doc(hidden)]
    pub fn blocks_mut(&mut self) -> &mut Vec<Block> {
        &mut self.blocks
    }

    /// The block with this id.
    pub fn block(&self, id: BlockId) -> Option<&Block> {
        self.blocks.iter().find(|b| b.0.id == id)
    }

    #[doc(hidden)]
    pub fn index_of(&self, id: BlockId) -> Option<usize> {
        self.blocks.iter().position(|b| b.0.id == id)
    }

    /// Whether the document can be written: false when it was read from a
    /// newer format version.
    pub fn is_writable(&self) -> bool {
        !matches!(&*self.frame, Frame::Verbatim(_))
    }

    fn writable_block(&mut self, id: BlockId) -> Result<&mut Block, EditError> {
        if !self.is_writable() {
            return Err(EditError::ReadOnly);
        }
        self.blocks.iter_mut().find(|b| b.0.id == id).ok_or(EditError::NoSuchBlock(id))
    }

    /// Replaces the text of one `text` block; the block keeps its id and
    /// position, every other block keeps its bytes. Setting the text it
    /// already has changes nothing (its bytes are kept).
    pub fn set_text(&mut self, id: BlockId, text: &str) -> Result<(), EditError> {
        let b = self.writable_block(id)?;
        if b.0.kind != BlockKind::Text {
            return Err(EditError::WrongKind(id));
        }
        if b.0.text != text {
            let t = b.text_mut();
            t.clear();
            t.push_str(text);
        }
        Ok(())
    }

    /// Inserts a new `text` block at `index` (clamped to the end).
    pub fn insert_text_block(&mut self, index: usize, id: BlockId, text: &str) -> Result<(), EditError> {
        if !self.is_writable() {
            return Err(EditError::ReadOnly);
        }
        let at = index.min(self.blocks.len());
        self.blocks.insert(at, Block::new_text(id, text.to_string()));
        Ok(())
    }

    /// Sets `ref.sha256` of an `image` or `video` block.
    pub fn set_media_sha256(&mut self, id: BlockId, sha256_hex: &str) -> Result<(), EditError> {
        let b = self.writable_block(id)?;
        if !matches!(b.0.kind, BlockKind::Image | BlockKind::Video) {
            return Err(EditError::WrongKind(id));
        }
        if b.0.media_sha256.as_deref() == Some(sha256_hex) {
            return Ok(());
        }
        let raw = b.0.written();
        let new_raw = splice_ref_member(&raw, "sha256", sha256_hex).ok_or(EditError::WrongKind(id))?;
        replace_raw(b, new_raw);
        Ok(())
    }

    /// Sets the `caption` of an `image` or `video` block.
    pub fn set_caption(&mut self, id: BlockId, caption: &str) -> Result<(), EditError> {
        let b = self.writable_block(id)?;
        if !matches!(b.0.kind, BlockKind::Image | BlockKind::Video) {
            return Err(EditError::WrongKind(id));
        }
        if b.0.caption.as_deref() == Some(caption) {
            return Ok(());
        }
        let raw = b.0.written();
        let new_raw = splice_string_member(&raw, "caption", caption).ok_or(EditError::WrongKind(id))?;
        replace_raw(b, new_raw);
        Ok(())
    }

    /// Sets the `alt` of an `image` block.
    pub fn set_image_alt(&mut self, id: BlockId, alt: &str) -> Result<(), EditError> {
        let b = self.writable_block(id)?;
        if b.0.kind != BlockKind::Image {
            return Err(EditError::WrongKind(id));
        }
        if b.0.image_alt.as_deref() == Some(alt) {
            return Ok(());
        }
        let raw = b.0.written();
        let new_raw = splice_string_member(&raw, "alt", alt).ok_or(EditError::WrongKind(id))?;
        replace_raw(b, new_raw);
        Ok(())
    }

    /// The stored form: `{"format":"sovereign.doc","v":2,"blocks":[…]}`, with
    /// unchanged blocks byte-identical to how they were read (or, for a
    /// document read from a newer `v`, the input unchanged).
    pub fn to_json_string(&self) -> String {
        match &*self.frame {
            Frame::Verbatim(s) => s.clone(),
            Frame::V2 { head, lead, seps, trail, tail, empty_inner } => {
                let cap = head.len()
                    + tail.len()
                    + self.blocks.iter().map(|b| b.0.written_len_hint() + 2).sum::<usize>();
                let mut out = String::with_capacity(cap);
                out.push_str(head);
                if self.blocks.is_empty() {
                    if let Some(inner) = empty_inner {
                        out.push_str(inner);
                    }
                } else {
                    out.push_str(lead);
                    for (i, b) in self.blocks.iter().enumerate() {
                        if i > 0 {
                            let sep = seps.get(i - 1).or(seps.first()).map(String::as_str).unwrap_or(",");
                            out.push_str(sep);
                        }
                        b.0.write(&mut out);
                    }
                    out.push_str(trail);
                }
                out.push_str(tail);
                out
            }
        }
    }

    /// The text of the `text` blocks, in order, joined by one blank line
    /// (`"\n\n"`), tokens kept in their stored form `[pii:<id>]`. What search,
    /// the assistant and skills read. Other blocks contribute nothing.
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        let mut first = true;
        for b in self.blocks.iter().filter(|b| b.0.kind == BlockKind::Text) {
            if !first {
                out.push_str("\n\n");
            }
            first = false;
            out.push_str(&b.0.text);
        }
        out
    }

    /// Bytes of text (UTF-8) held in the document's `text` blocks — the
    /// quantity the editor's 1 MB cap is about.
    pub fn text_bytes(&self) -> usize {
        self.blocks.iter().filter(|b| b.0.kind == BlockKind::Text).map(|b| b.0.text.len()).sum()
    }
}

/// The plain text of a stored content string in any form ([`SourceForm`]):
/// [`DocumentV2::read`] then [`DocumentV2::plain_text`] — the text blocks
/// joined by one blank line, tokens kept as `[pii:<id>]`. Empty when the
/// content is unreadable ([`FormatError`]), as the document service's view
/// is. For readers that hold stored content but no document service
/// (search snippets, assistant tools, skills over a snapshot); reading is
/// pure, so this is what `DocumentService::read(..).plain_text()` returns
/// for the same stored string.
pub fn plain_text_of(stored: &str) -> String {
    DocumentV2::read(stored).map(|(doc, _)| doc.plain_text()).unwrap_or_default()
}

impl fmt::Debug for DocumentV2 {
    /// Never prints document text.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DocumentV2")
            .field("blocks", &self.blocks.len())
            .field("text_bytes", &self.text_bytes())
            .finish()
    }
}

fn read_with(stored: &str, mut ids: Ids<'_>) -> Result<(DocumentV2, SourceForm), FormatError> {
    if stored.is_empty() {
        return Ok((DocumentV2::canonical(vec![]), SourceForm::Empty));
    }
    let raw_text = |ids: &mut Ids<'_>| {
        DocumentV2::canonical(vec![Block::new_text(ids.migrated(0), stored.to_string())])
    };
    let Ok((Kind::Object, span)) = json::whole_value(stored) else {
        return Ok((raw_text(&mut ids), SourceForm::RawText));
    };
    let Ok(members) = json::members(stored, span.clone()) else {
        return Ok((raw_text(&mut ids), SourceForm::RawText));
    };
    let get = |k: &str| members.iter().rev().find(|m| m.key == k);
    let is_ours = get("format")
        .filter(|m| m.kind == Kind::String)
        .and_then(|m| json::decode_string(&stored[m.value.clone()]))
        .is_some_and(|f| f == FORMAT);
    if is_ours {
        let v = match get("v") {
            Some(m) if m.kind == Kind::Number => {
                let t = &stored[m.value.clone()];
                if t.bytes().all(|c| c.is_ascii_digit()) {
                    t.parse::<u64>().unwrap_or(u64::MAX)
                } else {
                    return Err(format_error("the document's format version is not a whole number"));
                }
            }
            _ => return Err(format_error("the document has no format version")),
        };
        let blocks_member = get("blocks").filter(|m| m.kind == Kind::Array);
        if v > KNOWN_VERSION {
            // Read what can be read, for display only; write back verbatim.
            let blocks = blocks_member
                .and_then(|m| json::elements(stored, m.value.clone()).ok())
                .map(|els| parse_blocks(stored, &els.elems, &mut ids))
                .unwrap_or_default();
            let doc = DocumentV2 { blocks, frame: Arc::new(Frame::Verbatim(stored.to_string())) };
            return Ok((doc, SourceForm::NewerVersion { v }));
        }
        if v < KNOWN_VERSION {
            return Err(format_error("the document declares a format version this reader does not know"));
        }
        let Some(bm) = blocks_member else {
            return Err(format_error("the document's blocks are not a list"));
        };
        let els = json::elements(stored, bm.value.clone())
            .map_err(|_| format_error("the document's blocks are not readable"))?;
        let blocks = parse_blocks(stored, &els.elems, &mut ids);
        let open = bm.value.start + 1;
        let close = bm.value.end - 1;
        let empty = els.elems.is_empty();
        let frame = Frame::V2 {
            head: stored[..open].to_string(),
            lead: if empty { String::new() } else { stored[els.lead.clone()].to_string() },
            seps: els.seps.iter().map(|r| stored[r.clone()].to_string()).collect(),
            trail: if empty { String::new() } else { stored[els.trail.clone()].to_string() },
            tail: stored[close..].to_string(),
            empty_inner: empty.then(|| stored[open..close].to_string()),
        };
        return Ok((DocumentV2 { blocks, frame: Arc::new(frame) }, SourceForm::V2));
    }
    // The v1 envelope: an object with a string `body`.
    if let Some(body) = get("body").filter(|m| m.kind == Kind::String) {
        if let Some(body_text) = json::decode_string(&stored[body.value.clone()]) {
            let mut blocks = vec![Block::new_text(ids.migrated(0), body_text)];
            for (key, ty) in [("images", "image"), ("videos", "video")] {
                let Some(m) = get(key).filter(|m| m.kind == Kind::Array) else { continue };
                let Ok(els) = json::elements(stored, m.value.clone()) else { continue };
                for (kind, span) in els.elems {
                    let id = ids.migrated(blocks.len());
                    let raw = migrate_media(stored, kind, span, ty, id);
                    let len = raw.len();
                    blocks.push(parse_block(&raw, 0..len, Kind::Object, Some(id), &mut ids, blocks.len()));
                }
            }
            return Ok((DocumentV2::canonical(blocks), SourceForm::V1Envelope));
        }
    }
    Ok((raw_text(&mut ids), SourceForm::RawText))
}

fn fresh_id(seen: &mut HashSet<BlockId>) -> BlockId {
    loop {
        let id = BlockId::random();
        if seen.insert(id) {
            return id;
        }
    }
}

fn parse_blocks(stored: &str, elems: &[(Kind, Range<usize>)], ids: &mut Ids<'_>) -> Vec<Block> {
    elems
        .iter()
        .enumerate()
        .map(|(i, (k, span))| parse_block(stored, span.clone(), *k, None, ids, i))
        .collect()
}

/// Reads one block from `stored[span]`. Never fails: what cannot be read as a
/// known block is an unknown block, kept byte for byte. `forced_id`, when
/// given, is the block's id whatever it stores (a block just migrated, or one
/// whose bytes were just rewritten).
fn parse_block(
    stored: &str,
    span: Range<usize>,
    kind: Kind,
    forced_id: Option<BlockId>,
    ids: &mut Ids<'_>,
    index: usize,
) -> Block {
    let raw = stored[span].to_string();
    let mut data = BlockData {
        raw,
        raw_text_span: None,
        id: BlockId([0; 16]),
        type_name: String::new(),
        kind: BlockKind::Unknown,
        text: String::new(),
        text_dirty: false,
        markup: None,
        media_path: None,
        media_sha256: None,
        image_alt: None,
        caption: None,
        needs_description: false,
        rev: next_rev(),
    };
    let members = if kind == Kind::Object { json::members(&data.raw, 0..data.raw.len()).ok() } else { None };
    let Some(members) = members else {
        data.id = forced_id.unwrap_or_else(|| ids.substitute(index));
        return Block(Arc::new(data));
    };
    let raw = &data.raw;
    let get = |k: &str| members.iter().rev().find(|m| m.key == k);
    let string = |k: &str| get(k).filter(|m| m.kind == Kind::String).and_then(|m| json::decode_string(&raw[m.value.clone()]));
    // id: the stored one when it is valid and unique, else an in-memory one
    // (never written: the block's bytes are kept as read).
    data.id = match forced_id {
        Some(id) => id,
        None => {
            let stored_id = string("id").and_then(|h| BlockId::from_hex(&h)).filter(|id| !ids.seen().contains(id));
            match stored_id {
                Some(id) => {
                    ids.seen().insert(id);
                    id
                }
                None => ids.substitute(index),
            }
        }
    };
    data.type_name = string("type").unwrap_or_default();
    let ref_member = |k: &str| -> Option<(Option<String>, Option<String>)> {
        let m = get(k).filter(|m| m.kind == Kind::Object)?;
        let inner = json::members(raw, m.value.clone()).ok()?;
        let s = |key: &str| {
            inner
                .iter()
                .rev()
                .find(|x| x.key == key && x.kind == Kind::String)
                .and_then(|x| json::decode_string(&raw[x.value.clone()]))
        };
        Some((s("path"), s("sha256")))
    };
    match data.type_name.as_str() {
        "text" => {
            let text = get("text").filter(|m| m.kind == Kind::String);
            let markup_member = get("markup");
            let markup = string("markup");
            let markup_ok = markup_member.is_none() || markup.as_deref() == Some("markdown");
            if let (Some(t), true) = (text, markup_ok) {
                if let Some(decoded) = json::decode_string(&raw[t.value.clone()]) {
                    data.raw_text_span = Some(t.value.clone());
                    data.text = decoded;
                    data.markup = markup;
                    data.kind = BlockKind::Text;
                }
            }
        }
        "image" => {
            if let (Some((Some(path), sha)), Some(alt)) = (ref_member("ref"), string("alt")) {
                data.kind = BlockKind::Image;
                data.media_path = Some(path);
                data.media_sha256 = sha;
                data.image_alt = Some(alt);
                data.caption = string("caption");
                data.needs_description = get("needs_description")
                    .is_some_and(|m| m.kind == Kind::Literal && &raw[m.value.clone()] == "true");
            }
        }
        "video" => {
            if let Some((Some(path), sha)) = ref_member("ref") {
                data.kind = BlockKind::Video;
                data.media_path = Some(path);
                data.media_sha256 = sha;
                data.caption = string("caption");
            }
        }
        "table" => {
            let arr = |k: &str| get(k).is_some_and(|m| m.kind == Kind::Array);
            if arr("columns") && arr("rows") {
                data.kind = BlockKind::Table;
            }
        }
        _ => {}
    }
    Block(Arc::new(data))
}

/// Replaces a block's bytes by `new_raw`, keeping its id.
fn replace_raw(b: &mut Block, new_raw: String) {
    let id = b.0.id;
    let len = new_raw.len();
    let mut ids = Ids::Random(HashSet::new());
    *b = parse_block(&new_raw, 0..len, Kind::Object, Some(id), &mut ids, 0);
}

/// `raw` (one JSON object) with member `key` set to the string `value`: its
/// value replaced in place when present, else appended before the closing
/// brace. Everything else keeps its bytes.
fn splice_string_member(raw: &str, key: &str, value: &str) -> Option<String> {
    let mut lit = String::new();
    json::encode_string(value, &mut lit);
    splice_member(raw, key, &lit)
}

fn splice_member(raw: &str, key: &str, value_json: &str) -> Option<String> {
    let (kind, span) = json::whole_value(raw).ok()?;
    if kind != Kind::Object {
        return None;
    }
    let members = json::members(raw, span.clone()).ok()?;
    if let Some(m) = members.iter().rev().find(|m| m.key == key) {
        return Some(format!("{}{}{}", &raw[..m.value.start], value_json, &raw[m.value.end..]));
    }
    let close = span.end - 1;
    let body = raw[..close].trim_end();
    let mut out = String::with_capacity(raw.len() + key.len() + value_json.len() + 4);
    out.push_str(body);
    if !body.ends_with('{') {
        out.push(',');
    }
    json::encode_string(key, &mut out);
    out.push(':');
    out.push_str(value_json);
    out.push_str(&raw[close..]);
    Some(out)
}

/// `raw` with member `key` of its `ref` object set to the string `value`.
fn splice_ref_member(raw: &str, key: &str, value: &str) -> Option<String> {
    let (_, span) = json::whole_value(raw).ok()?;
    let members = json::members(raw, span).ok()?;
    let r = members.iter().rev().find(|m| m.key == "ref" && m.kind == Kind::Object)?;
    let inner = splice_string_member(&raw[r.value.clone()], key, value)?;
    Some(format!("{}{}{}", &raw[..r.value.start], inner, &raw[r.value.end..]))
}

/// Builds the v2 JSON of a block migrated from a v1 `images` / `videos` entry.
fn migrate_media(stored: &str, kind: Kind, span: Range<usize>, ty: &str, id: BlockId) -> String {
    let entry = (kind == Kind::Object).then(|| json::members(stored, span.clone()).ok()).flatten();
    let mut out = String::new();
    let keep_unknown = |out: &mut String| {
        // Not readable as a v1 entry: keep it, inside a block this version
        // does not know.
        out.push_str(r#"{"type":"x-v1-"#);
        out.push_str(ty);
        out.push_str(r#"","id":""#);
        out.push_str(&id.to_hex());
        out.push_str(r#"","v1":"#);
        out.push_str(&stored[span.clone()]);
        out.push('}');
    };
    let Some(entry) = entry else {
        keep_unknown(&mut out);
        return out;
    };
    let get = |k: &str| entry.iter().rev().find(|m| m.key == k);
    let string = |k: &str| get(k).filter(|m| m.kind == Kind::String).and_then(|m| json::decode_string(&stored[m.value.clone()]));
    let Some(path) = string("path") else {
        keep_unknown(&mut out);
        return out;
    };
    let caption = string("caption").unwrap_or_default();
    out.push_str(r#"{"type":""#);
    out.push_str(ty);
    out.push_str(r#"","id":""#);
    out.push_str(&id.to_hex());
    out.push_str(r#"","ref":{"path":"#);
    json::encode_string(&path, &mut out);
    out.push('}');
    if ty == "image" {
        out.push_str(r#","alt":"","needs_description":true"#);
    }
    out.push_str(r#","caption":"#);
    json::encode_string(&caption, &mut out);
    if ty == "video" {
        if let Some(d) = get("duration_secs").filter(|m| m.kind == Kind::Number) {
            out.push_str(r#","duration_secs":"#);
            out.push_str(&stored[d.value.clone()]);
        }
        if let Some(t) = string("thumbnail_path") {
            out.push_str(r#","thumbnail_ref":{"path":"#);
            json::encode_string(&t, &mut out);
            out.push('}');
        }
    }
    out.push('}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_ids_are_stable_and_distinct() {
        // Pinned: a change here would make two versions of this crate migrate
        // the same legacy document to different bytes.
        assert_eq!(BlockId::derived("document:x", 0), BlockId::derived("document:x", 0));
        assert_ne!(BlockId::derived("document:x", 0), BlockId::derived("document:x", 1));
        assert_ne!(BlockId::derived("document:x", 0), BlockId::derived("document:y", 0));
    }

    #[test]
    fn media_hash_and_caption_splice_keep_the_rest() {
        let s = r#"{"format":"sovereign.doc","v":2,"blocks":[{"type":"image","id":"00000000000000000000000000001002","ref":{"path":"/a.png"},"alt":"a cat","caption":"c"}]}"#;
        let (mut d, _) = DocumentV2::read(s).unwrap();
        let id = d.blocks()[0].id();
        d.set_media_sha256(id, "ab").unwrap();
        d.set_caption(id, "new caption").unwrap();
        d.set_image_alt(id, "a dog").unwrap();
        let b = &d.blocks()[0];
        assert_eq!(b.id(), id);
        assert_eq!(b.media_sha256(), Some("ab"));
        assert_eq!(b.media_path(), Some("/a.png"));
        assert_eq!(b.caption(), Some("new caption"));
        assert_eq!(b.image_alt(), Some("a dog"));
        assert_eq!(
            d.to_json_string(),
            r#"{"format":"sovereign.doc","v":2,"blocks":[{"type":"image","id":"00000000000000000000000000001002","ref":{"path":"/a.png","sha256":"ab"},"alt":"a dog","caption":"new caption"}]}"#
        );
    }

    #[test]
    fn v1_video_fields_survive_the_read() {
        let v1 = r#"{"body":"b","images":[],"videos":[{"path":"/v.mp4","caption":"c","duration_secs":12.5,"thumbnail_path":"/t.png"},{"path":"/w.mp4"}]}"#;
        let (d, form) = DocumentV2::read(v1).unwrap();
        assert_eq!(form, SourceForm::V1Envelope);
        let v = &d.blocks()[1];
        assert_eq!(v.video_duration_secs(), Some(12.5));
        assert_eq!(v.video_thumbnail_path().as_deref(), Some("/t.png"));
        let w = &d.blocks()[2];
        assert_eq!(w.video_duration_secs(), None);
        assert_eq!(w.video_thumbnail_path(), None);
        assert_eq!(d.blocks()[0].video_duration_secs(), None);
    }

    #[test]
    fn plain_text_of_reads_every_form() {
        let v2 = r#"{"format":"sovereign.doc","v":2,"blocks":[{"type":"text","id":"00000000000000000000000000001001","text":"one [pii:pii_record:a1]"},{"type":"image","id":"00000000000000000000000000001002","ref":{"path":"/a.png"},"alt":"x"},{"type":"text","id":"00000000000000000000000000001003","text":"two"}]}"#;
        assert_eq!(plain_text_of(v2), "one [pii:pii_record:a1]\n\ntwo");
        assert_eq!(plain_text_of(r#"{"body":"env","images":[]}"#), "env");
        assert_eq!(plain_text_of("raw {text}"), "raw {text}");
        assert_eq!(plain_text_of(""), "");
        assert_eq!(plain_text_of(r#"{"format":"sovereign.doc","blocks":[]}"#), "");
    }

    #[test]
    fn setting_the_same_text_keeps_the_bytes() {
        let s = r#"{"format":"sovereign.doc","v":2,"blocks":[{"type":"text","id":"00000000000000000000000000001001","text":"café"}]}"#;
        let (mut d, _) = DocumentV2::read(s).unwrap();
        let id = d.blocks()[0].id();
        d.set_text(id, "café").unwrap();
        assert_eq!(d.to_json_string(), s);
    }
}
