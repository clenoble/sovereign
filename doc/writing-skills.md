# Writing Third-Party Skills for Sovereign GE

This guide explains how to write, build, and distribute a skill plugin for Sovereign GE using the WASM Component Model.

## Overview

Skills are document-level transformations that users invoke from the UI. They can read and modify documents, produce files (e.g. PDF), return structured data (e.g. statistics), or interact with the Sovereign database — all from within a memory-safe, CPU-limited WebAssembly sandbox.

### How skills run

```
User clicks action in Skills panel
        │
        ▼
Tauri backend loads the WASM component
        │
        ▼
Creates a fresh Store (isolated memory, capped fuel)
        │
        ▼
Passes SkillDocument + params + granted capabilities
        │
        ▼
Skill runs: may call host-db functions, compute results
        │
        ▼
Returns SkillOutput → frontend renders result
```

Each execution gets a fresh `Store` — skills are stateless by design.

## Quick Start

### Prerequisites

```bash
rustup target add wasm32-wasip1
cargo install wasm-tools
```

### Scaffold a new skill

```
my-skill/
├── Cargo.toml
├── skill.json          # Manifest (metadata + capabilities)
├── src/
│   └── lib.rs          # Skill implementation
└── wit/
    └── skill.wit       # WIT interface definition (copy from repo)
```

### 1. Copy the WIT file

Copy `crates/sovereign-skills/wit/skill.wit` into your `wit/` directory. This is the contract your skill implements. Do not modify it.

### 2. Create `Cargo.toml`

```toml
[package]
name = "my-skill"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = "0.53"
```

The `cdylib` crate type is required for WASM component output.

### 3. Create `skill.json`

```json
{
    "name": "My Skill",
    "version": "0.1.0",
    "description": "What this skill does in one sentence",
    "author": "Your Name",
    "skill_type": "community",
    "capabilities": ["read_document"],
    "file_types": ["md", "txt"]
}
```

**Fields:**

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Human-readable display name |
| `version` | yes | Semver version string |
| `description` | yes | One-line description shown in the UI |
| `author` | yes | Author name or handle |
| `skill_type` | yes | Always `"community"` for third-party skills |
| `capabilities` | yes | Array of capabilities your skill needs (see below) |
| `file_types` | yes | File extensions this skill applies to. `[]` = universal. |

### 4. Implement the skill

```rust
// src/lib.rs

wit_bindgen::generate!({
    world: "skill-plugin",
    path: "wit",
});

struct MySkill;

impl Guest for MySkill {
    fn name() -> String {
        "my-skill".to_string()
    }

    fn required_capabilities() -> Vec<sovereign::skill::types::Capability> {
        use sovereign::skill::types::Capability;
        vec![Capability::ReadDocument]
    }

    fn actions() -> Vec<(String, String)> {
        // (action_id, display_label)
        vec![("analyze".to_string(), "Analyze Document".to_string())]
    }

    fn file_types() -> Vec<String> {
        vec!["md".to_string(), "txt".to_string()]
    }

    fn execute(
        action: String,
        doc: sovereign::skill::types::SkillDocument,
        params: String,
        _granted: Vec<sovereign::skill::types::Capability>,
    ) -> Result<sovereign::skill::types::SkillOutput, String> {
        match action.as_str() {
            "analyze" => {
                // Your logic here — doc.body contains the document text
                let result = format!(r#"{{"length": {}}}"#, doc.body.len());

                Ok(sovereign::skill::types::SkillOutput::StructuredData(
                    sovereign::skill::types::StructuredOutput {
                        kind: "analysis".to_string(),
                        json: result,
                    },
                ))
            }
            _ => Err(format!("Unknown action: {action}")),
        }
    }
}

export!(MySkill);
```

### 5. Build

```bash
# Compile to WASM
cargo build --target wasm32-wasip1 --release

# Convert core module → WASM component
wasm-tools component new \
    target/wasm32-wasip1/release/my_skill.wasm \
    -o my-skill.component.wasm
```

Note: the `.wasm` filename uses underscores (Cargo convention), but the component output can use hyphens.

### 6. Install

Place the skill in a subdirectory under the `skills/` directory at the Sovereign GE root:

```
skills/
└── my-skill/
    ├── skill.json
    └── my-skill.component.wasm
```

Sovereign GE discovers WASM skills on startup by scanning `skills/*/` for directories containing both a `skill.json` and a `.wasm` file.

## The WIT Interface

Your skill must implement the `skill-plugin` world defined in `skill.wit`:

### Exports (you implement)

