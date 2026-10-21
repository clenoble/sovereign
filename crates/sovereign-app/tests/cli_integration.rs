//! Integration tests for the sovereign CLI binary.
//!
//! Uses persistent mode with temp directories since in-memory dies with process.
//!
//! Every run sets `SOVEREIGN_DATA_DIR` to its own temp directory: the CLI
//! looks for the account (`crypto/auth.store`) there, and a test must never
//! reach the user's real data directory. (Before C-DOC adoption step 3 the
//! document commands never looked for an account; now `create-doc` and
//! `update-doc` authenticate — they write through the document service over
//! the encrypted store — and on a machine with a real account the test would
//! wait for a passphrase.)

use std::path::PathBuf;
use std::process::Command;

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!("sovereign-test-{}", std::process::id()))
}

fn sovereign_cmd() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sovereign-tauri"));
    // Use a temp directory for the database to isolate tests
    let temp_dir = temp_dir();
    // Use forward slashes so backslashes don't get misinterpreted as TOML escape sequences
    let db_path = temp_dir.join("test.db").display().to_string().replace('\\', "/");
    let config_content = format!(
        "[database]\nmode = \"persistent\"\npath = \"{db_path}\"\n",
    );
    let config_path = temp_dir.join("config.toml");
    std::fs::create_dir_all(&temp_dir).unwrap();
    std::fs::write(&config_path, config_content).unwrap();
    cmd.arg("--config").arg(config_path);
    cmd.env("SOVEREIGN_DATA_DIR", temp_dir.join("data"));
    cmd
}

fn run(cmd: &mut Command) -> String {
    let output = cmd.output().expect("Failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        panic!(
            "Command failed with status {:?}\nstdout: {stdout}\nstderr: {stderr}",
            output.status
        );
    }
    stdout
}

/// The command fails; its stderr.
fn run_failing(cmd: &mut Command) -> String {
    let output = cmd.output().expect("Failed to execute command");
    assert!(!output.status.success(), "expected a failure, got {:?}", output.status);
    String::from_utf8_lossy(&output.stderr).to_string()
}

// Needs persistent storage to survive across subprocess invocations.
// Mobile builds turn rocksdb off and fall back to in-memory, which dies
// with each CLI process — the second invocation can't see the doc the
// first one created.
#[cfg_attr(not(feature = "rocksdb"), ignore = "requires rocksdb for cross-subprocess persistence")]
#[test]
fn test_cli_round_trip() {
    let _ = std::fs::remove_dir_all(temp_dir());
    // THREAD-sync S1 (decided by Céline 2026-10-18): threads are written only
    // through the document service's stamped path, so `create-thread`, like
    // `create-doc`, needs the account — none here (`SOVEREIGN_DATA_DIR` is
    // this test's temp directory): it refuses and nothing is written. The
    // creation itself is tested in `commands::tests::create_and_list_threads`
    // over an encrypted store. (Migrated by the THREAD-sync test author.)
    let err = run_failing(sovereign_cmd()
        .arg("create-thread")
        .arg("--name")
        .arg("Research")
        .arg("--description")
        .arg("Research notes"));
    assert!(err.contains("no account found"), "create-thread stderr: [{err}]");

    // List threads: still works, and lists nothing.
    let threads_out = run(sovereign_cmd().arg("list-threads"));
    assert!(!threads_out.contains("Research"), "list-threads output: [{threads_out}]");
    let thread_id = "thread:research";

    // C-DOC adoption step 3: documents are created and updated through the
    // document service, which needs the account — none here, so nothing is
    // written (they used to land as plaintext rows). The create and update
    // paths themselves are tested in `commands::tests` over an encrypted
    // store.
    let err = run_failing(sovereign_cmd()
        .arg("create-doc")
        .arg("--title")
        .arg("Meeting Notes")
        .arg("--thread-id")
        .arg(thread_id));
    assert!(err.contains("no account found"), "create-doc stderr: [{err}]");
    let err = run_failing(sovereign_cmd()
        .arg("update-doc")
        .arg("--id")
        .arg("document:x")
        .arg("--title")
        .arg("Updated Notes"));
    assert!(err.contains("no account found"), "update-doc stderr: [{err}]");
    let list_out = run(sovereign_cmd().arg("list-docs"));
    assert!(list_out.contains("(0 documents)"), "list-docs output: [{list_out}]");

    // Versions are the service's: `commit` is refused.
    let err = run_failing(sovereign_cmd()
        .arg("commit")
        .arg("--doc-id")
        .arg("document:x")
        .arg("--message")
        .arg("Initial setup"));
    assert!(err.contains("document service"), "commit stderr: [{err}]");

    // List commits of an unknown document: empty, not an error.
    let list_commits_out = run(sovereign_cmd().arg("list-commits").arg("--doc-id").arg("document:x"));
    assert!(list_commits_out.contains("(0 commits)"), "List commits output: [{list_commits_out}]");

    // Clean up temp dir
    let _ = std::fs::remove_dir_all(temp_dir());
}
