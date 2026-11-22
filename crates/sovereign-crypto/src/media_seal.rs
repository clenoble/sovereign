//! The sealed files of the media store (contract C-MEDIA, spec ch. 03, *The
//! media store*; keys within ch. 07).
//!
//! - **Each medium is sealed under its own random key**, wrapped by the
//!   persona's KEK as a Document Key is: everything that decrypts user content
//!   hangs off the KEK, so guardian recovery (which restores the KEK) reopens
//!   the store, and nothing depends on the passphrase-bound Device Key.
//! - **It is sealed in a stream**, in chunks of [`CHUNK`] bytes
//!   (XChaCha20-Poly1305, one nonce per chunk: a random prefix and the chunk's
//!   counter), so a medium of any size is never held whole (ME3). The last
//!   chunk is flagged in its associated data, so a file cut at a chunk boundary
//!   does not open; the header is in the associated data of every chunk, so
//!   the mark it carries cannot be changed without the file failing to open.
//! - **The store names a medium by a keyed hash** of its SHA-256 (HMAC under a
//!   key derived from the KEK), never by the SHA-256 itself: a file name on
//!   disk does not confirm, to whoever reads the disk, that a known picture is
//!   there.
//! - **The KEK never leaves its holder.** This module asks for what it needs
//!   through [`MediaKeys`]: wrap a fresh medium key, unwrap a wrapped one, and
//!   the key that names files. The persona's encrypted store implements it
//!   over its own KEK (`sovereign_db::encrypted::EncryptedGraphDB`), so the
//!   media store opens with that store and nothing else; [`Kek`] implements it
//!   too.
//!
//! No logging here (see the crate's invariant): errors are returned.

use std::io::{self, Read, Write};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::aead::{KEY_SIZE, NONCE_SIZE};
use crate::document_key::{DocumentKey, WrappedDocumentKey};
use crate::kek::Kek;

/// The first bytes of every sealed medium (format version 1).
pub const MAGIC: &[u8; 8] = b"SOVMED01";
/// Plaintext bytes per sealed chunk.
pub const CHUNK: usize = 64 * 1024;
const TAG: usize = 16;
const PREFIX: usize = 16;
/// The wrapped key: its 32 bytes and a tag, under the KEK.
const WRAPPED: usize = KEY_SIZE + TAG;
/// MAGIC | flags | wrapped key's nonce | wrapped key | stream prefix.
pub const HEADER_LEN: usize = 8 + 1 + NONCE_SIZE + WRAPPED + PREFIX;
const FLAG_COPIED_BY_MIGRATION: u8 = 1;

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_string())
}

/// What the media store needs of the persona's KEK, without the KEK itself.
pub trait MediaKeys: Send + Sync {
    /// A fresh random key for one medium, and that key wrapped by the KEK.
    fn new_medium_key(&self) -> io::Result<(DocumentKey, WrappedDocumentKey)>;
    /// A medium's key, unwrapped by the KEK; an error when it does not open
    /// under it (another persona's medium, a changed file).
    fn unwrap_medium_key(&self, wrapped: &WrappedDocumentKey) -> io::Result<DocumentKey>;
    /// The HMAC key that names the store's files (derived from the KEK,
    /// domain-separated, `sovereign-media-store-name:v1`).
    fn medium_name_key(&self) -> [u8; KEY_SIZE];
}

impl MediaKeys for Kek {
    fn new_medium_key(&self) -> io::Result<(DocumentKey, WrappedDocumentKey)> {
        let key = DocumentKey::generate();
        let wrapped = key.wrap(self, 0).map_err(|_| invalid("the medium's key could not be wrapped"))?;
        Ok((key, wrapped))
    }

    fn unwrap_medium_key(&self, wrapped: &WrappedDocumentKey) -> io::Result<DocumentKey> {
        DocumentKey::unwrap(wrapped, self).map_err(|_| invalid("the medium's key does not open"))
    }

    fn medium_name_key(&self) -> [u8; KEY_SIZE] {
        let hk = Hkdf::<Sha256>::new(None, self.as_bytes());
        let mut out = [0u8; KEY_SIZE];
        hk.expand(b"sovereign-media-store-name:v1", &mut out).expect("32 bytes is within HKDF output limit");
        out
    }
}

/// The store's name of the medium whose SHA-256 is `sha256_hex`: a keyed
/// hash (HMAC-SHA256 under [`MediaKeys::medium_name_key`]), lowercase hex.
pub fn store_name(keys: &dyn MediaKeys, sha256_hex: &str) -> String {
    let key = zeroize::Zeroizing::new(keys.medium_name_key());
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&key[..]).expect("HMAC takes any key length");
    mac.update(sha256_hex.as_bytes());
    mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