| Function | Signature | Purpose |
|----------|-----------|---------|
| `name` | `() -> string` | Unique identifier for your skill |
| `required-capabilities` | `() -> list<capability>` | Capabilities you need |
| `actions` | `() -> list<tuple<string, string>>` | `(action_id, display_label)` pairs |
| `file-types` | `() -> list<string>` | File extensions (empty = universal) |
| `execute` | `(action, doc, params, granted) -> result<skill-output, string>` | Main entry point |

### Imports (host provides)

If your skill declares `read_all_documents` or `write_all_documents` capabilities, you can call these host functions:

| Function | Signature | Purpose |
|----------|-----------|---------|
| `search-documents` | `(query) -> result<list<search-result>, string>` | Full-text search across all documents |
| `get-document` | `(id) -> result<doc-detail, string>` | Fetch a single document by ID |
| `list-documents` | `(thread-id?) -> result<list<doc-entry>, string>` | List documents, optionally by thread |
| `create-document` | `(title, thread-id, content) -> result<string, string>` | Create a new document, returns ID |

## Capabilities

Capabilities are the permission model for skills. Declare only what you need — users will see what your skill requests.

| Capability | Description | Grants |
|------------|-------------|--------|
| `read_document` | Read the current document | Access to `doc.body`, `doc.title`, `doc.id` |
| `write_document` | Modify the current document | Return `ContentUpdate` to save changes |
| `read_all_documents` | Query across all documents | `search-documents`, `list-documents`, `get-document` host calls |
| `write_all_documents` | Create new documents | `create-document` host call |
| `read_filesystem` | Read files from disk | (reserved — not yet exposed to WASM skills) |
| `write_filesystem` | Write files to disk | (reserved — not yet exposed to WASM skills) |
| `network` | Make HTTP requests | (reserved — not yet exposed to WASM skills) |

**Principle of least privilege**: request only capabilities your skill actually uses. A word counter needs `read_document`. A duplicate skill needs `read_all_documents` + `write_all_documents`.

## Output Types

Your `execute` function returns one of four `SkillOutput` variants:

### `content-update(string)`

Returns updated body text that replaces the document content. Use for skills that transform or edit documents.

```rust
Ok(SkillOutput::ContentUpdate("# Modified content\nNew body text.".to_string()))
```

**Frontend behavior**: saves the updated document and refreshes the view.

### `file(file-output)`

Returns a binary file for the user to download.

```rust
Ok(SkillOutput::File(FileOutput {
    name: "export.csv".to_string(),
    mime_type: "text/csv".to_string(),
    data: csv_bytes,
}))
```

**Frontend behavior**: triggers a browser download (data is base64-encoded over IPC).

### `structured-data(structured-output)`

Returns JSON data to display in the UI (stats, search results, diagnostics).

```rust
Ok(SkillOutput::StructuredData(StructuredOutput {
    kind: "my_result_type".to_string(),
    json: r#"{"score": 42, "details": "..."}"#.to_string(),
}))
```

**Frontend behavior**: renders the JSON payload in a toast or modal.

### `none`

Side-effect only — no output to show.

```rust
Ok(SkillOutput::None)
```

**Frontend behavior**: closes the skills panel.

## Parameters

The `params` string is passed from the frontend when the skill action is invoked. For simple skills it's usually empty (`""`). For skills that need user input (e.g. find-replace), it carries a JSON payload:

```rust
fn execute(action: String, doc: SkillDocument, params: String, ...) -> ... {
    match action.as_str() {
        "find_replace" => {
            // Parse params as JSON
            let p: serde_json::Value = serde_json::from_str(&params)
                .map_err(|e| format!("Bad params: {e}"))?;
            let find = p["find"].as_str().unwrap_or_default();
            let replace = p["replace"].as_str().unwrap_or_default();
            // ...
        }
    }
}
```

Note: `serde_json` is not available in the WASM sandbox by default. Either use a no-std JSON parser, hand-parse simple JSON, or add `serde_json` as a dependency (it compiles to WASM but adds ~100KB).

## Sandbox Limits

WASM skills run under strict resource constraints:

| Resource | Default Limit |
|----------|---------------|
| Memory | 16 MB |
| CPU fuel | ~1 billion instructions |
| Instances | 10 |

If your skill exceeds these limits, execution is terminated and an error is returned to the user. These limits prevent runaway skills from impacting system performance.

## Calling Host Functions

To call into the Sovereign database from your WASM skill, use the generated bindings:

