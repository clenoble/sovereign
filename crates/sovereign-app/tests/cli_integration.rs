//! Integration tests for the sovereign CLI binary.
//!
//! Uses persistent mode with temp directories since in-memory dies with process.

use std::process::Command;

fn sovereign_cmd() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sovereign"));
    // Use a temp directory for the database to isolate tests
    let temp_dir = std::env::temp_dir().join(format!("sovereign-test-{}", std::process::id()));
    let config_content = format!(
        "[database]\nmode = \"persistent\"\npath = \"{}\"\n",
        temp_dir.join("test.db").display()
    );
    let config_path = temp_dir.join("config.toml");
    std::fs::create_dir_all(&temp_dir).unwrap();
    std::fs::write(&config_path, config_content).unwrap();
    cmd.arg("--config").arg(config_path);
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

#[test]
fn test_cli_round_trip() {
    // Create a thread
    let thread_out = run(sovereign_cmd()
        .arg("create-thread")
        .arg("--name")
        .arg("Research")
        .arg("--description")
        .arg("Research notes"));
    let thread_id = thread_out.trim();
    assert!(thread_id.starts_with("thread:"), "Got: {thread_id}");

    // Create a document in the thread
    let doc_out = run(sovereign_cmd()
        .arg("create-doc")
        .arg("--title")
        .arg("Meeting Notes")
        .arg("--thread-id")
        .arg(thread_id));
    let doc_id = doc_out.trim();
    assert!(doc_id.starts_with("document:"), "Got: {doc_id}");

    // Get the document
    let get_out = run(sovereign_cmd().arg("get-doc").arg("--id").arg(doc_id));
    assert!(get_out.contains("Meeting Notes"));

    // Update the document
    let update_out = run(sovereign_cmd()
        .arg("update-doc")
        .arg("--id")
        .arg(doc_id)
        .arg("--title")
        .arg("Updated Notes")
        .arg("--content")
        .arg("Some content here"));
    assert!(update_out.contains("Updated Notes"));

    // List documents
    let list_out = run(sovereign_cmd().arg("list-docs"));
    assert!(list_out.contains("Updated Notes"));
    assert!(list_out.contains("(1 documents)"));

    // List documents by thread
    let list_thread_out = run(sovereign_cmd()
        .arg("list-docs")
        .arg("--thread-id")
        .arg(thread_id));
    assert!(list_thread_out.contains("Updated Notes"));

    // List threads
    let threads_out = run(sovereign_cmd().arg("list-threads"));
    assert!(threads_out.contains("Research"));

    // Create a second document for relationship testing
    let doc2_out = run(sovereign_cmd()
        .arg("create-doc")
        .arg("--title")
        .arg("Source Doc")
        .arg("--thread-id")
        .arg(thread_id));
    let doc2_id = doc2_out.trim();

    // Add a relationship
    let rel_out = run(sovereign_cmd()
        .arg("add-relationship")
        .arg("--from")
        .arg(doc_id)
        .arg("--to")
        .arg(doc2_id)
        .arg("--relation-type")
        .arg("references")
        .arg("--strength")
        .arg("0.9"));
    assert!(
        rel_out.contains("related_to:"),
        "Relationship output: [{rel_out}]"
    );

    // List relationships
    let rels_out = run(sovereign_cmd()
        .arg("list-relationships")
        .arg("--doc-id")
        .arg(doc_id));
    assert!(
        rels_out.contains("relationships)"),
        "List relationships output: [{rels_out}]"
    );

    // Commit a document
    let commit_out = run(sovereign_cmd()
        .arg("commit")
        .arg("--doc-id")
        .arg(doc_id)
        .arg("--message")
        .arg("Initial setup"));
    assert!(
        commit_out.contains("commit:"),
        "Commit output: [{commit_out}]"
    );

    // List commits for the document
    let list_commits_out = run(sovereign_cmd()
        .arg("list-commits")
        .arg("--doc-id")
        .arg(doc_id));
    assert!(
        list_commits_out.contains("Initial setup"),
        "List commits output: [{list_commits_out}]"
    );

    // Clean up temp dir
    let temp_dir = std::env::temp_dir().join(format!("sovereign-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(temp_dir);
}
