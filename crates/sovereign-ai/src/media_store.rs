//! The hosts' media files (contract C-MEDIA, spec ch. 03, *The media store*):
//! [`FileMedia`], what both hosts give the document service.
//!
//! **The store's form: a folder of encrypted files** in the active persona's
//! area (`sovereign_core::auth::media_store_dir`, inside the persona's own
//! state directory), one sealed file per medium. Not records in the database:
//! a video of hundreds of megabytes would be read and written whole by the
//! database, and kept in its logs and compactions; a file is copied and read
//! in a stream (ME3), and the database stays small.
//!
//! **Its key: the persona's KEK.** Each medium has its own random key, wrapped
//! by the KEK as a Document Key is (`sovereign_crypto::media_seal`): the store
//! hangs off the root of the content chain, so guardian recovery reopens it,
//! and a duress session, which has its own KEK and its own area, can neither
//! find nor open the primary's media. A medium's file is named by a keyed hash
//! of its SHA-256, never by the SHA-256 itself. The KEK stays inside the
//! persona's encrypted store: `FileMedia` asks it to wrap and unwrap a
//! medium's key and for the naming key (`MediaKeys`), and never holds the KEK.
//!
//! **Layout:** `<area>/media/store/<2 hex>/<name>`; a copy is written to a
//! temporary file in `store/` and renamed into place once complete, so a
//! medium in the store is always whole.

use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use sovereign_crypto::media_seal::{self, MediaKeys, SealReader, SealWriter};
use sovereign_docs::{
    check_outside_path, hex_digest, is_medium, resolve_in_store, stream_bounded, MediaError, MediaFiles, MediaMark,
    StoredMedium, MEDIA_SIZE_CAP, SVG_SNIFF_BYTES,
};

/// The media files a host gives the document service.
pub struct FileMedia {
    store: Option<Store>,
}

struct Store {
    /// `<area>/media/store`: the sealed media.
    objects: PathBuf,
    /// The persona's KEK, through its store: wrap, unwrap, name; never the
    /// KEK itself.
    keys: Arc<dyn MediaKeys>,
}

/// An error opening or reading an outside file (v2.0.85): the I/O error kind
/// `NotFound` (on Windows, a file or a folder of its path that does not exist)
/// is [`MediaError::Absent`]; every other kind (`PermissionDenied`, a sharing
/// violation, an I/O error, …) is [`MediaError::Unreadable`]: the file may
/// be there, and is never recorded as absent.
fn outside_error(e: std::io::Error) -> MediaError {
    match e.kind() {
        std::io::ErrorKind::NotFound => MediaError::Absent,
        kind => MediaError::Unreadable(format!("{kind:?}")),
    }
}

fn store_error(e: std::io::Error) -> MediaError {
    MediaError::Unreadable(format!("media store: {:?}", e.kind()))
}

/// A name for a temporary file that no other copy shares.
fn temp_name() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut h = RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    format!(".incoming-{:016x}-{}", h.finish(), std::process::id())
}

/// Opens an outside file that names a regular file: never a directory, a
/// device or a pipe (whose read could block or never end).
fn open_outside(outside: &str) -> Result<(File, u64), MediaError> {
    check_outside_path(outside)?;
    let meta = std::fs::metadata(outside).map_err(outside_error)?;
    if !meta.is_file() {
        return Err(MediaError::Unreadable("not a regular file".into()));
    }
    let file = File::open(outside).map_err(outside_error)?;
    Ok((file, meta.len()))
}

impl FileMedia {
    /// No media store: every copy fails, so every outside path a write brings
    /// is neutralised and nothing is read. What `build_document_service`
    /// gives when the host names no persona's area (tools, tests, the CLI).
    pub fn without_store() -> Self {
        Self { store: None }
    }

    /// The media store in `dir` (the persona's area:
    /// `sovereign_core::auth::media_store_dir`), sealed under the persona's
    /// KEK, through `keys` (the persona's encrypted store, which keeps its
    /// KEK). The folder is made when the first medium is copied.
    pub fn in_area(dir: PathBuf, keys: Arc<dyn MediaKeys>) -> Self {
        Self { store: Some(Store { objects: dir.join("store"), keys }) }
    }

    /// The media store of `persona` under the data directory `base`.
    pub fn for_persona(base: &Path, persona: sovereign_core::auth::PersonaKind, keys: Arc<dyn MediaKeys>) -> Self {
        Self::in_area(sovereign_core::auth::media_store_dir(base, persona), keys)
    }

    fn store(&self) -> Result<&Store, MediaError> {
        self.store.as_ref().ok_or_else(|| MediaError::Unreadable("no media store".into()))
    }