```rust
use sovereign::skill::host_db;

fn execute(action: String, doc: SkillDocument, ...) -> Result<SkillOutput, String> {
    // Search across all documents
    let results = host_db::search_documents("meeting notes")
        .map_err(|e| format!("Search failed: {e}"))?;

    for r in &results {
        // r.id, r.title, r.snippet
    }

    // Get a specific document
    let detail = host_db::get_document("document:abc123")
        .map_err(|e| format!("Get failed: {e}"))?;
    // detail.title, detail.thread_id, detail.content

    // List all documents in a thread
    let docs = host_db::list_documents(Some("thread:xyz"))
        .map_err(|e| format!("List failed: {e}"))?;

    // Create a new document
    let new_id = host_db::create_document(
        "Summary",
        "thread:xyz",
        "# Auto-generated summary\n..."
    ).map_err(|e| format!("Create failed: {e}"))?;

    // ...
}
```

Host function calls are synchronous and bridge to the Sovereign database. They require the corresponding capabilities to be declared in your manifest.

## Complete Example: Word Count

This is the reference WASM skill included in the repo at `skills/word-count-wasm/`:

**`skill.json`**:
```json
{
    "name": "Word Count (WASM)",
    "version": "0.1.0",
    "description": "Word count skill running in a WASM sandbox",
    "author": "Sovereign GE",
    "skill_type": "community",
    "capabilities": ["read_document"],
    "file_types": ["md", "txt"]
}
```

**`src/lib.rs`**:
```rust
wit_bindgen::generate!({
    world: "skill-plugin",
    path: "wit",
});

struct WordCountWasm;

impl Guest for WordCountWasm {
    fn name() -> String {
        "word-count-wasm".to_string()
    }

    fn required_capabilities() -> Vec<sovereign::skill::types::Capability> {
        use sovereign::skill::types::Capability;
        vec![Capability::ReadDocument]
    }

    fn actions() -> Vec<(String, String)> {
        vec![("count".to_string(), "Word Count".to_string())]
    }

    fn file_types() -> Vec<String> {
        vec!["md".to_string(), "txt".to_string()]
    }

    fn execute(
        action: String,
        doc: sovereign::skill::types::SkillDocument,
        _params: String,
        _granted: Vec<sovereign::skill::types::Capability>,
    ) -> Result<sovereign::skill::types::SkillOutput, String> {
        use sovereign::skill::types::{SkillOutput, StructuredOutput};

        match action.as_str() {
            "count" => {
                let body = &doc.body;
                let words = body.split_whitespace().count();
                let characters = body.chars().count();
                let lines = if body.is_empty() { 0 } else { body.lines().count() };
                let reading_time_min = ((words as f64) / 200.0).ceil() as u64;

                let json = format!(
                    r#"{{"words":{},"characters":{},"lines":{},"reading_time_min":{}}}"#,
                    words, characters, lines, reading_time_min
                );

                Ok(SkillOutput::StructuredData(StructuredOutput {
                    kind: "word_count".to_string(),
                    json,
                }))
            }
            _ => Err(format!("Unknown action: {action}")),
        }
    }
}

export!(WordCountWasm);
```

## Tips

- **Keep it small**. WASM modules should be lightweight. Avoid pulling in large dependency trees.
- **No `std::fs` or `std::net`**. WASM skills cannot access the filesystem or network directly — use host functions instead.
- **Test natively first**. Write your core logic as a normal Rust library with `#[cfg(test)]` tests, then wrap it for WASM.
- **Use `format!` over serde** for simple JSON output. It avoids the `serde_json` dependency.
- **Action IDs are stable identifiers**. Use lowercase kebab-case (`"find-replace"`, `"export-csv"`). Display labels are for the UI.
- **Return clear errors**. The `Err(String)` from `execute` is shown to the user. Be specific: `"Unknown action: foo"` not `"Error"`.

## Skill Discovery & Loading

At startup, Sovereign GE:
1. Registers the 10 built-in core skills (compiled Rust, no WASM)
2. Scans the `skills/` directory for `skill.json` manifests
3. For each subdirectory with both `skill.json` and a `.wasm` file, loads the WASM component
4. Caches metadata (name, capabilities, actions, file types) to avoid re-instantiation
5. All skills appear in the Skills panel in the UI

The build script `build-wasm-skills.sh` automates building all WASM skills in the `skills/` directory.

---

## Appendix: Third-Party Skill Ideas

A non-exhaustive list of skills we'd like to see built by the community. Complexity and capability requirements vary — some are straightforward read-only analyzers, others require database access or produce file exports.

### Text & Writing

