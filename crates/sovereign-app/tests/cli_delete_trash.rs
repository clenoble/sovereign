//! S5-2 — the CLI trashes (C-DOC adoption step 5,
//! `doc/plans/C-DOC-adoption.md`): "`delete-doc` refuses without the account,
//! and with it moves the document to the trash; nothing is hard-deleted."
//!
//! This file holds the half a subprocess can check: without an account,
//! `delete-doc` refuses and the document is still there, untouched. The half
//! with the account (the trash) is `src/step5_tests.rs`, over an encrypted
//! store, since a subprocess would wait for a passphrase.
//!
//! Every run sets `SOVEREIGN_DATA_DIR` to its own temporary directory (no
//! `crypto/auth.store` there): the test never reaches a real workspace.

use std::path::PathBuf;
use std::process::Command;

use sovereign_db::schema::Document;
use sovereign_db::surreal::{SurrealGraphDB, StorageMode};
use sovereign_db::GraphDB;

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!("sovereign-test-s5-2-{}", std::process::id()))
}

fn db_path() -> String {
    temp_dir().join("test.db").display().to_string().replace('\\', "/")
}

fn sovereign_cmd() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sovereign-tauri"));
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join("config.toml");
    std::fs::write(&config_path, format!("[database]\nmode = \"persistent\"\npath = \"{}\"\n", db_path())).unwrap();
    cmd.arg("--config").arg(config_path);
    cmd.env("SOVEREIGN_DATA_DIR", dir.join("data"));
    cmd
}

fn open(rt: &tokio::runtime::Runtime) -> SurrealGraphDB {
    rt.block_on(async {
        let db = SurrealGraphDB::new(StorageMode::Persistent(db_path())).await.expect("open");
        db.connect().await.expect("connect");
        db.init_schema().await.expect("schema");
        db
    })
}

// Persistent storage must survive across processes; mobile builds have no
// rocksdb (as `cli_integration.rs`).
#[cfg_attr(not(feature = "rocksdb"), ignore = "requires rocksdb for cross-subprocess persistence")]
#[test]
fn s5_2_delete_doc_without_the_account_refuses_and_deletes_nothing() {
    let _ = std::fs::remove_dir_all(temp_dir());
    std::fs::create_dir_all(temp_dir()).unwrap();

    // A document row, put there directly by a helper process (the CLI cannot
    // create one without the account either). A separate process, because
    // RocksDB keeps its lock until the process that opened it exits.
    let id = create_row_in_a_helper_process();

    let output = sovereign_cmd().arg("delete-doc").arg("--id").arg(&id).output().expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        !output.status.success(),
        "delete-doc without an account must refuse; it exited {:?}\nstdout: {stdout}\nstderr: {stderr}",
        output.status
    );
    assert!(stderr.contains("no account found"), "delete-doc stderr: [{stderr}]");

    // Nothing deleted, nothing trashed (read in this process: the CLI has
    // exited, so the lock is free).
    let rt = tokio::runtime::Runtime::new().unwrap();
    let db = open(&rt);
    let all = rt.block_on(db.list_documents_including_deleted()).expect("list");
    let row = all.iter().find(|d| d.id_string().as_deref() == Some(id.as_str()));
    let row = row.unwrap_or_else(|| panic!("the document {id} was hard-deleted"));
    assert!(row.deleted_at.is_none(), "the document was trashed without the account");
    assert_eq!(row.title, "Keep me");
    drop(db);
    drop(rt);

    let _ = std::fs::remove_dir_all(temp_dir());
}

/// Runs this test binary again, on [`helper_create_row`] only, and returns the
/// id it printed.
fn create_row_in_a_helper_process() -> String {
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["helper_create_row", "--exact", "--nocapture", "--test-threads=1"])
        .env(HELPER_ENV, db_path())
        .output()
        .expect("helper process");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(out.status.success(), "helper failed: {stdout} / {}", String::from_utf8_lossy(&out.stderr));
    stdout
        .lines()
        .find_map(|l| l.split("S5-2 ROW ").nth(1))
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_else(|| panic!("no id from the helper: {stdout}"))
        .to_string()
}

const HELPER_ENV: &str = "SOVEREIGN_S5_2_HELPER_DB";

/// Not a test of its own: does something only when run by
/// [`create_row_in_a_helper_process`] (the variable names the database).
#[test]
fn helper_create_row() {
    let Ok(path) = std::env::var(HELPER_ENV) else { return };
    let rt = tokio::runtime::Runtime::new().unwrap();
    let id = rt.block_on(async {
        let db = SurrealGraphDB::new(StorageMode::Persistent(path)).await.expect("open");
        db.connect().await.expect("connect");
        db.init_schema().await.expect("schema");
        let created = db.create_document(Document::new("Keep me".into(), "thread:t".into(), true)).await.expect("create");
        created.id_string().unwrap()
    });
    println!("S5-2 ROW {id}");
}
