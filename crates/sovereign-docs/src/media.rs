//! The media store (contract C-MEDIA, spec ch. 03, *Received content is
//! hostile*): media are resolved only inside the store (ME2), read in a stream
//! and bounded by a cap (ME3).
//!
//! **Types and signatures only.** Written by the test author of lot L1b so the
//! tests in `tests/me*_*.rs` compile; every body is the implementer's. The
//! signatures may be argued on the channel; the tests are not edited by whoever
//! implements them.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// ME3: above this many bytes, hashing, copying and conversion stop with
/// [`MediaError::TooLarge`] and read nothing beyond it.
///
/// **4 GiB, proposed by the implementer of lot L1b; Céline decides.** It
/// keeps a phone's ordinary recordings: about 10 minutes of 4K at 60 frames a
/// second (HEVC, about 400 MB a minute), 20 to 25 minutes of 4K at 30, an hour
/// or more of 1080p. It refuses longer 4K recordings and professional formats
/// (ProRes, a few GB a minute). 4 GiB is also the size at which many phones
/// and FAT32 cards split a recording into parts, each of which then fits.
pub const MEDIA_SIZE_CAP: u64 = 4 * 1024 * 1024 * 1024;

/// Why a medium was not resolved, hashed or copied.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MediaError {
    /// ME2: refused from the name alone, before any file-system call
    /// (absolute, network, device name, `..`, on Windows `\\?\` and
    /// alternate data streams). The text says why; it never holds file content.
    #[error("media name refused: {0}")]
    Refused(String),
    /// ME2: the name resolves outside the media store (a link or a junction
    /// that leaves it).
    #[error("media name resolves outside the media store")]
    OutsideStore,
    /// ME3: the medium is larger than the cap; nothing beyond it was read.
    #[error("medium larger than the cap of {cap} bytes")]
    TooLarge { cap: u64 },
    /// The migration pass: the file's hash is not the one the block recorded;
    /// nothing was copied.
    #[error("the medium's hash is not the one the document recorded")]
    HashMismatch,
    /// A medium must be a medium (v2.0.81): the file's first bytes do not say
    /// image or video, whatever its name; nothing was copied.
    #[error("the file is not an image or a video")]
    NotAMedium,
    /// v2.0.85: no file is there (« not found »).
    #[error("no medium at that place")]
    Absent,
    /// The medium is there but cannot be read (no permission, a lock, an I/O
    /// error).
    #[error("medium cannot be read: {0}")]
    Unreadable(String),
}

/// How many first bytes of a file are enough to tell an image or a video
/// ([`is_medium`]). The migration pass reads no more than this of a file that
/// turns out not to be a medium. *Set by the test author; the implementer may
/// change the value, stating why.*
pub const MEDIUM_SNIFF_BYTES: usize = 64;

/// v2.0.84: "An SVG is a file whose root element is `svg`. It is read within a
/// stated bound larger than the binary sniff." How many first bytes of a text
/// file are read to find its root element; a declaration, a DOCTYPE, comments
/// and white space may come before it, nothing else. *Set by the test author;
/// the implementer may change the value, stating why.*
pub const SVG_SNIFF_BYTES: usize = 4096;

/// C-MEDIA, *The media store*: a mark the store keeps on a medium.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaMark {
    /// « copied by the migration »: every medium the migration pass copies,
    /// in doubt. It never leaves the device until she has seen it and kept it
    /// (a later lot). The mark cannot be rebuilt afterwards.
    CopiedByMigration,
}

/// A medium the store holds: the store's name for it, its SHA-256 (lowercase
/// hex), and its mark, if any. A medium inserted after the update has none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredMedium {
    pub name: String,
    pub sha256: String,
    pub mark: Option<MediaMark>,
}

/// C-MEDIA, *The media store*: what the migration pass at the update did
/// (`DocumentService::migrate_media_into_store`). Every media block that named
/// an outside path is either copied or neutralised; the neutralised are
/// counted, so they can be said.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaMigrationReport {
    /// Blocks whose file matched the recorded hash and was copied into the store.
    pub copied: usize,
    /// Blocks neutralised: refused by name, absent, unreadable, above the cap,
    /// or of another hash.
    pub neutralised: usize,
}