| Skill | Description | Capabilities | Output |
|-------|-------------|--------------|--------|
| **Readability Score** | Flesch-Kincaid, Gunning Fog, Coleman-Liau indices | `read_document` | StructuredData |
| **Grammar Check** | Rule-based grammar and style suggestions (LanguageTool-style) | `read_document` | StructuredData |
| **Spell Check** | Dictionary-based spellcheck with suggestions | `read_document` | StructuredData |
| **Outline Extractor** | Pull heading structure into a nested outline | `read_document` | StructuredData |
| **Table of Contents** | Insert/update a TOC from headings | `read_document`, `write_document` | ContentUpdate |
| **Slug Generator** | Generate URL-friendly slugs from document title | `read_document` | StructuredData |
| **Lorem Ipsum** | Insert placeholder text of configurable length | `write_document` | ContentUpdate |

### Data & Analysis

| Skill | Description | Capabilities | Output |
|-------|-------------|--------------|--------|
| **JSON/YAML Formatter** | Pretty-print or minify JSON/YAML blocks in documents | `read_document`, `write_document` | ContentUpdate |
| **CSV to Markdown Table** | Convert CSV data into markdown tables | `read_document`, `write_document` | ContentUpdate |
| **Frontmatter Parser** | Extract and display YAML/TOML frontmatter as structured data | `read_document` | StructuredData |
| **Link Checker** | Extract all URLs from a document, list them with status | `read_document` | StructuredData |
| **Duplicate Finder** | Find near-duplicate documents in the workspace | `read_all_documents` | StructuredData |
| **Tag Extractor** | Auto-extract keywords/tags from document content | `read_document` | StructuredData |

### Export & Conversion

| Skill | Description | Capabilities | Output |
|-------|-------------|--------------|--------|
| **HTML Export** | Render markdown as standalone HTML file | `read_document` | File |
| **EPUB Export** | Package document(s) into EPUB format | `read_document` or `read_all_documents` | File |
| **LaTeX Export** | Convert markdown to LaTeX source | `read_document` | File |
| **Plaintext Export** | Strip all markdown formatting | `read_document` | File |
| **Slides Export** | Convert heading-delimited sections into presentation slides (HTML) | `read_document` | File |

### Document Automation

| Skill | Description | Capabilities | Output |
|-------|-------------|--------------|--------|
| **Template Stamp** | Fill document templates with variable substitution from params | `read_document`, `write_document` | ContentUpdate |
| **Changelog Generator** | Scan threads for document changes, produce a changelog | `read_all_documents` | ContentUpdate or File |
| **Meeting Notes Template** | Create structured meeting notes (attendees, agenda, action items) | `write_all_documents` | StructuredData |
| **Daily Journal** | Create a dated journal entry document in a "Journal" thread | `write_all_documents` | StructuredData |
| **Merge Documents** | Concatenate multiple documents from a thread into one | `read_all_documents`, `write_all_documents` | StructuredData |

### Security & Privacy

| Skill | Description | Capabilities | Output |
|-------|-------------|--------------|--------|
| **PII Detector** | Scan for emails, phone numbers, SSNs, addresses | `read_document` | StructuredData |
| **Redactor** | Replace detected PII with `[REDACTED]` markers | `read_document`, `write_document` | ContentUpdate |
| **Entropy Scanner** | Flag high-entropy strings (potential secrets/API keys) | `read_document` | StructuredData |

### Formatting & Style

| Skill | Description | Capabilities | Output |
|-------|-------------|--------------|--------|
| **Sort Lines** | Alphabetically sort lines or list items | `read_document`, `write_document` | ContentUpdate |
| **Remove Duplicates** | Deduplicate repeated lines or paragraphs | `read_document`, `write_document` | ContentUpdate |
| **Wrap/Unwrap Lines** | Hard-wrap at N columns or unwrap to single-line paragraphs | `read_document`, `write_document` | ContentUpdate |
| **Case Converter** | Title Case, UPPER, lower, camelCase, snake_case | `read_document`, `write_document` | ContentUpdate |
| **Strip Comments** | Remove HTML comments or code comments from documents | `read_document`, `write_document` | ContentUpdate |
| **Footnote Collector** | Gather inline footnotes and format as endnotes | `read_document`, `write_document` | ContentUpdate |

### Cross-Document

| Skill | Description | Capabilities | Output |
|-------|-------------|--------------|--------|
| **Backlink Map** | Find all documents that reference the current one | `read_all_documents` | StructuredData |
| **Orphan Finder** | List documents that are never linked to from other documents | `read_all_documents` | StructuredData |
| **Thread Summary** | Produce a bullet-point summary of all documents in a thread | `read_all_documents` | StructuredData |
| **Word Frequency** | Corpus-wide term frequency analysis | `read_all_documents` | StructuredData |

---

If you build a skill and want it listed here, open a PR adding it to `skills/` with a `skill.json`, `README`, and the `.wasm` component.