    /// The path, inside the store, of the medium named `name` (ME2: refused
    /// lexically, and refused when a link leaves the store).
    fn path_of(store: &Store, name: &str) -> Result<PathBuf, MediaError> {
        if name.len() < 3 || !name.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(MediaError::Refused("not a store name".into()));
        }
        resolve_in_store(&store.objects, &format!("{}/{}", &name[..2], name))
    }

    /// Reads the medium of hash `sha256` from the store, decrypted, in a
    /// stream (for the host that will show media; none does in v0.0.13).
    pub fn open_medium(&self, sha256: &str) -> Result<impl Read, MediaError> {
        let store = self.store()?;
        let name = media_seal::store_name(&*store.keys, sha256);
        let path = Self::path_of(store, &name)?;
        let file = File::open(path).map_err(store_error)?;
        SealReader::new(file, &*store.keys).map_err(store_error)
    }

    /// The copy into the store: the file is read once, in a stream, under the
    /// cap; its first bytes must say image or video, and when `expected` is
    /// given its hash must be that one. Nothing is left in the store on a
    /// refusal.
    fn copy(&self, outside: &str, expected: Option<&str>, mark: Option<MediaMark>) -> Result<StoredMedium, MediaError> {
        let store = self.store()?;
        let (file, len) = open_outside(outside)?;
        if len > MEDIA_SIZE_CAP {
            return Err(MediaError::TooLarge { cap: MEDIA_SIZE_CAP });
        }
        std::fs::create_dir_all(&store.objects).map_err(store_error)?;
        let tmp = store.objects.join(temp_name());
        let result = (|| {
            let out = BufWriter::new(File::create(&tmp).map_err(store_error)?);
            let mut sealer = SealWriter::new(out, &*store.keys, mark.is_some()).map_err(store_error)?;
            let mut hasher = Sha256::new();
            let mut head: Vec<u8> = Vec::with_capacity(SVG_SNIFF_BYTES);
            let mut decided = false;
            stream_bounded(file, MEDIA_SIZE_CAP, |chunk| {
                if !decided {
                    let take = (SVG_SNIFF_BYTES - head.len()).min(chunk.len());
                    head.extend_from_slice(&chunk[..take]);
                    if head.len() == SVG_SNIFF_BYTES {
                        if !is_medium(&head) {
                            return Err(MediaError::NotAMedium);
                        }
                        decided = true;
                    }
                }
                hasher.update(chunk);
                sealer.write_all(chunk).map_err(store_error)
            })?;
            if !decided && !is_medium(&head) {
                return Err(MediaError::NotAMedium);
            }
            let sha = hex_digest(&hasher.finalize());
            if expected.is_some_and(|e| e != sha) {
                return Err(MediaError::HashMismatch);
            }
            let out = sealer.finish().map_err(store_error)?;
            out.into_inner().map_err(|e| store_error(e.into_error()))?.sync_all().map_err(store_error)?;
            Ok(sha)
        })();
        let sha = match result {
            Ok(sha) => sha,
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        };
        let name = media_seal::store_name(&*store.keys, &sha);
        let place = store.objects.join(&name[..2]);
        let finish = (|| {
            std::fs::create_dir_all(&place).map_err(store_error)?;
            let target = Self::path_of(store, &name)?;
            if target.is_file() {
                // Already in the store: the first copy, and its mark, stay.
                let _ = std::fs::remove_file(&tmp);
                let kept = File::open(&target).map_err(store_error).and_then(|f| media_seal::read_mark(f).map_err(store_error))?;
                return Ok(kept.then_some(MediaMark::CopiedByMigration));
            }
            std::fs::rename(&tmp, &target).map_err(store_error)?;
            Ok(mark)
        })();
        match finish {
            Ok(mark) => Ok(StoredMedium { name, sha256: sha, mark }),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }
}

impl MediaFiles for FileMedia {
    /// Never computes a hash from a path (C-MEDIA, ME2): the service no longer
    /// asks, and the answer is always « cannot be read ».
    fn sha256(&self, _path: &str) -> Option<String> {
        None
    }

    fn copy_into_store(&self, outside: &str, mark: Option<MediaMark>) -> Result<StoredMedium, MediaError> {
        self.copy(outside, None, mark)
    }

    fn copy_into_store_checked(
        &self,
        outside: &str,
        expected_sha256: &str,
        mark: Option<MediaMark>,
    ) -> Result<StoredMedium, MediaError> {
        self.copy(outside, Some(expected_sha256), mark)
    }

    fn first_bytes(&self, outside: &str, max: usize) -> Result<Vec<u8>, MediaError> {
        let (file, _) = open_outside(outside)?;
        let mut head = Vec::with_capacity(max.min(SVG_SNIFF_BYTES));
        file.take(max.min(SVG_SNIFF_BYTES) as u64).read_to_end(&mut head).map_err(outside_error)?;
        Ok(head)
    }