/// C-MEDIA, *The media store* (v2.0.84, Céline: « Liste large + registre »):
/// why the migration pass neutralised a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NeutralisedReason {
    /// v2.0.86: the block recorded no hash, so nothing can vouch for the
    /// file: nothing was read. (Replaces « written on another device »: the
    /// guarded copy holds for every document, whichever device wrote it last;
    /// Céline: « Plus simple : copier partout ».)
    NoRecordedHash,
    /// The file is absent.
    Absent,
    /// The file's hash is not the one the block recorded.
    OtherHash,
    /// The file's first bytes say neither image nor video.
    NotAMedium,
    /// ME2's lexical refusals (a network path, a device name, `..`, `\\?\`,
    /// an alternate data stream): nothing was opened.
    LexicalRefusal,
    /// v2.0.85: the file is there but above [`MEDIA_SIZE_CAP`]; never
    /// recorded as absent. A cap found too low can be mended from the record.
    TooLarge,
    /// v2.0.85: the file is there but could not be read at that moment.
    Unreadable,
}

/// One entry of the record of what the migration pass neutralised: the
/// document, the block, the former path **as text** and the reason. The record
/// lives in the persona's area, encrypted; nothing ever opens a path from it.
/// A later pass over it can mend a list of formats found too short.
#[derive(Clone, PartialEq, Eq)]
pub struct NeutralisedMedium {
    pub document: crate::types::DocId,
    pub block: sovereign_docformat::BlockId,
    pub former_path: String,
    pub reason: NeutralisedReason,
}

// Debug without the path (a path is content): its size only.
impl std::fmt::Debug for NeutralisedMedium {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NeutralisedMedium")
            .field("document", &self.document)
            .field("block", &self.block)
            .field("former_path_bytes", &self.former_path.len())
            .field("reason", &self.reason)
            .finish()
    }
}

/// C-MEDIA, *The media store*: where a media block's medium is, as the user is
/// told. A received state's outside paths are neutralised when it is applied;
/// its blocks keep their medium's name and hash and are shown as « medium not
/// on this device »: absent, never empty (ch. 03, *What cannot be read is never
/// shown as empty*).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaPresence {
    /// The medium is in this device's media store.
    InStore,
    /// The medium is not on this device (« medium not on this device »). A
    /// neutralised block resolves by its hash: it is `InStore` when the store
    /// holds a medium of that hash.
    NotOnThisDevice,
}


/// C-MEDIA, the record: the reason the migration pass gives a block whose
/// copy failed with `e`. The one place this is decided. A file that is there
/// is never « absent » (v2.0.85): absent only for [`MediaError::Absent`]
/// (« not found »); a file above the cap is « too large », one that is there
/// but cannot be read is « unreadable ».
pub fn neutralised_reason_of(e: &MediaError) -> NeutralisedReason {
    match e {
        MediaError::Refused(_) | MediaError::OutsideStore => NeutralisedReason::LexicalRefusal,
        MediaError::HashMismatch => NeutralisedReason::OtherHash,
        MediaError::NotAMedium => NeutralisedReason::NotAMedium,
        MediaError::TooLarge { .. } => NeutralisedReason::TooLarge,
        MediaError::Absent => NeutralisedReason::Absent,
        MediaError::Unreadable(_) => NeutralisedReason::Unreadable,
    }
}

/// The name the record stores for a reason.
pub(crate) fn reason_name(r: NeutralisedReason) -> &'static str {
    match r {
        NeutralisedReason::NoRecordedHash => "no_recorded_hash",
        NeutralisedReason::Absent => "absent",
        NeutralisedReason::OtherHash => "other_hash",
        NeutralisedReason::NotAMedium => "not_a_medium",
        NeutralisedReason::LexicalRefusal => "lexical_refusal",
        NeutralisedReason::TooLarge => "too_large",
        NeutralisedReason::Unreadable => "unreadable",
    }
}