fn nonce_of(prefix: &[u8; PREFIX], counter: u64) -> XNonce {
    let mut n = [0u8; NONCE_SIZE];
    n[..PREFIX].copy_from_slice(prefix);
    n[PREFIX..].copy_from_slice(&counter.to_be_bytes());
    *XNonce::from_slice(&n)
}

fn aad(header: &[u8], last: bool) -> Vec<u8> {
    let mut a = Vec::with_capacity(header.len() + 1);
    a.extend_from_slice(header);
    a.push(last as u8);
    a
}

/// Writes one sealed medium to `out`, in a stream. Call [`SealWriter::finish`]
/// at the end: a writer dropped before it leaves a file that never opens.
pub struct SealWriter<W: Write> {
    out: W,
    cipher: XChaCha20Poly1305,
    prefix: [u8; PREFIX],
    header: Vec<u8>,
    buf: Vec<u8>,
    counter: u64,
}

impl<W: Write> SealWriter<W> {
    /// Starts a sealed medium under a fresh key wrapped by the persona's KEK
    /// (through `keys`), with the mark « copied by the migration » when
    /// `copied_by_migration`.
    pub fn new(mut out: W, keys: &dyn MediaKeys, copied_by_migration: bool) -> io::Result<Self> {
        let (key, wrapped) = keys.new_medium_key()?;
        if wrapped.ciphertext.len() != WRAPPED {
            return Err(invalid("unexpected wrapped key length"));
        }
        let mut prefix = [0u8; PREFIX];
        {
            use rand::Rng;
            rand::rng().fill_bytes(&mut prefix);
        }
        let mut header = Vec::with_capacity(HEADER_LEN);
        header.extend_from_slice(MAGIC);
        header.push(if copied_by_migration { FLAG_COPIED_BY_MIGRATION } else { 0 });
        header.extend_from_slice(&wrapped.nonce);
        header.extend_from_slice(&wrapped.ciphertext);
        header.extend_from_slice(&prefix);
        out.write_all(&header)?;
        let cipher = XChaCha20Poly1305::new(key.as_bytes().into());
        Ok(Self { out, cipher, prefix, header, buf: Vec::with_capacity(CHUNK), counter: 0 })
    }

    fn seal(&mut self, last: bool) -> io::Result<()> {
        let nonce = nonce_of(&self.prefix, self.counter);
        let ct = self
            .cipher
            .encrypt(&nonce, Payload { msg: &self.buf, aad: &aad(&self.header, last) })
            .map_err(|_| invalid("a chunk could not be sealed"))?;
        self.out.write_all(&ct)?;
        self.counter += 1;
        self.buf.clear();
        Ok(())
    }

    /// Seals what is left as the last chunk and returns the output.
    pub fn finish(mut self) -> io::Result<W> {
        self.seal(true)?;
        self.out.flush()?;
        Ok(self.out)
    }
}

