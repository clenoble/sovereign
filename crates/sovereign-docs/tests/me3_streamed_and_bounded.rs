//! ME3 — streamed and bounded.
//!
//! Spec ch. 03, *Received content is hostile — contract (C-MEDIA)*: "**ME3 —
//! Streamed and bounded.** Hashing, copying and conversion read a media file in
//! a stream, never whole. Above a size cap, a constant named in the code, the
//! operation stops with a named error. Nothing is read beyond the cap."
//!
//! The reader here never ends (like `/dev/zero`) and counts what is taken from
//! it, so "nothing is read beyond the cap" is a count, not a timing. Knowing
//! that a stream is *above* the cap takes one byte past it, so the bound
//! checked is cap + 1 bytes read (reported as a reading of "nothing beyond").
//! "Never whole" is not observable from outside beyond that bound: a reader
//! that never ends cannot be read whole, and the cap bounds the rest.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use sovereign_docs::{copy_stream, sha256_in_store, sha256_stream, MediaError, MEDIA_SIZE_CAP};

/// sha256("test").
const SHA_TEST: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

/// A stream of zeros that never ends, counting the bytes taken from it.
struct Endless(Arc<AtomicU64>);
impl Read for Endless {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        buf.fill(0);
        self.0.fetch_add(buf.len() as u64, Ordering::SeqCst);
        Ok(buf.len())
    }
}

/// A writer counting the bytes given to it.
struct Counted(Arc<AtomicU64>);
impl Write for Counted {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.fetch_add(buf.len() as u64, Ordering::SeqCst);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// ME3: the cap is a constant named in the code, and a real bound.
#[test]
fn me3_the_cap_is_a_real_bound() {
    assert!(MEDIA_SIZE_CAP < u64::MAX, "MEDIA_SIZE_CAP is still the no-cap placeholder");
    assert!(MEDIA_SIZE_CAP > 0);
}

// ME3: hashing stops above the cap with the named error, and reads nothing beyond it.
#[test]
fn me3_hashing_stops_above_the_cap_reading_nothing_beyond() {
    let cap = 100_000;
    let read = Arc::new(AtomicU64::new(0));
    assert_eq!(sha256_stream(Endless(read.clone()), cap), Err(MediaError::TooLarge { cap }));
    let n = read.load(Ordering::SeqCst);
    assert!(n <= cap + 1, "read {n} bytes for a cap of {cap}");
}

// ME3: copying stops above the cap with the named error, reading and writing nothing beyond it.
#[test]
fn me3_copying_stops_above_the_cap_reading_nothing_beyond() {
    let cap = 100_000;
    let read = Arc::new(AtomicU64::new(0));
    let written = Arc::new(AtomicU64::new(0));
    assert_eq!(copy_stream(Endless(read.clone()), Counted(written.clone()), cap), Err(MediaError::TooLarge { cap }));
    let (r, w) = (read.load(Ordering::SeqCst), written.load(Ordering::SeqCst));
    assert!(r <= cap + 1, "read {r} bytes for a cap of {cap}");
    assert!(w <= cap, "wrote {w} bytes for a cap of {cap}");
}

// ME3: at the cap exactly, the operation completes and is correct ("above" the cap stops, not "at").
#[test]
fn me3_at_the_cap_hashing_and_copying_complete() {
    assert_eq!(sha256_stream(&b"test"[..], 4), Ok(SHA_TEST.to_string()));
    let mut out = Vec::new();
    assert_eq!(copy_stream(&b"test"[..], &mut out, 4), Ok(4));
    assert_eq!(out, b"test");
    assert_eq!(sha256_stream(&b"tests"[..], 4), Err(MediaError::TooLarge { cap: 4 }));
}

// ME3: a medium the store holds above the cap is not hashed.
#[test]
fn me3_a_stored_medium_above_the_cap_is_not_hashed() {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("sov-me3-{}-{nanos}", std::process::id()));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    std::fs::create_dir_all(&root).unwrap();
    let _clean = Cleanup(root.clone());
    std::fs::write(root.join("big.bin"), vec![7u8; 5_000]).unwrap();
    assert_eq!(sha256_in_store(&root, "big.bin", 1_000), Err(MediaError::TooLarge { cap: 1_000 }));
}