/// The reason a record entry names ([`reason_name`]).
pub(crate) fn reason_of_name(s: &str) -> Option<NeutralisedReason> {
    [
        NeutralisedReason::NoRecordedHash,
        NeutralisedReason::Absent,
        NeutralisedReason::OtherHash,
        NeutralisedReason::NotAMedium,
        NeutralisedReason::LexicalRefusal,
        NeutralisedReason::TooLarge,
        NeutralisedReason::Unreadable,
    ]
    .into_iter()
    .find(|r| reason_name(*r) == s)
}

// ------------------------------------------------------------ the formats --

/// C-MEDIA, *The media store*, "a medium must be a medium": whether a file's
/// first bytes say image or video, by the list of formats (v2.0.84). Never
/// decided by the file's name. Given at most [`SVG_SNIFF_BYTES`]; a binary
/// format is decided within [`MEDIUM_SNIFF_BYTES`]. The insertion and the
/// migration pass copy only a file for which this holds.
///
/// The list is the contract's (spec v2.0.84; Céline, 2026-11-14: « Liste
/// large + registre »), each format known by the signature it defines:
/// - images: PNG, JPEG, GIF, WebP, BMP (on a coherent header, never « BM »
///   alone), TIFF (and BigTIFF), HEIC and HEIF, AVIF (by their `ftyp`
///   brands), SVG (by its root element `svg`, see below);
/// - videos: MP4 and MOV (by their `ftyp` brands; an old QuickTime file by a
///   `moov`, `mdat`, `wide`, `free` or `skip` box at offset 4), WebM (an EBML
///   header whose DocType is `webm`), AVI (a RIFF header of type `AVI `).
///
/// **SVG** is text, so its first bytes prove less: it is a medium only when
/// its root element is `svg`, found within these bytes. Before it, a UTF-8
/// BOM, white space, an XML declaration, processing instructions, comments
/// and a DOCTYPE are allowed; nothing else is. An XML file with another root
/// is not a medium, whatever its name.
pub fn is_medium(first_bytes: &[u8]) -> bool {
    let b = first_bytes;
    is_png(b)
        || is_jpeg(b)
        || is_gif(b)
        || is_riff(b, b"WEBP")
        || is_riff(b, b"AVI ")
        || is_bmp(b)
        || is_tiff(b)
        || is_iso_media(b)
        || is_old_quicktime(b)
        || is_webm(b)
        || is_svg(b)
}

/// Whether `first_bytes` say SVG ([`is_medium`]'s check for it): the one
/// medium that is text, and whose rendering must be inert.
pub fn is_svg(first_bytes: &[u8]) -> bool {
    svg_root(&first_bytes[..first_bytes.len().min(SVG_SNIFF_BYTES)])
}

fn is_png(b: &[u8]) -> bool {
    b.starts_with(b"\x89PNG\r\n\x1a\n")
}

fn is_jpeg(b: &[u8]) -> bool {
    b.starts_with(&[0xFF, 0xD8, 0xFF])
}

fn is_gif(b: &[u8]) -> bool {
    b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a")
}

/// A RIFF container of the given form type (`WEBP`, `AVI `).
fn is_riff(b: &[u8], form: &[u8; 4]) -> bool {
    b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == form
}