impl<W: Write> Write for SealWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let mut rest = data;
        while !rest.is_empty() {
            if self.buf.len() == CHUNK {
                // More follows: the full chunk is not the last one.
                self.seal(false)?;
            }
            let room = CHUNK - self.buf.len();
            let take = room.min(rest.len());
            self.buf.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// Whether a sealed medium's header carries the mark « copied by the
/// migration ». Reads the header only; the mark is authenticated when the
/// medium is opened ([`SealReader`]).
pub fn read_mark<R: Read>(mut r: R) -> io::Result<bool> {
    let mut header = [0u8; HEADER_LEN];
    r.read_exact(&mut header)?;
    if &header[..8] != MAGIC {
        return Err(invalid("not a sealed medium"));
    }
    Ok(header[8] & FLAG_COPIED_BY_MIGRATION != 0)
}

/// Reads one sealed medium back, in a stream; any change to the file makes a
/// read fail.
pub struct SealReader<R: Read> {
    inner: R,
    cipher: XChaCha20Poly1305,
    prefix: [u8; PREFIX],
    header: Vec<u8>,
    counter: u64,
    pending: Vec<u8>,
    plain: Vec<u8>,
    pos: usize,
    done: bool,
    /// Whether the header carries the mark « copied by the migration ».
    pub copied_by_migration: bool,
}

impl<R: Read> SealReader<R> {
    /// Opens a sealed medium under the persona's KEK (through `keys`).
    pub fn new(mut inner: R, keys: &dyn MediaKeys) -> io::Result<Self> {
        let mut header = vec![0u8; HEADER_LEN];
        inner.read_exact(&mut header)?;
        if &header[..8] != MAGIC {
            return Err(invalid("not a sealed medium"));
        }
        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&header[9..9 + NONCE_SIZE]);
        let wrapped = WrappedDocumentKey {
            ciphertext: header[9 + NONCE_SIZE..9 + NONCE_SIZE + WRAPPED].to_vec(),
            nonce,
            epoch: 0,
        };
        let key = keys.unwrap_medium_key(&wrapped)?;
        let mut prefix = [0u8; PREFIX];
        prefix.copy_from_slice(&header[HEADER_LEN - PREFIX..]);
        let copied_by_migration = header[8] & FLAG_COPIED_BY_MIGRATION != 0;
        Ok(Self {
            inner,
            cipher: XChaCha20Poly1305::new(key.as_bytes().into()),
            prefix,
            header,
            counter: 0,
            pending: Vec::with_capacity(CHUNK + TAG + 1),
            plain: vec![],
            pos: 0,
            done: false,
            copied_by_migration,
        })
    }

    /// Decrypts the next chunk into `plain`; false at the end.
    fn next_chunk(&mut self) -> io::Result<bool> {
        if self.done {
            return Ok(false);
        }
        // Read one byte past a full chunk: whether more follows says whether
        // this chunk is the last.
        let want = CHUNK + TAG + 1;
        while self.pending.len() < want {
            let mut tmp = vec![0u8; want - self.pending.len()];
            let n = self.inner.read(&mut tmp)?;
            if n == 0 {
                break;
            }
            self.pending.extend_from_slice(&tmp[..n]);
        }
        let last = self.pending.len() <= CHUNK + TAG;
        let take = if last { self.pending.len() } else { CHUNK + TAG };
        if take < TAG {
            return Err(invalid("a sealed medium is cut short"));
        }
        let nonce = nonce_of(&self.prefix, self.counter);
        let chunk: Vec<u8> = self.pending.drain(..take).collect();
        self.plain = self
            .cipher
            .decrypt(&nonce, Payload { msg: &chunk, aad: &aad(&self.header, last) })
            .map_err(|_| invalid("a sealed medium was changed or cut"))?;
        self.pos = 0;
        self.counter += 1;
        self.done = last;
        Ok(true)
    }
}

impl<R: Read> Read for SealReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.pos == self.plain.len() {
            if !self.next_chunk()? {
                return Ok(0);
            }
        }
        let n = (self.plain.len() - self.pos).min(out.len());
        out[..n].copy_from_slice(&self.plain[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(len: usize) {
        let kek = Kek::generate();
        let data: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
        let mut w = SealWriter::new(Vec::new(), &kek, len % 2 == 1).unwrap();
        for part in data.chunks(10_007) {
            w.write_all(part).unwrap();
        }
        let sealed = w.finish().unwrap();
        assert!(!sealed.windows(16).any(|x| len >= 16 && x == &data[..16]), "no plaintext on disk");
        let mut r = SealReader::new(&sealed[..], &kek).unwrap();
        assert_eq!(r.copied_by_migration, len % 2 == 1);
        let mut back = vec![];
        r.read_to_end(&mut back).unwrap();
        assert_eq!(back, data, "length {len}");
        assert_eq!(read_mark(&sealed[..]).unwrap(), len % 2 == 1);
    }

    #[test]
    fn sealed_media_round_trip_at_every_chunk_edge() {
        for len in [0, 1, 15, CHUNK - 1, CHUNK, CHUNK + 1, 2 * CHUNK, 2 * CHUNK + 7] {
            round_trip(len);
        }
    }

    #[test]
    fn a_cut_or_changed_medium_does_not_open() {
        let kek = Kek::generate();
        let mut w = SealWriter::new(Vec::new(), &kek, false).unwrap();
        w.write_all(&vec![7u8; 2 * CHUNK]).unwrap();
        let sealed = w.finish().unwrap();
        // Cut at the end of the first chunk.
        let cut = &sealed[..HEADER_LEN + CHUNK + TAG];
        assert!(SealReader::new(cut, &kek).unwrap().read_to_end(&mut vec![]).is_err());
        // The mark flipped in the header.
        let mut flipped = sealed.clone();
        flipped[8] ^= FLAG_COPIED_BY_MIGRATION;
        assert!(SealReader::new(&flipped[..], &kek).unwrap().read_to_end(&mut vec![]).is_err());
        // Another persona's KEK.
        assert!(SealReader::new(&sealed[..], &Kek::generate()).is_err());
    }

    #[test]
    fn names_are_keyed() {
        let (a, b) = (Kek::generate(), Kek::generate());
        let sha = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
        assert_eq!(store_name(&a, sha), store_name(&a, sha));
        assert_ne!(store_name(&a, sha), store_name(&b, sha));
        assert!(!store_name(&a, sha).contains(sha));
    }
}