    fn store_name_for(&self, sha256: &str) -> Option<String> {
        let store = self.store.as_ref()?;
        if sha256.len() != 64 || !sha256.bytes().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let name = media_seal::store_name(&*store.keys, sha256);
        Self::path_of(store, &name).ok().filter(|p| p.is_file()).map(|_| name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_crypto::kek::Kek;

    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tmp(tag: &str) -> Tmp {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let d = std::env::temp_dir().join(format!("sov-media-store-{}-{nanos}-{tag}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        Tmp(d)
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x10\x00\x00\x00\x10\x08\x06\x00\x00\x00";

    #[test]
    fn a_medium_is_copied_sealed_found_by_its_hash_and_read_back() {
        let t = tmp("copy");
        let outside = t.0.join("cat.png");
        let mut bytes = PNG.to_vec();
        bytes.extend(std::iter::repeat(0xAB).take(200_000));
        std::fs::write(&outside, &bytes).unwrap();
        let media = FileMedia::in_area(t.0.join("area"), Arc::new(Kek::generate()));
        let m = media.copy_into_store(&outside.to_string_lossy(), None).unwrap();
        assert_eq!(m.sha256, hex_digest(&Sha256::digest(&bytes)));
        assert_eq!(m.mark, None);
        assert_eq!(media.store_name_for(&m.sha256), Some(m.name.clone()));
        let mut back = vec![];
        media.open_medium(&m.sha256).unwrap().read_to_end(&mut back).unwrap();
        assert_eq!(back, bytes);
        // On disk: no plaintext, and the name is not the hash.
        let stored = std::fs::read(t.0.join("area").join("store").join(&m.name[..2]).join(&m.name)).unwrap();
        assert!(!stored.windows(PNG.len()).any(|w| w == PNG));
        assert_ne!(m.name, m.sha256);
        // Another persona's KEK finds nothing.
        let other = FileMedia::in_area(t.0.join("area"), Arc::new(Kek::generate()));
        assert_eq!(other.store_name_for(&m.sha256), None);
        // The pass's checked copy: another hash copies nothing, the right one marks.
        let wrong = "0".repeat(64);
        assert_eq!(media.copy_into_store_checked(&outside.to_string_lossy(), &wrong, None), Err(MediaError::HashMismatch));
        let again = media.copy_into_store_checked(&outside.to_string_lossy(), &m.sha256, Some(MediaMark::CopiedByMigration)).unwrap();
        assert_eq!(again.mark, None, "the first copy and its mark stay");
        let leftovers: Vec<_> = std::fs::read_dir(t.0.join("area").join("store")).unwrap().flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(".incoming")).collect();
        assert!(leftovers.is_empty(), "no temporary file is left");
    }

    #[test]
    fn not_a_medium_absent_and_unreadable_are_told_apart() {
        let t = tmp("refuse");
        let text = t.0.join("notes.png");
        std::fs::write(&text, b"These are my notes, not a picture.\n").unwrap();
        let media = FileMedia::in_area(t.0.join("area"), Arc::new(Kek::generate()));
        assert_eq!(media.copy_into_store(&text.to_string_lossy(), None), Err(MediaError::NotAMedium));
        let gone = media.first_bytes(&t.0.join("gone.png").to_string_lossy(), 64).unwrap_err();
        assert_eq!(gone, MediaError::Absent, "an absent file is absent");
        let dir = media.first_bytes(&t.0.to_string_lossy(), 64).unwrap_err();
        assert!(matches!(dir, MediaError::Unreadable(_)), "a folder is here, and not a file to read");
        assert!(matches!(media.first_bytes("//server/share/x.png", 64), Err(MediaError::Refused(_))));
        let head = media.first_bytes(&text.to_string_lossy(), 8).unwrap();
        assert_eq!(head, b"These ar");
        assert!(FileMedia::without_store().copy_into_store(&text.to_string_lossy(), None).is_err());
    }

    /// The background pass a host spawns at unlock (`media_pass_task`) says
    /// what it did when it ends: the notice fires, with no path in it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_background_pass_says_what_it_did_when_it_ends() {
        use sovereign_db::traits::GraphDB;
        let t = tmp("notice");
        let store = crate::docs_host::testing::encrypted_memory_store("media-notice").await;
        let docs = std::sync::Arc::new(crate::docs_host::build_document_service_in_area(
            store.clone(),
            Some(crate::docs_host::testing::account_key()),
            "desktop".into(),
            &t.0.join("media"),
        ));
        let gone = t.0.join("gone.png").to_string_lossy().replace('\\', "/");
        let content = serde_json::json!({"format": "sovereign.doc", "v": 2, "blocks": [
            {"type": "image", "id": format!("{:032x}", 2), "ref": {"path": gone, "sha256": "ab".repeat(32)}, "alt": "a", "caption": ""}
        ]});
        let mut d = sovereign_db::schema::Document::new("Before".into(), "thread:t1".into(), true);
        d.content = content.to_string();
        store.create_document(d).await.unwrap();
        let said = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let sink = said.clone();
        sovereign_docs::media_pass_task(docs.clone(), move |text| sink.lock().unwrap().push(text)).await;
        let said = said.lock().unwrap().clone();
        assert_eq!(said.len(), 1, "one notice when the pass ends");
        assert!(said[0].contains("1 could not be copied."), "it says what was neutralised");
        assert!(!said[0].contains("gone.png"), "no path in the notice");
        assert_eq!(docs.neutralised_media().await.unwrap().len(), 1);
    }
}