fn le_u32(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn be_u32(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

/// BMP, on a coherent header (spec v2.0.84: not on « BM » alone): the two
/// reserved fields are zero, the DIB header has a size a BMP defines, and the
/// pixel data starts after the headers, at a plausible offset inside the
/// file. A text file that begins with « BM » fails on its reserved bytes.
fn is_bmp(b: &[u8]) -> bool {
    const DIB_SIZES: [u32; 8] = [12, 16, 40, 52, 56, 64, 108, 124];
    if b.len() < 18 || &b[0..2] != b"BM" {
        return false;
    }
    let (Some(file_size), Some(reserved), Some(offset), Some(dib)) =
        (le_u32(b, 2), le_u32(b, 6), le_u32(b, 10), le_u32(b, 14))
    else {
        return false;
    };
    reserved == 0
        && DIB_SIZES.contains(&dib)
        && offset >= 14 + dib
        && offset <= 64 * 1024
        && (file_size == 0 || offset < file_size)
}

/// TIFF, little- or big-endian, and BigTIFF.
fn is_tiff(b: &[u8]) -> bool {
    b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") || b.starts_with(b"II+\0") || b.starts_with(b"MM\0+")
}

/// The ISO media file brands that say MP4 or MOV video, HEIC or HEIF, or
/// AVIF.
const MEDIA_BRANDS: &[&[u8; 4]] = &[
    // MP4
    b"isom", b"iso2", b"iso3", b"iso4", b"iso5", b"iso6", b"iso7", b"iso8", b"iso9", b"mp41", b"mp42", b"mp71",
    b"avc1", b"av01", b"dash", b"M4V ", b"M4VH", b"M4VP", b"MSNV", b"f4v ",
    // MOV (QuickTime)
    b"qt  ",
    // HEIC and HEIF
    b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"hevm", b"hevs", b"mif1", b"mif2", b"msf1",
    // AVIF
    b"avif", b"avis",
];

/// Audio-only brands of the same container: never a medium, whatever their
/// compatible brands say.
const AUDIO_BRANDS: &[&[u8; 4]] = &[b"M4A ", b"M4B ", b"M4P ", b"F4A ", b"F4B "];

/// An ISO media file (`ftyp` box at offset 4) of a brand on the list: by its
/// major brand, else by a compatible brand when the major one is not an
/// audio brand.
fn is_iso_media(b: &[u8]) -> bool {
    if b.len() < 12 || &b[4..8] != b"ftyp" {
        return false;
    }
    let size = be_u32(b, 0).unwrap_or(0) as usize;
    if size != 0 && size < 12 {
        return false;
    }
    let brand = |at: usize| -> Option<[u8; 4]> { b.get(at..at + 4).map(|s| [s[0], s[1], s[2], s[3]]) };
    let on = |list: &[&[u8; 4]], x: &[u8; 4]| list.iter().any(|l| *l == x);
    let Some(major) = brand(8) else { return false };
    if on(MEDIA_BRANDS, &major) {
        return true;
    }
    if on(AUDIO_BRANDS, &major) {
        return false;
    }
    let end = if size == 0 { b.len() } else { size.min(b.len()) };
    (16..end).step_by(4).filter_map(brand).any(|c| on(MEDIA_BRANDS, &c))
}

/// An old QuickTime movie, which may begin with another atom than `ftyp`.
fn is_old_quicktime(b: &[u8]) -> bool {
    const ATOMS: [&[u8; 4]; 5] = [b"moov", b"mdat", b"wide", b"free", b"skip"];
    if b.len() < 8 || !ATOMS.iter().any(|a| &b[4..8] == *a) {
        return false;
    }
    let size = be_u32(b, 0).unwrap_or(0);
    size == 0 || size == 1 || size >= 8
}

/// WebM: an EBML header whose DocType element says `webm` (a Matroska file
/// says `matroska`, and is not on the list).
fn is_webm(b: &[u8]) -> bool {
    if !b.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return false;
    }
    let mut i = 4;
    while i + 2 < b.len() {
        if b[i] == 0x42 && b[i + 1] == 0x82 {
            let first = b[i + 2];
            let len = first.leading_zeros() as usize + 1;
            if len > 8 || i + 2 + len > b.len() {
                return false;
            }
            let mut value = (first as u64) & (0xFFu64 >> len);
            for k in 1..len {
                value = (value << 8) | b[i + 2 + k] as u64;
            }
            let start = i + 2 + len;
            return value == 4 && b.get(start..start + 4) == Some(&b"webm"[..]);
        }
        i += 1;
    }
    false
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// SVG, by its root element (spec v2.0.84). Only a UTF-8 BOM, white space, an
/// XML declaration or processing instruction, comments and a DOCTYPE may come
/// before `<svg` (or `<prefix:svg`); the element's name must end within the
/// bytes given.
fn svg_root(b: &[u8]) -> bool {
    let mut i = if b.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };
    loop {
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        let rest = &b[i..];
        if rest.starts_with(b"<?") {
            let Some(j) = find(&rest[2..], b"?>") else { return false };
            i += 2 + j + 2;
        } else if rest.starts_with(b"<!--") {
            let Some(j) = find(&rest[4..], b"-->") else { return false };
            i += 4 + j + 3;
        } else if rest.len() >= 9 && rest[..9].eq_ignore_ascii_case(b"<!DOCTYPE") {
            let Some(j) = doctype_end(&rest[9..]) else { return false };
            i += 9 + j;
        } else if rest.starts_with(b"<") {
            let name_len = rest[1..].iter().position(|c| matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/'));
            let Some(n) = name_len else { return false };
            let name = &rest[1..1 + n];
            let local = match name.iter().rposition(|c| *c == b':') {
                Some(k) => &name[k + 1..],
                None => name,
            };
            return local == b"svg";
        } else {
            return false;
        }
    }
}

/// The length of a DOCTYPE's remainder up to and including its closing `>`,
/// across quoted strings and an internal subset in `[...]`.
fn doctype_end(b: &[u8]) -> Option<usize> {
    let mut quote: Option<u8> = None;
    let mut depth = 0usize;
    for (i, &c) in b.iter().enumerate() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                b'"' | b'\'' => quote = Some(c),
                b'[' => depth += 1,
                b']' => depth = depth.saturating_sub(1),
                b'>' if depth == 0 => return Some(i + 1),
                _ => {}
            },
        }
    }
    None
}

