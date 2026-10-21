//! The v2 document model (spec ch. 03, *Document format v2 — blocks*).
//!
//! This module was the editor's **provisional** model; it now lives in
//! `sovereign-docformat`, shared with the document service (C-DOC), and is
//! re-exported here with the same names and signatures
//! ([`DocumentV2::read`], [`DocumentV2::to_json_string`], [`Block`],
//! [`BlockId`], [`SourceForm`], [`FormatError`]).
//!
//! What it holds to (guarantee 2 and ch. 03):
//! - `read` accepts the three stored forms: v2 (`{"format":"sovereign.doc","v":2,…}`),
//!   the v1 envelope `{"body","images","videos"}` (→ one `text` block, then one
//!   `image`/`video` block per entry, in that order) and a raw-text body (→ one
//!   `text` block holding the whole string). Migrated images have `alt: ""`
//!   and a "needs a description" flag; migrated media have no `ref.sha256`
//!   until the document service computes it;
//! - personal-data tokens are `[pii:<record_id>]` (record ids contain `:`).
//!   Text is stored exactly as typed — nothing is escaped. Whether text in
//!   token form is a token is decided at display time by the host's resolver;
//! - a `v` higher than known is reported, so the editor opens it read-only and
//!   never rewrites it: the stored string is written back unchanged;
//! - unknown block types, unknown fields of known blocks and unknown top-level
//!   fields are preserved; unchanged blocks keep their bytes.
//!
//! `Debug` for [`DocumentV2`] and [`Block`] does not print document text
//! (guarantee 3: a `{:?}` in a log line is a log line).

pub use sovereign_docformat::{Block, BlockId, DocumentV2, FormatError, SourceForm};

pub(crate) use sovereign_docformat::BlockKind;