// ------------------------------------------------------------- ME2, names --

/// Whether a path component is a Windows device name (`CON`, `NUL`, `COM1`,
/// …), with or without an extension, in any case.
fn is_device_name(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or("").trim_end_matches(' ');
    let upper = stem.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$") {
        return true;
    }
    for prefix in ["COM", "LPT"] {
        if let Some(rest) = upper.strip_prefix(prefix) {
            let mut chars = rest.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                if c.is_ascii_digit() || matches!(c, '\u{b9}' | '\u{b2}' | '\u{b3}') {
                    return true;
                }
            }
        }
    }
    false
}

/// The refusals both checks share: a network path, `\\?\` and `\\.\`, a
/// control character, `..`, a device name, an alternate data stream (any `:`
/// after the drive letter). `drive_allowed`: an outside path may start with a
/// drive letter (`C:`), a store name may not.
fn shared_refusals(name: &str, drive_allowed: bool) -> Result<(), MediaError> {
    let refused = |why: &str| Err(MediaError::Refused(why.to_string()));
    if name.is_empty() {
        return refused("an empty name");
    }
    if name.chars().any(|c| c.is_control()) {
        return refused("a control character");
    }
    let unified = name.replace('\\', "/");
    if unified.starts_with("//?/") || unified.starts_with("//./") {
        return refused("a device or verbatim path prefix");
    }
    if unified.starts_with("//") {
        return refused("a network path");
    }
    let bytes = unified.as_bytes();
    let has_drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    let after_drive = if has_drive { &unified[2..] } else { unified.as_str() };
    if after_drive.contains(':') {
        return refused("an alternate data stream");
    }
    if has_drive && !drive_allowed {
        return refused("an absolute path");
    }
    for component in after_drive.split('/') {
        if component == ".." {
            return refused("a parent component");
        }
        if is_device_name(component) {
            return refused("a device name");
        }
    }
    Ok(())
}

/// ME2, first step: the lexical check of a name the store is asked to resolve.
/// Pure: it makes no file-system call.
pub fn check_media_name(name: &str) -> Result<(), MediaError> {
    shared_refusals(name, false)?;
    if name.starts_with('/') || name.starts_with('\\') {
        return Err(MediaError::Refused("an absolute path".to_string()));
    }
    Ok(())
}

/// C-MEDIA, the migration pass and insertion: ME2's lexical refusals, except
/// the refusal of an absolute path (an outside path is absolute by nature;
/// spec v2.0.84). A network path, a device name, `..`, the `\\?\` prefix and
/// an alternate data stream are refused **without any file-system call**.
pub fn check_outside_path(path: &str) -> Result<(), MediaError> {
    shared_refusals(path, true)
}

/// An I/O error as a media error: « not found » is [`MediaError::Absent`],
/// any other kind is [`MediaError::Unreadable`] (the file may be there).
fn io_kind(e: std::io::Error) -> MediaError {
    match e.kind() {
        std::io::ErrorKind::NotFound => MediaError::Absent,
        kind => MediaError::Unreadable(format!("{kind:?}")),
    }
}

/// ME2, second step: the path, inside the store rooted at `root`, of the
/// medium named `name`. Runs [`check_media_name`] first; then opens nothing
/// unless the resolved path is inside `root` ([`MediaError::OutsideStore`]
/// for a link or junction that leaves it).
pub fn resolve_in_store(root: &Path, name: &str) -> Result<PathBuf, MediaError> {
    check_media_name(name)?;
    let canon_root = std::fs::canonicalize(root).map_err(io_kind)?;
    let mut joined = canon_root.clone();
    for component in name.split(['/', '\\']).filter(|c| !c.is_empty() && *c != ".") {
        joined.push(component);
    }
    // The deepest part of the path that exists, resolved through every link
    // and junction on the way, must still be inside the store.
    let mut probe = joined.clone();
    loop {
        match std::fs::canonicalize(&probe) {
            Ok(real) => {
                if !real.starts_with(&canon_root) {
                    return Err(MediaError::OutsideStore);
                }
                return Ok(if probe == joined { real } else { joined });
            }
            Err(_) => {
                if !probe.pop() || !probe.starts_with(&canon_root) {
                    return Err(MediaError::OutsideStore);
                }
            }
        }
    }
}

/// ME2 and ME3: the SHA-256 of the medium the store at `root` holds under
/// `name`, read in a stream and bounded by `cap`.
pub fn sha256_in_store(root: &Path, name: &str, cap: u64) -> Result<String, MediaError> {
    let path = resolve_in_store(root, name)?;
    let file = std::fs::File::open(&path).map_err(io_kind)?;
    sha256_stream(file, cap)
}

// ------------------------------------------------------------ ME3, streams --

const STREAM_CHUNK: usize = 64 * 1024;

/// Lowercase hex of a digest.
pub fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Reads `reader` in chunks, never more than `cap + 1` bytes in all, handing
/// each chunk to `each`. Stops with [`MediaError::TooLarge`] as soon as more
/// than `cap` bytes have been read; the chunk that crosses the cap is not
/// handed on. Returns the number of bytes read.
pub fn stream_bounded<R: Read>(
    reader: R,
    cap: u64,
    mut each: impl FnMut(&[u8]) -> Result<(), MediaError>,
) -> Result<u64, MediaError> {
    let mut limited = reader.take(cap.saturating_add(1));
    let mut buf = vec![0u8; STREAM_CHUNK];
    let mut total: u64 = 0;
    loop {
        let n = match limited.read(&mut buf) {
            Ok(0) => return Ok(total),
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(io_kind(e)),
        };
        total += n as u64;
        if total > cap {
            return Err(MediaError::TooLarge { cap });
        }
        each(&buf[..n])?;
    }
}

/// ME3: the SHA-256 (lowercase hex) of what `reader` yields, read in a
/// stream. Above `cap` bytes it stops with [`MediaError::TooLarge`]; nothing
/// beyond the cap is read.
pub fn sha256_stream<R: Read>(reader: R, cap: u64) -> Result<String, MediaError> {
    let mut hasher = Sha256::new();
    stream_bounded(reader, cap, |chunk| {
        hasher.update(chunk);
        Ok(())
    })?;
    Ok(hex_digest(&hasher.finalize()))
}

/// ME3: copies what `reader` yields to `writer`, in a stream; returns the
/// number of bytes copied. Above `cap` bytes it stops with
/// [`MediaError::TooLarge`]; nothing beyond the cap is read or written.
pub fn copy_stream<R: Read, W: Write>(reader: R, mut writer: W, cap: u64) -> Result<u64, MediaError> {
    let write_err = |e: std::io::Error| MediaError::Unreadable(format!("write: {:?}", e.kind()));
    let n = stream_bounded(reader, cap, |chunk| writer.write_all(chunk).map_err(write_err))?;
    writer.flush().map_err(write_err)?;
    Ok(n)
}

#[cfg(test)]
mod format_tests {
    //! The implementer's own checks of the list's signatures (the contract's
    //! tests are the test author's, in `tests/`).
    use super::*;

    #[test]
    fn signatures_on_the_list() {
        let bmp: &[u8] = b"BM\x46\x00\x00\x00\x00\x00\x00\x00\x36\x00\x00\x00\x28\x00\x00\x00\x01\x00";
        let cases: [(&str, &[u8]); 12] = [
            ("GIF", b"GIF89a\x01\x00\x01\x00"),
            ("WebP", b"RIFF\x24\x00\x00\x00WEBPVP8 "),
            ("AVI", b"RIFF\x24\x00\x00\x00AVI LIST"),
            ("BMP", bmp),
            ("TIFF", b"II*\x00\x08\x00\x00\x00"),
            ("TIFF BE", b"MM\x00*\x00\x00\x00\x08"),
            ("HEIC", b"\x00\x00\x00\x18ftypheic\x00\x00\x00\x00mif1heic"),
            ("AVIF", b"\x00\x00\x00\x1cftypavif\x00\x00\x00\x00avifmif1miaf"),
            ("MOV", b"\x00\x00\x00\x14ftypqt  \x00\x00\x02\x00qt  "),
            ("old MOV", b"\x00\x00\x00\x08wide\x00\x00\x10\x00mdat"),
            ("WebM", b"\x1a\x45\xdf\xa3\x9f\x42\x86\x81\x01\x42\xf7\x81\x01\x42\x82\x84webm\x42\x87"),
            ("SVG", b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
        ];
        for (what, bytes) in cases {
            assert!(is_medium(bytes), "{what} is a medium");
        }
    }

    #[test]
    fn near_misses_are_not_media() {
        let cases: [(&str, &[u8]); 9] = [
            ("text starting BM", b"BMW notes: the car goes to the garage on Monday.\n"),
            ("Matroska", b"\x1a\x45\xdf\xa3\x9f\x42\x86\x81\x01\x42\x82\x88matroska"),
            ("m4a", b"\x00\x00\x00\x20ftypM4A \x00\x00\x00\x00M4A mp42isom\x00\x00\x00\x00"),
            ("RIFF WAVE", b"RIFF\x24\x00\x00\x00WAVEfmt "),
            ("XML, another root", b"<?xml version=\"1.0\"?>\n<KeePassFile><Meta/></KeePassFile>"),
            ("HTML", b"<!DOCTYPE html><html><body><svg/></body></html>"),
            ("text, then svg", b"hello <svg/>"),
            ("svg name too long", b"<svgx/>"),
            ("cut before the root", b"<?xml version=\"1.0\"?><!-- a comment that never ends"),
        ];
        for (what, bytes) in cases {
            assert!(!is_medium(bytes), "{what} is not a medium");
        }
    }

    #[test]
    fn svg_after_a_long_prolog() {
        let mut s = String::from("\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n");
        s.push_str("<!-- Created with Inkscape (http://www.inkscape.org/) -->\n<!-- ");
        s.push_str(&"licence text ".repeat(30));
        s.push_str("-->\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\" [ <!ENTITY a \"b>\"> ]>\n");
        s.push_str("<svg:svg xmlns:svg=\"http://www.w3.org/2000/svg\" width=\"10\"></svg:svg>");
        assert!(s.len() > MEDIUM_SNIFF_BYTES && s.len() < SVG_SNIFF_BYTES);
        assert!(is_medium(s.as_bytes()));
        assert!(!is_medium(&s.as_bytes()[..64]), "64 bytes are not enough for this one");
    }

    #[test]
    fn outside_paths_keep_the_drive_and_refuse_the_rest() {
        for ok in ["C:/Users/someone/Pictures/cat.png", r"C:\Users\x\a.jpg", "/home/x/a.png", "/m/cat.png"] {
            assert_eq!(check_outside_path(ok), Ok(()), "{ok}");
        }
        for bad in [
            "//server/share/n.png",
            r"\\server\share\n.png",
            r"\\?\C:\x.png",
            r"\\.\PhysicalDrive0",
            "C:/x/../y.png",
            "C:/x/NUL",
            "C:/x/a.png:stream",
            "C:/x/com1.jpg",
        ] {
            assert!(matches!(check_outside_path(bad), Err(MediaError::Refused(_))), "{bad}");
        }
    }
}
