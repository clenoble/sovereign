# 03 — Data Model

How Sovereign GE stores the user's corpus: the storage engine and where it lives, the records
(documents, threads, milestones, relationships, commits), how versions, deletion and AI-suggested
links work, and how an existing folder tree is brought in by bulk import.

*Consolidates:* specification v1.3 §JSON Schema Specification (Storage Model, Document Node
Structure, Intent Thread Manifest, Relationship Model, Spatial Positioning, Version Control) and
Open Questions 5–7 and 9; the project's archived plans (private: memory consolidation, phase 5,
the v0.0.9 bulk import, the storage benchmark spike and batched writes);
code at v0.0.9 (`main` = d02c353); `RELEASE_NOTES_v0.0.9.md`.

---

## Storage

**Status:** ✅ Built — embedded SurrealDB behind the `GraphDB` trait (code:
`crates/sovereign-db/src/{traits.rs,surreal.rs}`).

All records live in one embedded SurrealDB graph database per persona. There is no server
process and no per-file JSON store.

| Platform / use | Engine | Selected by |
|---|---|---|
| Desktop (native shell; the frozen Tauri desktop build too) | RocksDB | `rocksdb` cargo feature |
| Mobile (Android) | SurrealKV — pure Rust, no native dependencies | `surrealkv` feature, `rocksdb` off |
| Tests, `database.mode = "memory"` | In-memory | `StorageMode::Memory` |

A build with neither persistent feature falls back to in-memory storage with a warning; shipped
builds always carry one of the two (code: `crates/sovereign-db/src/surreal.rs`). On Android,
SurrealKV replaced the earlier in-memory key-value store in v0.0.6.

**Location.** The data directory is `SOVEREIGN_DATA_DIR` when set (the mobile entry point sets
it to the app sandbox), otherwise `~/.sovereign` (code: `crates/sovereign-core/src/lib.rs`). The
database sits at `<data dir>/data/sovereign.db`. The Tauri host (the mobile app) reads `database.mode`
(`persistent` | `memory`) and `database.path` from its config; a relative path is resolved
against the data directory (code: `crates/sovereign-app/src/setup.rs`,
`crates/sovereign-core/src/config.rs`). The native shell uses the default path directly (code:
`crates/sovereign-shell/src/crypto.rs`).

**Per-persona databases.** Each persona (primary, and the optional duress persona) has its own
physically separate database and its own key databases, so a session opened under one persona
cannot reach the other's rows. The persona model and its guarantees are in ch. 07.

**Layering.** Every caller holds a `GraphDB`. Three implementations stack:

| Layer | Role |
|---|---|
| `SurrealGraphDB` | Raw storage: queries, indexes, records as written |
| `EncryptedGraphDB` | Decorator: seals designated fields on write, opens them on read, maintains blind-index token hashes, stamps and verifies commit MACs (ch. 07) |
| `LayeredGraphDB` | Atomic swap: the app boots on the raw layer and swaps the encrypted decorator in after login, transparently to all holders |

Rows written before the swap stay plaintext until encrypted; the decrypt paths treat a row with
no nonce as plaintext. The CLI command `encrypt-data` encrypts existing plaintext documents
idempotently (code: `crates/sovereign-db/src/{layered.rs,encrypted.rs}`,
`crates/sovereign-app/src/cli.rs`). A `MockGraphDB` implements the same trait for unit tests.

*Replaces v1.x:* "Database: `data/sovereign.db/` (RocksDB directory)" relative to the working
directory, and the original per-file JSON design — the JSON schemas of v1.x were kept as the
"logical model"; this chapter describes the records as stored instead.

### Tables

**Status:** ✅ Built (code: `crates/sovereign-db/src/schema.rs`).

| Table | Record | Specified in |
|---|---|---|
| `document` | `Document` | this chapter |
| `thread` | `Thread` (a lane on the canvas) | this chapter |
| `milestone` | `Milestone` | this chapter |
| `related_to` | `RelatedTo` edge — user-created relationship | this chapter |
| `suggested_link` | `SuggestedLink` edge — AI-suggested relationship | this chapter |
| `commit` | `Commit` — one version of one document | this chapter |
| `row_recovery` | `RowRecovery` — prior value of a non-document row overwritten by a peer | ch. 05 |
| `purge_marker` | `PurgeMarker` — what stays of a purged document: its id and deletion stamp, no content | this chapter (C-DOC G13, G27) |
| `store_record` | A keyed value of the store: the one-time migration's record, the trash's start date | this chapter (*Content encoding*, C-DOC G27) |
| `contact`, `conversation`, `message` | Unified communications | ch. 13 |
| `entity`, `pii_record`, `share_record` | Personal-data vault and disclosure ledger | ch. 09 |

### Record identity

**Status:** ✅ Built (code: `crates/sovereign-db/src/schema.rs`, `surreal.rs`).

Record ids are SurrealDB record ids, exchanged as `"table:key"` strings (`thing_to_raw` /
`raw_to_thing`); the key is never backtick-escaped, so ids round-trip. Record-level reads and
writes check that an id names the expected table. Records arriving from a paired device are inserted under
their **original id** (`create_*_with_id`, idempotent: an existing id is left untouched), so the
same document has the same id on every device (ch. 05).

### Indexes

**Status:** ◐ Partial — 32 explicit indexes are defined at schema init; a few filtered fields
are not yet indexed (below).

SurrealDB does not index a field unless told to. The rule, from the storage benchmark spike:
**every field a query filters on carries an explicit index.** Without one, the thread query took
688 ms over 50,000 documents; with `idx_thread_id`, 0.12 ms (in-memory, SurrealDB 2.6). The
spike's other targets (single fetch < 5 ms, 2-hop traversal < 50 ms, title search < 100 ms,
50,000-document bulk insert < 30 s) all passed (source: the storage benchmark spike, in the
project's archived plans, private).

Indexes in this chapter's tables (code: `SurrealGraphDB::init_schema`):

| Table | Indexed fields |
|---|---|
| `document` | `thread_id`, `title`, `created_at`, `is_owned`, `deleted_at`, `pii_scanned_at` |
| `thread` | `deleted_at` |
| `commit` | `document_id`, `timestamp` |
| `milestone` | `thread_id` |
| `suggested_link` | `status` |

Filtered but not indexed today: `document.title_token_hashes` and `thread.name_token_hashes`
(blind-index search), `document.peer_review_pending`, `row_recovery.review_pending`, and the
`in`/`out` pair of `suggested_link` used by the duplicate check [whether SurrealDB indexes edge
endpoints implicitly: unverified]. Indexes for the communications and PII tables are listed in
ch. 13 and ch. 09.

### Batched relationship writes

**Status:** ○ Specified, not built.

Bulk relationship creation runs in transactions, not as one `RELATE` query per edge; acceptance
was "batch insert within 2× of single-insert benchmarks" (source: the project's archived plans,
private). As built, `create_relationship` issues one `RELATE`
per edge and no batch API exists on `GraphDB`. Nothing creates relationships in bulk today
(import creates none; consolidation creates at most five suggestions per cycle), so the
requirement is not yet load-bearing.

---

## Document

**Status:** ◐ Partial — the record is built and in use, and every writer stores format v2
through the document service (*Content encoding*). Ownership is the trust state of ch. 04, where
the assistant's adoption differs from G12 (*Ownership*); multi-party ownership is deferred (D3,
decided 2026-10-10).

A document is one node of the user's graph: a title, a body, the lane it belongs to, and the
metadata other subsystems attach to it. Fields are flat on the record (code:
`crates/sovereign-db/src/schema.rs`, `Document`).

### Fields as built

**Status:** ✅ Built — 47 fields. v1.3 had a conceptual schema only; most of these fields (soft
delete, encryption, reliability, PII, pinning, trust and provenance, sync, alerts) have no
counterpart in it.

| Group | Field | Type | Meaning |
|---|---|---|---|
| Identity | `id` | record id | `document:<key>` |
| Core | `title` | String | Ciphertext at rest under the encrypted layer |
| | `content` | String | The body (see *Content encoding*); ciphertext at rest under the encrypted layer |
| | `thread_id` | String | Lane membership (`thread:<key>`); may point to a deleted thread (see *Thread deletion*) |
| | `is_owned` | bool | Owned (control plane) vs external (data plane). See *Ownership* |
| | `created_at`, `modified_at` | DateTime UTC | Timeline position; import sets them from the original file |
| Canvas | `spatial_x`, `spatial_y` | f32 | Stored position (ch. 12) |
| | `pinned` | bool | Keeps the document surfaced; plaintext metadata |
| Versions | `head_commit` | Option\<String\> | Latest commit id |
| Deletion | `deleted_at` | Option\<String\> | RFC 3339 soft-delete marker; `None` = active |
| Encryption (ch. 07) | `encryption_nonce` | Option\<String\> | Nonce paired with `content`; `None` = plaintext |
| | `title_nonce` | Option\<String\> | Nonce paired with `title` |
| | `title_token_hashes` | Vec\<String\> | Per-database keyed hashes of title tokens (blind index for title search) |
| Web origin | `source_url` | Option\<String\> | The page's URL, set by the document service when a document is created with web provenance (the shell's "Save page", the app's web clip; the shell also puts the URL at the top of the body) |
| Reliability (legacy, ch. 04) | `reliability_classification` | Option\<String\> | "Factual" / "Opinion" / "Fiction" |
| | `reliability_score` | Option\<f32\> | 0.0–5.0 |
| | `reliability_assessment` | Option\<String\> | JSON rubric |
| | `assessed_at` | Option\<DateTime\> | Last assessment |
| PII (ch. 09) | `body_raw_encrypted` | Option\<String\> | Sealed pre-tokenization body, for gated reveal |
| | `body_raw_nonce` | Option\<String\> | Its nonce |
| | `pii_scanned_at` | Option\<DateTime\> | `None` = not yet scanned |
| Trust and provenance (ch. 04, C-DOC G12) | `trust` | Option\<String\> | "owned" / "external" / "undetermined"; `None` on rows from before the service, which read from `is_owned` |
| | `provenance_kind` | Option\<String\> | "written" / "assistant" / "web" / "import" / "peer" / "copy"; `None` = unrecorded (rows from before the service) |
| | `provenance_detail` | Option\<String\> | The URL (web), the source path (import), the device (peer), or a copy's chain of provenances |
| | `trust_decision`, `trust_decision_device`, `trust_decision_at` | Option\<String\>, Option\<String\>, Option\<DateTime\> | The user's last explicit trust decision: the trust chosen, the device, the time |
| Writes (C-DOC G4, G11) | `write_seq` | u64 | Bumped on every write of title or content; part of the revision a save names |
| | `last_writer` | Option\<String\> | Who wrote the current state (`editor:<session>`, `assistant`, `skill:<name>`, …), for the restore-point rule |
| Sync (ch. 05, C-DOC G24–G26) | `stamp` | Option\<String\> | Hybrid-logical-clock stamp of the write that produced this state (`wall_ms:counter:device`) |
| | `prev_stamp` | Option\<String\> | The stamp of the state this one replaced |
| | `stamp_history` | Vec\<String\> | Earlier stamps of this document (bounded) |
| | `author_device` | Option\<String\> | The device on which the current state was written |
| | `decision_stamp` | Option\<String\> | The stamp of the last trust decision |
| | `kept_versions` | Vec\<String\> | Concurrent local states lost to a remote state with a greater stamp, kept as versions (bounded) |
| Alerts (ch. 05, C-DOC G31) | `alert_device`, `alert_finding`, `alert_previous_version`, `alert_raised_at` | Option\<String\> ×3, Option\<DateTime\> | An open alert from the incoming-change analysis: the device the change came from, what was found (no document text), the version before the change, when |
| | `model_check` | Option\<String\> | A received change still waiting for the model's verdict: the sending device, the version before the change, the received state's stamp; no document text |
| Peer review (retired) | `peer_review_pending`, `peer_review_peer`, `peer_review_at`, `peer_review_prior_commit`, `peer_review_assessment` | bool, Option\<…\> ×4 | No production path sets them since the review flag was retired (2026-10-15, C-WIRE 13; C-DOC G19, G31): the alert fields and `kept_versions` replace them |

All fields added after the first schema are `#[serde(default)]`, so older rows deserialize
without migration. `Document::new` creates the default body `{"body":"","images":[]}`.

The `reliability_*` group is superseded by the `Assessment` record of ch. 04 and kept until that
record exists.

### Content encoding

**Status:** ✅ Built, released in v0.0.11 — decided 2026-10-10 (Céline). One reader for every form (`sovereign-docformat`), and readers, saves, creators, delete and sync go through the document service, which writes v2. The one-time migration runs at each persona's first unlock with its notice, its record is sealed at rest, the decoy is seeded in v2, and the deprecated field parser is gone (`sovereign-docformat` still reads v1 and raw, for received or restored content); a test refuses any write of an old form. The v2 editor is the shell's only editor (ch. 12). v0.0.10 had the tolerant read only (lot 0).

**Decision.** A document's `content` is a **versioned JSON document made of typed blocks**:
`{"format": "sovereign.doc", "v": 2, "blocks": [ {"type": "text", …}, {"type": "image", …},
{"type": "table", …}, … ]}`.
- Every block declares its `type`; nothing is guessed. The `blocks` array is the document's
  structure and index. A document can aggregate text, media, data tables, and later charts,
  graphs and drawings, each as its own block type with its own schema.
- **An unknown block type is preserved unchanged** by every reader and writer, so a newer
  block type never breaks or is lost in an older version.
- JSON because every language and script reads it, the code already uses it, and it stays
  readable by eye.
- **Migration:** the current envelope `{"body", "images", "videos"}` reads as v1; a raw-text
  body (written by the shell editor or bulk import) reads as a single text block. Each is
  converted to v2 at its next write, after a commit, and a one-time migration converts the rest.
  The tolerant read is a bridge, removed once the migration has run.
- **The one-time migration — decided 2026-10-18 (Céline, confirming the 2026-10-10 rule):** it runs
  once, in the background, at the first unlock after the update; when it is done a notice says how
  many documents were converted and that each one's earlier state is in its History. Until then a
  document is converted at its next write. Then the tolerant read is removed (adoption step 6).
- **What the migration cannot read, and saves during it — v0.0.12 item 7** (Céline, 2026-10-24:
  count them, list them, retry only after a version change):
  - A document row that exists but cannot be decrypted is **counted** in the notice and **left as
    it was**; nothing about it changes. A row whose purge is under way (an unrevoked purge marker
    for its id) is left out, readable or not.
  - **The notice, as built** (one wording for both hosts, `sovereign-docs`): "N documents converted
    to the new format — each one's earlier state is in its History." (for one: "1 document
    converted to the new format — its earlier state is in its History."). When some could not be
    read, the full stop gives way to "; M could not be read and were left as they are." (for one:
    "; 1 could not be read and was left as it is."). With nothing converted, only "M documents could
    not be read and were left as they are." (for one: "1 document could not be read and was left as
    it is."). A store whose conversion completed under v0.0.11 converts nothing more at its first
    unlock under v0.0.12 and gets that last form, once, when it finds rows it cannot read. A retry
    that converts at least one document says so in the same words, counting what it converted and
    what still cannot be read. A notice not yet shown when a later pass has something to tell is
    merged into the new one, never lost.
  - The desktop's Alerts window **lists** each one under "Documents that could not be read": a
    short id (the first 8 characters of its key) and its last modified date, "(in the trash)"
    where it applies, never a title. The status line says how many and that they are in Alerts.
  - **The Tauri app lists them too** (V2b, built: merged
    `9f99b7f`, 2026-10-25; PANDA2's contract, the laptop's second position). The heading, the line
    "Each one is kept as it was, untouched. Sovereign will try to read it again after its next
    update." and each document's line (short id, "(in the trash)", last modified date in local
    time) are worded in one place, in `sovereign-docs`, and both hosts use them; the shell's text
    is unchanged. The app's Alerts dialog (phone and Tauri desktop) shows them below the orphans:
    a heading, the meaning line, and a list labelled by the heading, with nothing to act on. While
    any remain, the dialog's notice bar says "1 document could not be read" (or "N documents…")
    as a status, with a button that opens the dialog (PANDA2's choice); it is not the one-time
    migration notice, and it stays while any remain. The list is loaded with the other alerts and
    again when a migration ends. A list that cannot be read gives an empty list, with a warning in
    the log (the shell leaves it empty too, without logging). The notice is announced as a status;
    its button, "See it" (or "See them"), opens the dialog. A locked session's request is refused
    and the app shows an empty list, so one persona's list never shows under the other; the app
    has no lock signal yet, so a store change empties the list at the next reload (backlog). The
    dialog stays open while this list is the only thing in it.
  - The migration's record keeps the build version that ran and the ids it could not read. Only an
    unlock under a **different** build version tries them again; a row that now decrypts is
    converted like any other and leaves the list. A store whose migration completed under v0.0.11
    is listed at its first unlock under v0.0.12, without converting readable rows that arrived
    since.
  - The migration takes the write lock **per document**, not for its whole run, so a save, a trash
    or a create waits for at most one document's conversion.
  - An editor holding the document gets no "changed elsewhere" from its own conversion: it takes
    the converted form silently when the row was last written by the migration and its plain text
    equals what the editor opened; any other change still warns. Text typed in the editor meanwhile
    is kept, and its next save is based on the converted state, so it makes no conflict (the
    desktop shell's editor).
- The block schemas are specified below, in *Document format v2 — blocks*.

What follows describes the forms in use up to v0.0.10.

The intended form of `content` is a JSON envelope (code: `crates/sovereign-core/src/content.rs`,
`ContentFields`):

```json
{ "body": "markdown text", "images": [{ "path": "…", "caption": "…" }],
  "videos": [{ "path": "…", "caption": "…", "duration_secs": 12.0, "thumbnail_path": "…" }] }
```

Media are referenced by path; they are not stored in the database.

> *Decided since (above); kept as the record of the conflict, as of v0.0.9.* Writers disagreed on what `content` holds. The
> native shell saves edits as raw text but creates documents, web clips and skill output as
> the envelope, and it reads and shows `content` as raw text (code:
> `crates/sovereign-shell/src/{app.rs,canvas.rs,panels.rs}`). Bulk import writes raw markdown
> (code: `crates/sovereign-import/src/lib.rs`). Mobile reads and writes the envelope and shows a
> raw body as empty (code: `crates/sovereign-app/src/tauri_commands/documents.rs`). Sync copies
> `content` as it is, so both forms meet on every device. `ContentFields::parse` falls back to an
> empty body when `content` is not JSON. Traced by code reading, not reproduced at runtime, the
> effects are:
>
> - a document written raw (a shell edit, an import) reads as empty on the phone, and saving it
>   there replaces the body with that empty one;
> - a document written as the envelope shows as JSON in the shell, including in its edit buffer;
> - a shell skill that rewrites a document parses a raw body as empty, so it runs on an empty
>   body and saves the result over the real one.
>
> v0.0.10 made reads tolerant of a bare body; since v0.0.11 the service reads every form with one
> reader and writes v2. One case remains by construction: a legacy raw-text document whose whole
> text is a JSON object with a string `body` reads as an envelope, and keeps only that body at
> its next write.


### Document format v2 — blocks

**Status:** ✅ Built, released in v0.0.11 — specified 2026-10-10; choices marked **[choice]**
remain to be reviewed by Céline (code: `crates/sovereign-docformat`). The editor shows an image,
video or table block, and a block of an unknown type, as a placeholder (code:
`crates/sovereign-editor/src/view.rs`); nothing creates a table block yet.

**Envelope.**
```json
{ "format": "sovereign.doc", "v": 2, "blocks": [ … ] }
```
- `format` is always `"sovereign.doc"`; `v` is the format version. A reader that meets a `v`
  higher than it knows **opens the document read-only and never rewrites it**.
- `blocks` is ordered; the order is the document's order. An empty document has `"blocks": []`.
- Unknown top-level fields are preserved on write.

**Every block** has:
- `type` — a string naming the block type. Required.
- `id` — a random 128-bit identifier (hex), unique within the document, **stable across edits**:
  editing a block keeps its id; a new block gets a new one. Ids let undo, history, sync and
  review talk about one block rather than the whole document.
- any further fields defined by its type. Fields a reader does not know are preserved.

**Unknown block types** are preserved exactly — same position, same JSON value — by every reader
and writer. The editor shows them as a read-only placeholder ("this kind of block is not
supported in this version") and removes one only by an explicit user action on that block.

**Stable serialisation.** Writing a document whose blocks did not change produces the same
bytes: key order and number formatting are preserved for unchanged blocks. Otherwise every save
looks like a change to history and to sync.

**Block types in v2.**

| `type` | Fields | Notes |
|---|---|---|
| `text` | `markup`: `"markdown"`; `text`: string | **[choice]** v2 stores markdown source. Rich text (styled spans) would be a later `markup` value, not a new format. Personal-data tokens `[pii:<record_id>]` (ch. 09; ids contain `:`) live inside `text` and are atomic: the editor never lets a token be partly edited. |
| `image` | `ref`: media reference; `alt`: string (required; empty only with `decorative: true`); `caption`: string | `alt` serves screen readers (WCAG). |
| `video` | `ref`; `caption`; `duration_secs` (optional); `thumbnail_ref` (optional) | |
| `table` | `columns`: [{`name`, `kind`}]; `rows`: [[cell, …]] | **Decided 2026-10-10 (Céline): in v2 only if cells are typed values** — a number is stored as a number, not a string, and calculations over cells work. Otherwise the block waits for a later version. A table of strings with a display hint does not qualify. |

**Media reference** (`ref`):
- **Through v0.0.12:** `{ "path": "<local path>", "sha256": "<hex>" }`. Media were not copied (the
  originals stayed where they were), and the hash let a moved or altered file be detected.
- **From v0.0.13 (C-MEDIA, *The media store*, below):** `{ "store": "<the medium's name in the
  store>", "sha256": "<hex>" }`. A media block names no outside path.
- **A device still on v0.0.12 does not receive such a block.** The format changed, so the wire
  version went up, and documents are not exchanged with an older build (ch. 05, *Peers on an older
  build*). Its own reader would take the block for one of an unknown kind; what its editor writes
  back after an edit was not traced.

**Reading the older forms (migration).**
- The v1 envelope `{"body", "images", "videos"}` → one `text` block holding `body`, then one
  `image` / `video` block per entry, in that order. v1 media have neither a description nor a
  hash: the migrated image block carries an empty `alt` flagged *needs a description*, and
  `ref.sha256` stays absent until the document service computes it (the editor never reads
  files). Such a block is valid in that state. *(Decided 2026-10-10.)*
- A raw-text body → one `text` block holding the whole string.
- Conversion happens at the document's next write, after a commit; a one-time pass converts the
  rest; then the tolerant read is removed. As built, the old field parser is gone, and
  `sovereign-docformat` still reads both forms, for content received from an older peer or
  restored from an old version (C-DOC G2).

**What the format does not do.** It does not encrypt (the encrypted layer encrypts `content`
whole, ch. 07); it does not hold the title, thread or trust state (record fields, ch. 03, ch. 04);
it does not version itself (history is the commit chain).
### Ownership

**Status:** ◐ Partial — the trust state (owned / external / undetermined) and its provenance are
recorded by the document service (C-DOC G12); the assistant's "adopt" differs from G12 (below);
the adoption gestures have no surface yet.

`is_owned = true` marks the user's own content: control plane, trusted, not injection-fenced,
drawn as a rounded rectangle. `false` marks external content: data plane, fenced, drawn as a
parallelogram (ch. 08, ch. 12).

The trust state is set when the document is created, by the surface that creates it, through the
document service (`trust`, with `provenance_kind` and `provenance_detail`); the service keeps
`is_owned` true only when the trust is owned. A row from before the service has no `trust` and
reads as owned or external from `is_owned`, provenance *unrecorded*. The service's `set_trust`
changes trust in either direction and records the decision, which travels by sync (G12); no
surface calls it yet. A document arriving by sync takes the sender's trust decision or, without
one, the sender's trust; a sender that sends no trust gives undetermined, and content from
outside (web, import) that arrives owned without a decision lands undetermined (code:
`crates/sovereign-docs/src/service.rs`). Requirement: a document's ownership is the same on
every device.

*Where the code differs from G12:* the assistant's "adopt" calls the store's `adopt_document`,
which sets only `is_owned` to true. It records no trust, decision or stamp, so the row's `trust`
and `is_owned` then disagree, and the adoption does not travel to paired devices (code:
`crates/sovereign-ai/src/orchestrator.rs`, `crates/sovereign-db/src/surreal.rs`).

The tri-state — *undetermined, pending the user's judgment*, failing closed — and when
ownership is decided for imported content are specified in ch. 04 (*Ownership is not decided at
landing*). See also *Bulk import → Ownership at import* below.

### ○ Deferred (D3) — multi-party ownership

**Status:** ○ Deferred — D3 decided 2026-10-10: multi-party ownership is a future
collaboration chapter, kept separate from the trust state (owned / external / undetermined,
ch. 04), which is built first. The v1.x block below is kept as that chapter's starting intent.

v1.x described each document with an ownership block, not a flag:

```json
"ownership": {
  "creator": "user_id", "current_owner": "user_id",
  "admins": ["user_id"], "contributors": ["user_id"],
  "provenance_chain": [{ "from": "user_id", "to": "user_id",
                          "timestamp": "ISO-8601", "action": "fork | transfer" }]
},
"sovereignty": { "trust_level": "owned | imported | external_snapshot",
                 "origin_url": "url | null", "import_method": "user_action | ai_ingest | api" }
```

> *Decided since (D3, 2026-10-10, Céline; ch. 15).* Trust — three states, reversible, with
> provenance — is built separately from multi-party ownership (K9, decided the same day;
> *Ownership*, C-DOC), and multi-party ownership waits for a future collaboration chapter. The
> code has the trust state, `is_owned` and `source_url`, and nothing of this block. The v1.x
> thread `members` block (see *Thread*) is the same question at thread level [link to D3
> inferred, not stated in D3].

### v1.x document fields not built

**Status:** ○ Specified, not built.

| v1.x field | Today |
|---|---|
| `node_type` (document / media / annotation / relationship) | One record type; media are paths inside `content`; relationships are edges |
| `content.mime_type`, `primary` (base64 / text / file reference) | `content` is text; no MIME field |
| `extracted_metadata.tags` | Not built |
| `extracted_metadata.semantic_embedding` | Not built — semantic search via embeddings is roadmap |
| `relationships.outbound[].snapshot` | Not built — see *Link snapshots* |
| `version_history` (author, diff, branches) | See *Version control* |
| `pending_edits` | ✗ Retired — replaced by last-writer-wins by stamp, the overwritten state kept as a version, and an alert when the incoming-change analysis finds something (ch. 05; C-DOC G19, G26, G31) |

---

## Thread

**Status:** ✅ Built (code: `crates/sovereign-db/src/schema.rs`, `Thread`; `surreal.rs`).

A thread is an intent container — a project or topic — rendered as a lane on the canvas. It
replaces the folder; documents relate across threads through edges.

| Field | Type | Meaning |
|---|---|---|
| `id` | record id | `thread:<key>` |
| `name`, `description` | String | Ciphertext at rest under the encrypted layer |
| `created_at` | DateTime UTC | |
| `modified_at` | DateTime UTC | Time of the last change, for display; defaults to now for rows older than sync. Across devices it orders only a row with no `stamp` |
| `deleted_at` | Option\<String\> | Soft-delete marker |
| `stamp` | Option\<String\> | The hybrid-logical-clock stamp of the thread's last create, rename or deletion (`wall_ms:counter:device`); across devices the later stamp wins (*Thread deletion and the fallback lane*, 2026-10-18). A row without one is ordered as if stamped at its modification time |
| `name_nonce`, `description_nonce` | Option\<String\> | Nonces; `None` = plaintext |
| `name_token_hashes` | Vec\<String\> | Blind index for find-by-name |

Operations (`GraphDB`): create, rename / re-describe (`update_thread`), find by name, move a
document to another thread, **merge** (all documents of the source move to the target, then the
source is soft-deleted), **split**, soft delete and restore. As built, the orchestrator's
`split_thread` intent creates the new, empty thread and moves no documents; the user is meant
to move documents afterwards (code: `crates/sovereign-ai/src/orchestrator.rs`).

**Merging keeps the source thread — contract (C-MERGE).** ● Built in v0.0.13 (lot B1, merged
2026-11-17), specified 2026-11-13 and revised 2026-11-17. Its second run, on the branch merged with main,
read each guarantee's host: the shell's drop and lane, the phone's lane, the assistant's tools and
context, the undo by name (MG5), the level (MG6), and an older peer's rows end to end (MG4).

Céline, 2026-10-25: « It should not delete the source thread, which should continue to exist on the
canvas in the past. It will just have an end date with an indication that it has been merged into
another thread. » Until v0.0.13, a merge, which only the assistant offers, moved every document and
soft-deleted the source, and no host offered its undo.
- **MG1 — Nothing is deleted.** A merge gives the source two fields, its end date and the thread it
  was merged into. No document moves, and the source is not deleted. The target records nothing.
- **MG2 — The source's lane stops at its end date and shows its mark,** « merged into '{target}' on
  {YYYY-MM-DD} », which is also its accessibility label, after its name.
  - Its documents stay in it, and stay editable. Their cards are drawn no later than the end date,
    even when edited after the merge: the thread goes on existing « in the past ».
  - The end date is the merge stamp's time, so that every device draws the same lane; the day shown
    is in the device's local time.
  - On the phone, which shows one lane at a time, the ended lane shows its mark.
- **MG3 — An ended thread takes no new or moved document, and is the target of nothing.**
  - No picker, menu, drop or assistant offers it for a document, and a merge into it is refused.
  - **Its live end** is the thread at the end of its chain of merges, followed within a bound (the
    number of threads). A drop on its lane, a find-or-create by its name and a duplicate of one of
    its documents go to its live end. Where a host already names the thread a document went to, it
    adds « ('{name}' was merged into it) »; a drop in the shell says it in a notice, since the
    document did not go where it was dropped.
  - A request that names it is answered that it was merged, on its date, and its live end is
    proposed. Nothing is changed.
  - **With no live end** (a loop that two devices' merges made, or a deleted end), the request or the
    drop is refused and said. Nothing is changed.
  - An ended thread is not a live thread of its name, for find-or-create or for an ambiguity (AT2).
  - A restore from the trash goes back to the ended thread: it is the document's own. An ended thread
    can still be renamed, and it counts among the threads, as its lane is drawn.
- **MG4 — The two fields travel with a stamp of their own.** A received row changes the merge only by
  that stamp. **A row without it never changes the merge,** so a rename and a merge made on two
  devices combine.
  - **An older peer (v0.0.12)** ignores the fields and their stamp, and keeps showing the source as a
    live thread. A rename made there no longer undoes the merge.
  - **A v0.0.13 device that hears of a thread only through an older peer** shows it live until it
    syncs with a device that holds the merge. Nothing is undone; the news is late.
  - **A merge made on an older peer** soft-deletes the source, and that deletion reaches the newer
    devices; its document moves do not, since documents are not exchanged across wire versions
    (ch. 05). So on a newer device the source arrives deleted and its documents are orphans: the
    Unfiled lane and the alert.
  - **The notes say:** update every device before merging.
- **MG5 — A merge can be undone, by asking the assistant** (Céline, 2026-11-17: « Assistant seul »:
  no new gesture). Undoing it clears the two fields under a new merge stamp.
  - **At any time and from any device, by naming the thread.** The two fields are the whole merge, so
    the undo needs no record kept in memory, and it outlives a restart. « Undo that », after the
    merge (AT7), is the same operation.
  - **It is refused if the thread is not ended,** and the reply says so.
- **MG6 — Level 3 (Modify)** (Céline, 2026-11-13: « Niveau 3 normal »). A merge is reversible, so it
  leaves level 5. It follows the level's usual rules, earned trust included, and an auto-approved
  merge keeps its undo (AT7).
  - **The approvals counted while a merge was level 5 do not count:** its trust starts at zero in
    v0.0.13.
  - This replaces the rule of v0.0.12, « merging always asks », which held while a merge deleted.
- **The words** (the Fable maille, by delegation of Céline, 2026-11-17), with « 1 document » in the
  singular:
  - the proposal: « Merge thread '{from}' into '{into}': '{from}' ends now and keeps its {n}
    documents; new ones go to '{into}' »; with none, « …: '{from}' ends now; it holds no documents »;
  - the outcome: « Merged '{from}' into '{into}' ({n} documents stay in '{from}') »; with none,
    « Merged '{from}' into '{into}' »;
  - a request that names an ended thread: « '{name}' was merged into '{target}' on {YYYY-MM-DD} and
    takes no new document. Use '{live end}' instead? Nothing was changed. »;
  - no live end: « '{name}' has ended (merged into '{target}'), and no live thread takes its place.
    Nothing was changed. »;
  - a merge into an ended thread: « '{into}' has ended (merged into '{target}'): merge into
    '{live end}' instead. Nothing was changed. »;
  - a merge undone: « Undid the merge of '{name}' into '{target}': '{name}' is live again »;
  - the undo refused: « thread '{name}' is not merged into another thread; nothing was undone »;
  - in these sentences '{target}' is the thread it was merged into, with that merge's date, and
    '{live end}' the end of its chain; without a chain they are the same thread;
  - a drop redirected, in the shell, a notice « Moved »: « '{title}' is now in '{target}' ('{name}'
    was merged into it). »

**Find or create by name** (v0.0.12 item 6: Céline's rule for the assistant, 2026-10-24, "Nom
exact seulement, sinon il demande", carried to every caller; for the callers that cannot ask, a
choice made with the test author). Callers that find a thread by name or create it, where
nobody can be asked "which one?" — a skill's thread (the daily journal's included), the bulk
import's folder threads, the browser's "Web" thread — reuse the one live thread whose name equals
the name exactly, ignoring case and surrounding spaces ("Work" never resolves to "Work Archive"),
and create the thread when there is none. When two or more live threads have that exact name, none is picked: the call is
refused ("N threads are named '{name}'; none was picked — rename one of them first"); the import
resolves every name before writing anything, so such a name refuses the whole import (code:
`crates/sovereign-db/src/thread_names.rs`, `existing_thread_for_find_or_create`). The assistant's
own actions on a thread by name follow ch. 08, *Action targets*.

**Two threads of the name sought: asked once, remembered** (Céline, 2026-11-13: « Demander une fois,
retenir »). **Built in v0.0.13** (item 9, merged 2026-11-19; specified 2026-11-18). Until v0.0.13 such a name refused
every call for good: two devices that each created « Web » or « Journal » before their first sync
stopped saving pages and journal entries until one thread was renamed. For the browser's « Web »
thread, the daily journal and the import:
- **FC1 — The first time, Sovereign asks which.** When two or more live threads have the exact name
  sought, the host that runs the call asks which one to use, listing each with its number of
  documents and its creation date in local time, in History's format (as ch. 08, AT2 and AT11 do), and
  saying that the choice will be remembered. Nothing is written for that call until she answers; no
  answer, nothing is written and nothing is remembered.
- **FC2 — The choice is remembered.** The next calls for that name use the thread chosen, without
  asking.
- **FC3 — Where it is kept.** In the session's own store, sealed at rest as a thread's name is
  (ch. 07): the name as find-or-create compares it, the thread chosen, and the live threads of that
  name it was chosen among. It is this persona's and this device's: it does not travel to the other
  devices, and the other persona never sees it.
- **FC4 — What forgets it.** The choice holds while the live threads of that name are exactly the
  ones it was chosen among. When that changes, it is forgotten: one of them deleted, merged away or
  renamed out of the name (the one chosen included; a rename that changes only case or surrounding
  spaces keeps the name as compared, and the choice), or a third thread of the name, made here or
  received by sync.
  Then one live thread of the name is used without asking, and two or more ask again.
- **FC5 — The import asks too, instead of refusing.** It asks, for each name two live threads share,
  before writing anything, and remembers each answer as FC2 says. Where nobody can answer (the bulk
  import run without a terminal), it refuses the whole import as before, and says so.
- **FC6 — A call nobody can be asked about is refused and said.** Where no window can ask (a call
  made in the background with no host prompt), the call is refused with the sentence above, and
  the refusal is shown to the person, not only written to the log.

Requirement: a thread's name and description are sealed at rest whichever operation creates the
thread (ch. 07). ◐ — not every creation path does so yet; tracked in the private backlog.

### Thread deletion and the fallback lane

**Status:** ✅ Built, released in v0.0.11 — the decision below (adoption step 5, 2026-10-17), the
warning for a document whose thread was deleted (S5-12, `ed719c7`, 2026-10-18: computed, in both
sync directions; thread tombstones are kept), and the later action winning across devices
(2026-10-18), which also settles a thread deleted here and renamed offline on the other device.
Restoring a thread a peer deleted works since v0.0.12 (item 6, A4). One lane per thread since
v0.0.12 (ch. 12, *Y — thread lanes*).

**Decided 2026-10-17 (Céline).** Deleting a thread moves **every document still in it to the
trash** — not to the fallback lane — and **deletes the thread**: nothing on this device restores
it. To bring a thread back, the user creates a new thread and assigns the documents to it from the
trash. It comes back only by a later action on another device (below). The confirmation says
that all the thread's documents will be placed in the trash (with the thread's name and its number of documents, ch. 08 AT5). The
thread record survives only as what sync needs to carry the deletion to paired devices, never
shown (ch. 05). The fallback lane remains for documents whose thread never existed locally.
**Across devices (decided 2026-10-18, Céline):** for the same thread, **the later action wins** —
a rename made after a deletion on another device brings the thread back under that name, a
deletion made after a rename keeps it deleted; every thread write (create, rename, delete) carries
a clock stamp for this. A deleted thread **shrinks to its id** after 30 days, once no document
(live or in the trash) points to it: its name is removed from disk. *As built (released in
v0.0.11):*
- **One stamped path.** Every thread write, on every path that writes one (the shell, the
  assistant, the phone, skills, import, backup restore, the decoy's seeding), goes through the
  document service's one thread-write path, stamped by the same device clock as documents' writes
  (G25); the stamp travels in the row, a field added with a default. How rows are compared,
  unstamped ones included, and the 5-minute bound on thread stamps: ch. 05, *Rows*.
- **A rename that wins over a deletion** brings the thread back live under the newer name. The
  documents the deletion sent to the trash stay there (restorable into the thread, now shown under
  its live name); its live documents stop asking what to do; nothing is moved. A deletion that
  wins over a rename keeps the thread deleted, and this device's live documents of it ask.
- **The shrink.** The 30 days run from the physical time of the deletion's stamp, wherever the
  deletion was made, so paired devices shrink it at about the same time; the purge tick does it.
  What remains — the id and the deletion stamp, no name, no description — is shown nowhere and
  stays in the sync manifest: a received row for that id with an older stamp is refused, one with
  a newer stamp (a rename made after the deletion) re-creates the thread with what it carries. A
  document arriving for a thread already shrunk still asks, worded without a name: "“{title}” —
  its thread was deleted."

A thread is renamed in its lane's label or by the assistant. ✅ Built, released in v0.0.11
(`24c25a3`, 2026-10-18); the rename in the lane label too (`7dd8fbe`). **A thread a peer deleted** is restored from the list of
previous values (ch. 05, *Non-destructive peer overwrite*): it comes back live, with its name, by a
stamped write later than the deletion, so the restore travels like any thread write; its documents
in the trash stay there until restored (v0.0.12 item 6, A4). A restore that fails is said, where it
was asked, and changes nothing (built in v0.0.13, item C2, merged 2026-11-18; ch. 05, same section).
Deleting a thread is all or nothing: if one of its documents cannot go to the trash, the others
come back and the thread stays. In the trash, a deleted thread's documents are grouped, and can
be restored all at once into a thread the user chooses, or one by one. A document that arrives by
sync for a thread deleted here (written on another device meanwhile) raises a warning asking the
user what to do with it — move it to another thread, create a new thread for it, or put it in the
trash; until then it stays visible.

*The fallback lane, as built:* any document whose `thread_id` does not resolve to a live thread —
its thread never existed here, or was deleted here while the document arrived by sync — is drawn
in one shared fallback lane, "Unfiled", which appears only while such a document exists.

- **Native shell:** one lane per live thread, with no cap, then Unfiled after the threads' lanes
  when needed (since v0.0.12; ch. 12, *Y — thread lanes*) (code:
  `crates/sovereign-shell/src/canvas.rs`).
- **Mobile:** one lane per thread, then Unfiled when a document needs it (code:
  `frontend/src/lib/components/mobile/MobileCanvas.svelte`).

*Replaces v1.x / Phase 5 plan:* hard delete of the thread record with an "Uncategorized" lane;
deletion is now soft.

### v1.x thread manifest not built

**Status:** ○ Specified, not built.

v1.x gave each thread a manifest with `members` (creator, admins, contributors), a per-document
`role` (proposal / research / draft / review / custom) and `spatial_position`, and a `timeline`
of thread-wide snapshots (`auto | user_checkpoint | branch_point`) with branches. None of these
exist: membership is the single user (see D3 above), positions are per document (ch. 12), and
versions are per document (below).

---

## Milestone

**Status:** ✅ Built (code: `crates/sovereign-db/src/schema.rs`, `Milestone`).

A milestone marks a significant point in a thread's history on the timeline: `title`,
`timestamp` (creation time), `thread_id`, `description`. Created through the orchestrator's
`create_milestone` intent (code: `crates/sovereign-ai/src/orchestrator.rs`), listed per thread
or globally, deletable, and synced (ch. 05). Rendering: ch. 12. Milestones are not
field-encrypted; what is sealed at rest is defined in ch. 07.

---

## Relationships

**Status:** ◐ Partial — typed edges and suggested links are built; link snapshots are not.

### Typed edges (`related_to`)

**Status:** ✅ Built (code: `crates/sovereign-db/src/schema.rs`, `RelatedTo`, `RelationType`;
`surreal.rs`).

A relationship is a directed SurrealDB graph edge `in → related_to → out` carrying
`relation_type`, `strength` (0.0–1.0, for weighted queries) and `created_at`. Eight types:

| Type | Meaning |
|---|---|
| `references` | Citation or reference |
| `derivedfrom` | Forked from, or based on, another document |
| `continues` | Sequential continuation |
| `contradicts` | Conflicting information |
| `supports` | Supporting evidence |
| `branchesfrom` | Version branch point |
| `contactof` | Links a contact to a document |
| `attachedto` | Attachment |

Types serialize lowercase; parsing also accepts the underscored forms (`derived_from`, …).
Queries: outgoing, incoming (backlinks), all, and a breadth-first `traverse(doc, depth, limit)`
that skips soft-deleted documents. The CLI `add-relationship` defaults strength to 0.5.
Relationship edges are not field-encrypted (ch. 07).

### Link snapshots (asymmetric bidirectional links)

**Status:** ○ Specified, not built.

v1.x: when A links to B, A stores a snapshot of B (`captured_at`, `content_hash`, `preview`), so
A keeps B's content if B is deleted or changed; B knows it is referenced but holds no snapshot,
and B may not have access to A. The built edge carries no snapshot, and the access clause
presupposes multiple users (D3).

### Suggested links (`suggested_link`)

**Status:** ✅ Built (code: `crates/sovereign-db/src/schema.rs`, `SuggestedLink`;
`surreal.rs`).

AI-suggested relationships live in their own edge table, structurally distinct from the user's
`related_to` edges, so "what the AI proposed" and "what the user asserted" never mix and can be
queried apart. A suggestion carries `relation_type`, `strength`, `rationale` (the model's
one-sentence reason), `source` (`consolidation` | `chat`), `status`, `created_at`,
`resolved_at`.

Lifecycle:

- **`pending`** — created by consolidation (below). `chat` exists as a source but no chat path
  creates suggestions yet.
- **`accepted`** — the user accepts: status and `resolved_at` are set, and a real `related_to`
  edge is created **in the suggested direction**, with the same type and strength.
- **`dismissed`** — the user dismisses: the pair is never suggested again. The duplicate check
  covers every status and both directions.

Accept and dismiss are commands of the Tauri host (code:
`crates/sovereign-app/src/tauri_commands/suggestions.rs`); the frozen Tauri desktop UI surfaced
them. The mobile app loads pending suggestions and shows their number as a badge in the chat
sheet; it offers no accept or dismiss, since the suggestion panel belongs to the desktop layout
only (code: `frontend/src/lib/components/mobile/MobileChatSheet.svelte`; the panel is mounted in
`frontend/src/routes/+page.svelte`, which `frontend/src/routes/+layout.svelte` replaces with the
mobile shell on a phone). ◐ The native shell has no suggestion surface yet.

*Replaces v1.x:* nothing — v1.3 did not describe suggested links.

---

## Version control

**Status:** ◐ Partial — per-document chains, integrity and restore are built; auto-commit differs by surface; branching is not built.

### Per-document commit chain

**Status:** ✅ Built (code: `crates/sovereign-db/src/{schema.rs,surreal.rs}`, `Commit`,
`DocumentSnapshot`).

Versions are kept **per document**, as a linear chain. A commit records `document_id`,
`parent_commit` (the document's previous `head_commit`), `message`, `timestamp`, a full
`snapshot`, and `signature`. Committing creates the commit and moves the document's
`head_commit` to it. Commits are listed newest first.

The snapshot is a full copy, not a delta: `title`, `content`, `thread_id`, and — because the
row may be ciphertext — the nonces paired with title and content and the title's blind-index
hashes, so the snapshot stays decryptable after later edits. `deleted_at` is set only on
snapshots used as sync transport, so deletions propagate between devices (ch. 05); local
history leaves it empty.

*Replaces v1.x:* global commits over thread-wide snapshots, `author`, `diff` as delta encoding,
and `branches` — the Phase 5 plan replaced global commits with per-document chains.

### Commit integrity

**Status:** ✅ Built — under the encrypted layer both commit-creating paths (`commit_document`,
`insert_commit`) stamp a MAC (code: `crates/sovereign-db/src/encrypted.rs`).

Requirement: under the encrypted layer every new commit is stamped with a device-keyed MAC over
its canonical fields (document id, parent, message, snapshot including nonces and title hashes),
so local history is tamper-evident. Listing and reading history check the MAC and warn on
mismatch; **restore refuses** a commit whose MAC does not verify. Commits created before the MAC
existed carry none and remain readable; how long they stay accepted is tracked in the private
backlog. Keys and the MAC primitive: ch. 07.

### Restore

**Status:** ✅ Built — every host restores through the document service (`restore_version`): the
restore lands whole or not at all (C-DOC G4), and a version of another document is refused (G9)
(code: `crates/sovereign-docs/src/service.rs`).

Restoring a version writes the snapshot's title and content (with their nonces and title
hashes) back to the document and then **creates a new commit** "Restored from \<commit id\>" —
history is never rewritten, and the restore itself can be undone. Restore does not change lane
membership or deletion state. Under the encrypted layer, restore also refuses a snapshot that
predates nonce-carrying commits when the live row is encrypted, since it could not be decrypted.
Restore is reachable from the History window (native shell), the orchestrator's `restore`
intent, the Tauri app's history, and an alert's "Open the previous version", which opens History
with that version selected (native shell; ch. 05). In the native shell the History rows come
from the document service, with the store's own version ids; a row whose integrity check fails
says so ("can't be verified"), a history that cannot be read opens no window and says so, and a
malformed version id is refused as such, not as "no such version". With no document service (the
workspace locked), History does not open and the notice says "The workspace is locked: log in to
see a document’s versions." Restoring a version whose title and text equal the current state
changes nothing and adds no version. *(v0.0.11 failed on every row:
the rows carried SurrealDB's display form of the id, which quotes the reserved table name
`commit`; fixed in v0.0.12 lot 1, tested through the shell's own path.)* The frozen Tauri desktop build also has a
history panel.

### Auto-commit policy

**Status:** ✅ Built — both hosts make versions through the document service's policy (C-DOC
G6); the threshold is still to calibrate.

**Decided 2026-10-11 (Céline): versions are made by amount of change, never by time.** A new
version is made when the amount of change since the last version reaches a threshold; also at an
explicit save (Ctrl+S), at the end of an editing session, and after every write that is not the
user's editor (assistant, skill, sync, import, restore). Neither the time elapsed nor the number
of saves (a proxy for time under autosave) makes a version. The measure and the threshold are to
be evaluated; the proposal is in *Document service — contract (C-DOC)*, G6.

*Replaced:* "commit frequency adapts to activity. Under high activity, commit every 50 edits or
5 minutes; under low activity, on context switch (document close) or at session end."

| Surface | As built |
|---|---|
| Mobile (Tauri host) | Each save of the document panel goes to the service, which makes a version when the policy says so, and always for an explicit save; closing the panel ends its editing session, so what it saved last becomes a version (code: `crates/sovereign-app/src/doc_saves.rs`) |
| Native shell | The editor's saves go to the service under the same policy; closing an editor, or quitting, ends its editing session (code: `crates/sovereign-shell/src/app/editor_v2.rs`) |

Creating a document makes exactly one version, whatever the origin, import included (G5); every
write that is not the user's editor (the assistant, a skill, sync, import, restore) makes a
version of its result (G6), a skill's named after the skill (G7). The CLI `commit` command is
refused: each `update-doc` is a version (code: `crates/sovereign-app/src/{cli.rs,commands.rs}`;
the CLI lives in the frozen `sovereign-tauri` binary). The 30-second auto-commit timer of the
Tauri host is retired.

### Branching and merge

**Status:** ○ Specified, not built — branching; ✗ Retired — merge through `pending_edits`,
replaced by last-writer-wins by stamp with the overwritten state kept as a version (ch. 05).

v1.x: the user checks out any historical snapshot, creates a branch from it, branches coexist
and show as a split on the timeline, merge conflicts go through `pending_edits` and are resolved
by the creator or an admin, with automatic merge when changes touch different sections. Always
commit before creating a branch.

Built: one linear chain per document, no checkout, no branch. Concurrent edits from paired
devices are resolved by last-writer-wins by stamp: the overwritten local state is kept as a
version, and an alert is raised only when the incoming-change analysis finds something (ch. 05;
C-DOC G19, G26, G31). The `branchesfrom` relation type exists, but nothing
creates it. The timeline's branch visualisation, merge UI and historical scrubber (v1.3 Open
Question 5) are therefore not built either (ch. 12).

---

## Deletion

**Status:** ✅ Built, released in v0.0.11 (C-DOC adoption step 5, `cad9771`, 2026-10-17): every delete goes to the trash through the document service; shell and phone purge at unlock and hourly; the trash is in Settings on both. Up to v0.0.10: both UIs soft-deleted, purge only on the phone, no trash.

### Soft delete and 30-day purge

**Status:** ✅ Built, released in v0.0.11 (adoption step 5): the gestures below (one click with
Undo, the trash in Settings, Empty trash, restore into a chosen thread) on the desktop shell and
the phone; the purge in both hosts. The warning for a document arriving by sync for a thread
deleted here is built too (S5-12, 2026-10-18).

Deleting a document moves it to the trash through the document service (`trash`, C-DOC G13): it
sets `deleted_at` and keeps the row, so sync carries the tombstone (ch. 05); every list, search
and traversal excludes it, and `restore_from_trash` brings it back. Deleting a thread sends its
documents to the trash and deletes the thread (*Thread deletion*). Both hosts purge through the
service at unlock and then hourly (`purge_tick`): a document 30 days in the trash goes, with its
versions, key and edges, and leaves a purge marker (G13, G27); a deleted thread shrinks to its id
when *Thread deletion* says (code: `crates/sovereign-docs/src/{service.rs,hosts.rs}`, `crates/sovereign-shell/src/app.rs`,
`crates/sovereign-app/src/{lib.rs,doc_trash.rs}`).

**Decided 2026-10-10 (Céline): delete is a soft delete everywhere, with a trash.** Deleting
moves the item to a trash the user can see; it stays recoverable for 30 days and is then purged.
The user can empty the trash at any time, which purges at once. There is no separate permanent
delete. As built (adoption step 5), both hosts follow it; a purge is local, and what travels to
paired devices is the deletion (G13, G27, as decided 2026-10-23).

**Decided 2026-10-17 (Céline) — the gestures.** Deleting a document is **one click**; the notice
says it is in the trash and can be restored for 30 days, and offers **Undo**. The only permanent
delete is **emptying the trash**, which asks first; there is no per-item "delete forever". The
trash is reached from **Settings**. A document comes back from the trash into its thread; if its
thread was deleted, the user chooses the thread it goes to (to "bring back" a thread, create a
new one and assign the documents to it from the trash).

**Actions act on what was shown — built in v0.0.12** (item 9; the cloud review of v0.0.11, R15
and R20; merged `f879443`, 2026-10-25). A peer's
deletion or a synced document can change the trash or an orphan group between the moment a list
is drawn and the moment the person confirms; until v0.0.11 the confirm re-listed and so acted on
documents nobody had seen. The choices marked PANDA2's were made by the PANDA2 maille on
2026-10-24.
- **Empty trash empties what was shown**, on the desktop and on the phone. The confirm carries the
  ids the trash view listed, and the service (`empty_trash_of`) purges exactly those still in the
  trash. A document that entered the trash after the view was drawn stays, and the next view lists
  it; a shown document no longer in the trash (restored or purged meanwhile, here or elsewhere) is
  left alone. The report counts what was purged, and the message names the skipped ones in
  neutral words: "1 document was no longer in the trash and was left alone." (PANDA2's choice).
  The 30-day purge still purges by date, not by a view.
- **An orphan group's action acts on the documents shown** (a document that arrived for a thread
  deleted here, *Thread deletion*): "Move all to a thread…", "All in a new thread…" and "Trash all"
  apply to the ids the group listed, those still waiting (`resolve_orphans_shown`). A document
  that joined the group after is left waiting and listed at the next refresh. The result says how
  many were acted on; when none of the shown is still waiting, nothing is changed, no thread is
  created, and it says so (PANDA2's choice: zero, not a refusal). Desktop and phone.
- **Delete says what it did.** `trash` tells "moved to the trash" from "was already in the trash".
  In the second case the desktop and the phone say "Already in the trash", with no Undo; an Undo
  restores only a deletion this Delete made, never one another device made.
- **"Restore into…" offers every thread** (desktop): every live thread, in the lanes' order, in a
  list that scrolls by wheel and by keyboard (Up, Down, Page Up, Page Down, Home, End); Enter
  restores into the selected thread, Escape cancels. For a screen reader it is a list, each thread
  an option named by its name, with the selection exposed. The same for "Restore all into a
  thread…". Until v0.0.11 it showed at most 8 threads, with no scrolling.

- The orchestrator has no document-delete intent: `delete_document` is graded Destruct in the
  action gate, but no handler acts on it [by code reading].

Requirement: purge removes everything that carries a document's content. ◐ Being completed
(private backlog). Soft-deleted contacts, entities and PII records follow their own chapters
(ch. 09, ch. 13).

*Replaces v1.x:* v1.3 did not describe deletion.

---

## What cannot be read is never shown as empty

**Status:** ○ Specified 2026-11-13, to build in v0.0.13. One rule for one class of defect, found in
four places at once: the paired list (C-AREA, AR3), the phone's History (« No saved versions yet »
on a read error), the unreadable documents (« could not check » shown as « none »), and listings that
skip the rows they cannot decrypt.
In v0.0.13 two more join them, beside the unreadable documents in the same reading: the list of
alerts and the list of orphans (PANDA2, 2026-11-17, by the rule's own generality).

**The rule,** in the words ch. 06 already uses: what cannot be read is **kept, counted and said**,
never dropped, replaced or shown as nothing. A store or a list that fails to read is left as it is.
The number of what could not be read is reported, and the user sees a sentence that says so.
- **On the desktop:** the Alerts window, as for undecryptable rows (CHANGELOG v2.0.53).
- **On the phone:** the screen where the failure happens says it in words, for instance History:
  « This document's history could not be read », not « No saved versions yet ». The count also
  goes to the phone's sync alerts.
- **A fifth place waits for the sync-convergence lot, out of v0.0.13:** in sync, one row that
  cannot be decrypted blocks a peer's later deletions.

**History, in v0.0.13.** **Built in v0.0.13** (item C5, merged 2026-11-18), on the desktop and the phone.
Specified 2026-11-18 (decided by the Fable
maille, by delegation of Céline (2026-11-14), on 2026-11-18, with its sentences). How the rule above
holds for a document's History. The names its rows give are in ch. 12, *History on both hosts*.
- **HR1 — A whole list that cannot be read says so.** The document service answers with the list
  and, when the read failed, its sentence: « This document's history could not be read. » The
  sentence is worded in the document service (`sovereign-docs`); the host shows it and holds no copy.
  « No saved versions yet » is shown only for a list that was read and is empty. On the desktop,
  History opens no window and its notice says it (ch. 12).
- **HR2 — A locked session is a refusal, not a read error.** A list asked for while the session is
  locked is refused, as any call is then; it is not said to be unreadable and it is not counted.
- **HR3 — The count, for the session.** The document service counts, for the session, the documents
  whose history it failed to read. The phone's sync alerts report that count, with its sentence,
  worded in the document service: « The history of 1 document could not be read. » or « The
  histories of {n} documents could not be read. » (two forms, never « document(s) »). A document
  leaves the count at its next good read. Nothing is stored: a new session starts at zero, and says
  it again when that History is opened.
- **HR4 — One version that cannot be read stays in its place.** It is listed at its place and time
  among the others. Its title and preview are the document service's sentence, « This version could
  not be read. », never its ciphertext and never a blank where it is shown (the fields that carry it
  may hold the sentence apart). « Use this version » (phone) and « Restore »
  (desktop) are not offered for it. Its document counts as in HR3, once. The mark is made where the
  version is read, so every caller of the document service gets it. Until v0.0.13 the phone showed
  the ciphertext as its title and preview, and the desktop offered Restore and refused it after the
  click. (The desktop half: PANDA2, 2026-11-18, by the same rule.)

**Its boundary.** Another persona's rows must look absent, because which persona is which is not
readable (C-AREA, ATREST-011). C-AREA does this by keeping them out of a session's store. The rule
above is for a session's own data. One shared assertion serves C-AREA's AR3, the History fix and the
unreadable-documents fix: a session's own unreadable item is counted, and the count is shown.

## Received content is hostile — contract (C-MEDIA)

**Status:** ✅ Built for v0.0.13 (lot L1b, merged 2026-11-15), specified 2026-11-13. Its tests are in
`crates/sovereign-docs/tests/` (`me1_*` to `me4_*`, `media_store_*`, `g18_*`) and
`crates/sovereign-ai/tests/` (`me1_real_files.rs`, `media_record_encrypted_at_rest.rs`). Not yet:
inserting a medium from any host (the guarded copy at insertion is built and tested in the document
service, but no shipped host offers the gesture, so a document has media only from before the
update or from another build), and showing a document's media, with the inert SVG (below).

**Until v0.0.13:** a document's media blocks carried a local file path. The service computed their
hash by reading that path, on seven call paths, among them the application of a state received
from a paired device. So received content chose which local file was read, whole.

**The media store** (Céline, 2026-11-13: « oui copie chiffrée des médias dans sovereign »).
- **Sovereign keeps its own encrypted copy** of every image and video a document holds, in the
  active persona's area. The original stays where it was, untouched, and is no longer read after
  insertion.
- **Inserting a medium copies it into the store.** After the copy, the block names the store's
  medium, its name in the store and its hash, and no longer the outside path. Insertion is the only
  gesture that copies an outside path.
- **The documents that exist at the update are migrated once, on each device, before any sync is
  applied** (raised by the Fable maille). For every media block that still names an outside path:
  - ME2's lexical refusals come first, except the refusal of an absolute path: an outside path is
    absolute by nature. A network path, a device name, `..`, the `\\?\` prefix and an alternate
    data stream are neutralised without any file-system call;
  - **the same guarded copy for every document, whichever device wrote it last** (Céline,
    2026-11-14: « Plus simple : copier partout »);
  - **a block with no recorded hash is neutralised, and nothing is read for it;**
  - **otherwise, its first bytes are read first.** A file that is not an image or a video is
    neutralised there, counted and said, and never read further;
  - **then** the file is read in a stream, under ME3's cap, and its hash is compared with the
    block's recorded hash:
    - **if they match,** the file is copied into the store, and the block names the store's medium;
    - **if not, or if the file is absent,** the block is neutralised, and counted and said.
  - **Why not by the last writer.** An earlier rule neutralised every block of a document whose
    current state was written on another device. The Fable maille found its cost:
    - an image inserted on the desktop, followed by a word corrected on the laptop, would have gone
      « not on this device » on the desktop that holds it, since both devices then share one current
      state written on the laptop;
    - a version records its date, not its device, so finding which edit inserted each image would
      have meant reading the history's chain.
    - Céline chose the guarded copy everywhere. An honest path named by another device is absent
      here, so it is neutralised as before.
  - **A video's thumbnail path** is an outside path too. It is copied if the thumbnail is a medium,
    under the same guards, and recorded otherwise.
  - **Every medium the pass copies is marked « copied by the migration »** in the store. A medium
    inserted after the update has no mark. The mark cannot be rebuilt afterwards; the second guard
    below rests on it.
  - **After the pass, no outside path is left.**
  - **An older version in a document's history** that names an outside path goes through the same
    guarded copy when it is restored: first bytes, then hash, then copy and mark, or neutralise and
    record.
  - **When the pass fails in the background,** the user is told, with no path and no title
    (Céline, 2026-11-14: « Le garder »): « Some images and videos could not be moved into
    Sovereign's encrypted store yet. This is tried again before anything is received from your
    other devices, and at the next unlock. »
  - **The cost, said in the notes:** the disk space of the copies at once, and time at the first
    launch.
  - **What the user is told after the pass** shows no path and no title (Céline, 2026-11-14:
    « Ajouter le registre »). The text, in English like the rest of the interface:
    - « N images and videos were copied into Sovereign's encrypted store; the originals stay where
      they are. »
    - « M could not be copied. »
    - « The list of what was not copied is kept, encrypted, on this device. »
    - The notice says only what is built (the Fable maille, by delegation of Céline, 2026-11-14). No
      host shows « not on this device » yet, and no repair pass is built. No screen shows the list
      either: the user learns how many were not copied, not which.
  - **In doubt, copy, with two guards** (Céline, 2026-11-13: « Dans le doute, copier », then, asked
    again with the question corrected, « Copier, avec les garde-fous »). A document received before
    the update and then saved here may keep a path that a peer named, with a hash filled on this
    device from that very file. The code cannot tell it from the user's own image. Removing in doubt
    would have removed every image inserted before the update.
    - **A medium must be a medium:** the pass, and insertion too, copy a file only if its first bytes
      say image or video, never by its name. Anything else is neutralised, counted and said.
      - **The formats are a list in the contract, broad** (Céline, 2026-11-14: « Liste large +
        registre »). Images: PNG, JPEG, GIF, WebP, BMP, TIFF, HEIC and HEIF, AVIF, and SVG. Videos:
        MP4 and MOV, WebM, AVI. Each is known by the signature its format defines, and each has its
        test. Until v0.0.13, no host filtered formats at insertion, so a document may name any
        format.
      - **A format with a weak signature gets a check as strong as the guard's purpose.** The guard
        keeps a key file, a password database or a document from being copied.
        - **An SVG is a file whose root element is `svg`.** It is read within a stated bound larger
          than the binary sniff. A declaration, a DOCTYPE, comments and white space may come before
          it; nothing else may. An XML file with another root is not a medium, whatever its name.
        - **A BMP is accepted on a coherent header,** not on its two letters « BM ».
      - **SVG is accepted** (Céline, 2026-11-14: « Les accepter »). It is text, so its first bytes
        prove less than a binary signature does. **An SVG is shown without running its scripts and
        without loading anything it references:** no network, no other file.
        - **As built, no host shows a document's media blocks.** The editor draws a placeholder with
          the alternative text, and the app's interface renders no media block.
        - **The app's Markdown preview of a document's text does show images named in the text,**
          bounded by the app's content security policy: no local file is read. An SVG there is shown
          as an image, so it is inert by the browser, not by Sovereign's code.
        - The requirement, with its test, binds the lot that first shows a document's media, on each
          host. It does not bind the lot that copies them.
      - **What the pass neutralises, it records.** For each block: the document, the block, the
        former path as text, and the reason (absent, another hash, no recorded hash, not a medium, a
        lexical refusal, too large, unreadable).
        - **« No recorded hash » is a reason of its own:** there was no hash, so it is not « another
          hash ». The record says what was the case, so that a later pass can mend the right thing.
        - **The record is never synced or backed up,** whatever store holds it: not by the document
          service, and not by the database's own sync or backup. Each has its test.
        - **A file that is there is never recorded as absent.** A file above the cap is « too large »,
          and one that cannot be read at that moment is « unreadable ». A cap found too low can then be
          mended like a list found too short.
        - The record lives in the persona's area, encrypted, outside every synced store: **it never
          leaves the device.** Its paths name her folders. Nothing ever opens a path from it.
        - **Its encryption has its test.** A former path holding a marker string is neutralised, and
          afterwards no file under the profile holds that string in clear.
        - It is what stands behind the report's « neutralised » count. A list found too short
          after the update can then be mended by a later pass over the record, without loss.
        - It is ch. 03's rule applied to the pass: what cannot be read is kept, counted and said.
    - **A medium copied in doubt never leaves the device** (by content-addressed media, backup
      between contacts, or a node) until she has seen it and kept it. Since the code cannot tell
      the two cases apart, every medium the pass copies is in doubt: it is the mark above. What
      « seen and kept » means is defined by the later lot that moves media; this binds it.
  - **What this does not protect against:**
    - such a path is read once, by this pass, under ME2's refusals and ME3's cap (its first bytes
      only, if it is not an image or a video), and if it is an image, that image shows in a document
      a peer wrote;
    - the block's hash is part of the document and syncs. A hash filled here on a peer-named path has
      always gone back to that peer, which learns whether a file it guessed is there. That is the
      leak of the v0.0.12 review's finding 1, and C-MEDIA ends it for every state received after the
      update.
- **From the update on, content that was received, imported as a Sovereign document, or written by
  the assistant never has its paths copied or read,** except once, by the migration pass (above).
  Conversion and duplication copy nothing.
  - **A received state's outside paths are neutralised when it is applied,** before anything else
    can see them. This covers sync, a restored backup and a Sovereign import, owned documents
    included. It also covers a received state kept as a version without being made current
    (C-WIRE 7): a later restore of that version has no outside path to read, and none is kept
    before the pass has run.
    - The block keeps its medium's name and hash, and is to be shown as « medium not on this
      device »: absent, not empty (*What cannot be read is never shown as empty*). As built, no host
      shows a document's media blocks yet; the lot that first shows them says it on the block.
    - A later save of that document, even by its owner, therefore has no outside path to copy.
    - ME1 then holds by construction. (Raised by the laptop maille: an owned document received by
      sync, then saved, would otherwise have its peer-named path read.)
    - **A neutralised block resolves by its hash in the local store.** When the device that holds
      the medium receives the document back, the image is shown there again.
  - **Four tests:**
    - **the round trip:** A holds a document with a medium. It syncs to B, B edits it and saves,
      and it syncs back. On A the medium is shown, and no path named by a received state was read;
    - **the legacy path, last written elsewhere:** a document applied before the update names a
      sentinel path, and its current state was written on another device. The sentinel is read at
      most once, by the pass, under the guarded copy, and is never read again;
    - **the residual, last written here:** a document received before the update, then saved on
      this device, names a sentinel path. The sentinel is read at most once, by the pass; it is
      copied only if it is a medium and its hash matches, and is then marked « copied by the
      migration ». It is never read again;
    - **the life of a document:** an image is inserted on device A, then a word is corrected on
      device B, and both sync before the update. After the pass, A shows the image, copied and
      marked, and B records the path as absent.
  - **C-DOC's G18 is replaced** on its first clause (*Guarantees*, below). Its tests are updated by
    the test author.
- **Whether the store is a folder of encrypted files or records in the database** is a technical
  choice. It is stated in the lot's design and covered by C-AREA's walk either way.
- **The cost, said to the user:** the disk space of a second copy. Deleting an original no longer
  removes the image from the document.

**Guarantees (C-MEDIA, each one a test):**
- **ME1 — A received path is never opened.** A state that arrives whole from outside the device
  never leads to a read of a path it names. That covers sync, a restored backup, and a document
  imported as a Sovereign document. A media field in such a state is data, not a location.
  - Bulk import of the user's own files, which she chose, is not such a state.
- **ME2 — Media are resolved only inside the media store.** A hash is computed only over what the
  media store holds, named by the store. It is never computed from a path that content supplies.
  - **Lexical refusals come first, before any file-system call:** an absolute path, a network path,
    a device name, `..`, and on Windows the `\\?\` prefix and alternate data streams.
  - **Then a resolution inside the store** opens nothing unless the resolved path is inside it. A
    link or junction that leaves the store is refused there.
- **ME3 — Streamed and bounded.** Hashing, copying and conversion read a media file in a stream,
  never whole. Above a size cap, a constant named in the code, the operation stops with a named
  error. Nothing is read beyond the cap.
  - **The cap is 4 GiB** (Céline, 2026-11-14, on the laptop maille's proposal). It keeps about ten
    minutes of 4K at 60 frames a second, and an hour or more of 1080p. These are estimates, not
    measures.
    - Many phones split a recording at that size, so each part fits.
    - A larger file is recorded as « too large », and a later pass with a higher cap can copy it.
- **ME4 — Every caller.** ME1 to ME3 hold on the seven call paths that hash media today, through
  `fill_media_hashes`: `create`, `save_locked`, `restore_version`, `apply_state` (both `apply_wire`
  for sync and `restore_backup`), `convert_one` and `duplicate`.
  - **The test covers each one,** the application of a received state first.
  - **It also covers two sources of hostile paths:** a document whose imported content names local
    files, and a document the assistant writes after reading hostile content.

**Decided as a direction, not built in v0.0.13:** content-addressed media. A block names its media
by hash and size, and the media store resolves it. With that, a path never travels at all.

## Document service — contract (C-DOC)

**Status:** ✅ Built, released in v0.0.11 — specified 2026-10-10; amended 2026-10-11 with
Céline's decisions (versions by amount of change, never by time; ingest of titles, captions and
alt texts; hook failure; the purge marker; trust across devices). Remaining choices marked
**[choice]**. Contracts C-DOC and C-ENC of the roadmap (lot 1), implemented by lot 2b. The
public API and the test suite came before the service: `crates/sovereign-docs` (tests in
`tests/`, map in `TESTS.md`) and `crates/sovereign-docformat` (the v2 model). The service was
built to make that suite pass, by a different author, without changing it, and every host goes
through it (*Adoption*, below).

*Why this chapter and not ch. 02:* the contract is about the document record — its stored
form, its versions, its deletion, its trust — which this chapter defines; ch. 02 lists the two
crates.

### What the service is

The one door through which documents are **created, read, saved, deleted, restored and
purged**. Before it, about thirty places read or wrote `content` themselves (the shell, the
mobile host's commands, the assistant's tools, skills, search, import, sync, backup, the PII
sweep, consolidation, seeding), three stored forms coexisted (the v1 envelope, raw text, and
v2), each writer decided alone whether to commit, run personal-data ingest, or keep provenance,
and the new editor, which writes v2, could not be switched on until every reader understood it.
The service puts encoding, versions, ingest, provenance and change events in one place; every
other crate goes through it.

**What it owns:** the stored form of `content` (C-ENC: one form written, every form read, the
migration); when a version is made; the trash and the purge, including everything that hangs
off a document; trust and provenance of documents; linking personal-data records to a document
on a Sovereign paste, and the resolver rule's answer for documents; media hashes; and, since
v0.0.12 (the lanes lot), moving a document to another thread (`move_to_thread`): every move goes
through it (code: `crates/sovereign-docs/src/moves.rs`).

**What it does not own:** position and pinning on the canvas (C-LANE; callers keep the store
calls until then); detecting personal data (C-PII implements the hook below);
deciding who wins a sync (ch. 05 decides; the service applies); the editor's undo (C-TEXT);
judgments (ch. 04 `Assessment`).

### What it is given, and nothing else

| Given | Why |
|---|---|
| The persona's store **through the encrypted layer** (`EncryptedGraphDB`) | Every body it writes is sealed and every commit it makes is MAC'd (ch. 07). There is no constructor over a plain store: the type is the guarantee. |
| A commit policy | When an editor save becomes a version: when the amount of change since the last version reaches its threshold (G6). Never by time. |
| A personal-data ingest hook (`PiiIngest`) | Implemented by the PII ingest service (C-PII, lot 2d). Receives every text field of a write — the title, the text blocks, captions and alt texts — writes records, sources and the preserved raw body itself, returns each field's canonical text. |
| Media file access (`MediaFiles`) | Hashes media for `ref.sha256` — the editor never reads files. |
| A clock | Times of user writes and trust decisions; trash expiry. It never decides a version. |
| An event sink | `DocumentChanged{id, kind}`. |
| This device's identity: the persona's peer id, never the display name (ch. 05, *Stamps*; v0.0.12 item 22) | Recorded in the stamps of the writes made here and on the user's trust decisions, which travel to her other devices (G12). |

### Operations

`create(NewDocument)` · `read(id)` → typed view · `list(thread)` · `token_status(doc, record)` ·
`save(id, Edit)` · `apply_skill_output(id, base, skill, output)` · `end_session(id, session)` ·
`set_trust(id, trust)` · `versions(id)` · `read_version(id, commit)` · `restore_version(id,
commit)` · `trash(id)` · `restore_from_trash(id)` · `list_trash()` · `empty_trash()` ·
`purge_from_trash(id)` **[choice]** · `purge_expired()` · `is_purged(id)` ·
`apply_remote(RemoteDocument)` · `migrate_all()` · and, specified 2026-10-15 (G20–G22):
`duplicate(Duplicate)` · `preview_adoption(AdoptionSelector)` · `apply_adoption(&preview)`.
Since v0.0.12 (item 9, *Deletion*): `trash(id)` returns whether it moved the document or found
it already in the trash; `empty_trash_of(shown)` purges the shown ids still in the trash and
reports the skipped ones; `empty_trash()` is called by no UI.

An **edit** carries the revision it was read at, an optional title, optional content — the
whole v2 document (the editor), new text for given text blocks, or plain text (accepted only
for a document with at most one text block) — its **origin** (an editor session, the
assistant, a skill, import), an explicit-save flag (Ctrl+S), and the tokens pasted from a
genuine Sovereign clipboard payload, each with the document it was copied from (ch. 12,
*Clipboard*).

**Trust** has three states — owned, external, undetermined (fenced like external), ch. 04. A
**trust decision** is a change the user made explicitly (`set_trust`): the trust chosen, the
device, the time; it travels with the document.
**Provenance** is one of: written here, the assistant **[choice]**, web (with URL), import
(with source path), a paired device (which created it), or *unrecorded* for rows that predate
the service **[choice]**.

### Guarantees — each one is tested

1. **One form is written.** Every write that stores content — create, save, skill output,
   restore, sync, migration — stores format v2; a write to a legacy document converts it (after
   G3's version). Content that is not valid v2 is refused, and a refused write stores nothing.
2. **Every stored form is read.** The v1 envelope (with or without media), a raw-text body, the
   empty string, and anything that is neither v2 nor a v1 envelope read as v2 blocks; a
   non-empty stored string never reads as an empty document. A v2 document of a newer `v`, or
   one claiming `sovereign.doc` that cannot be read, is shown read-only and never rewritten by
   any operation (it can still go to the trash and back).
3. **Migration commits the prior state first.** A legacy document is converted at its next
   write and by the one-time migration, each time after a version of its legacy state. The
   conversion is **deterministic** — the same legacy content under the same document id gives
   the same bytes on every device, or two devices that migrate independently would conflict
   over every document. Migration changes nothing else (title, trust, provenance, thread,
   times — the timeline does not move), runs no personal-data ingest, migrates trashed documents
   without restoring them, leaves read-only documents untouched, and is idempotent.
4. **Never overwrite without a restore point.** Before a stored state is replaced by a write
   from anything other than the editing session that wrote it — another session, the
   assistant, a skill, import, sync, a restore, the migration — that state is a version **[choice:
   saves of one editing session may replace each other, the policy decides]**. A write lands
   whole — title and content together — or not at all: after a failure at any point the document
   reads as its prior state or its new state, and the prior state is recoverable. A create that
   fails leaves no listed document.
5. **Every document has a version from its creation**: create makes exactly one version, of the
   state it stored, whatever the origin and the policy.
6. **Versions follow the amount of change, never time** (decided 2026-10-11). An editor save
   becomes a version when the amount of change since the last version reaches the policy's
   threshold, or when the user asked for one (Ctrl+S). Time passing never makes a version, nor
   does the number of saves. The end of an editing session makes a version of what it left;
   every non-editor write (assistant, skill, sync, import, restore) makes a version of its
   result; no version repeats the one before it. The policy is asked about editor saves only,
   is told the amount of change and nothing else, and can only add versions.
   - **The measure — [choice], to be evaluated.** A distance between two states (the last
     version's and the one saved), not a count of keystrokes: it needs no counter a crash could
     lose, gives the same answer on every device, and typing then erasing the same words is not a
     change worth a version. For the title and each text block (matched by block id): the
     characters (Unicode scalar values) outside the longest common prefix and suffix, on both
     sides — what was deleted plus what was inserted for one edited region, an upper bound when
     several regions changed; linear time. A text block on one side only counts its whole
     length. A non-text block added, removed or changed counts a fixed weight (to calibrate; a
     picture has no characters, and replacing it deserves a version); a change in the order of
     the blocks counts it once more. The measure is zero exactly for equal states, symmetric,
     and never shrinks as a change grows.
   - **The threshold** is a parameter of the policy; its default is **to calibrate** (a
     starting point of about a short paragraph of typing is set in code, not decided).
7. **A skill's output** is applied after a version of the prior state and becomes a version
   named after the skill. A text-only write never removes, reorders or alters a non-text block;
   plain text is refused for a document with more than one text block, and text for an id that
   is not a text block is refused.
8. **Only MAC'd commits.** Every commit the service makes, on every path, carries a MAC that
   verifies (ch. 07). A version whose MAC does not verify is listed as invalid and never
   restored; a legacy unsigned one is listed as unsigned.
9. **Restore** writes a version's title and text back, after a version of the current state,
   and the result is a new version — history is never rewritten. A legacy snapshot is written
   as v2, a newer-format one verbatim. Restore changes neither trust, provenance, thread nor
   trash state; it refuses another document's commit, a trashed or a purged document.
10. **Personal-data ingest runs on every write of text** — create, save, skill output, import,
    sync, restore — every time a text field changes, not only the first time. The text fields
    are the **title, every text block, and every caption and alt text** (decided 2026-10-11);
    the hook receives all of them on each such write, not only those that changed, so no field
    escapes; it receives nothing else (unknown blocks are not sent). Tokens may therefore appear
    in titles, captions and alt texts. The stored text is exactly what the hook returns; an
    answer naming a field it was not given is refused. **If the hook fails, the content is stored
    as given and the document is marked not scanned**, so the sweep and read-time masking cover
    it — nothing typed is lost (decided 2026-10-11). Writes that change no text field (identical
    save, trash, restore from trash, migration) do not run it.
11. **Writers never silently replace each other.** A save carries the revision it was based on;
    a save based on a revision that is no longer current is refused and writes nothing; of two
    concurrent saves from the same revision exactly one lands **[choice]**. A save identical to
    the stored state writes nothing, makes no version, changes no time and emits nothing.
12. **Trust and provenance are set at creation and never changed silently.** Provenance never
    changes. Trust changes only by **a user decision**: `set_trust`, the user's explicit gesture
    here, in either direction, which records the decision (this device, the time); or such a
    decision made on one of her paired devices and received by sync, which is applied **in
    either direction** — trust across devices follows who decided, not the direction (decided by
    Céline, 2026-10-11: adopted on the phone to edit it, it arrives owned on the desktop; lowered
    on the desktop, the phone follows). Of two decisions, the later by its recorded time wins
    **[choice]**. No save, skill, migration, restore or trash changes trust, and nothing else in
    sync does: a remote state that differs in trust without a decision leaves local trust as it
    is, in both directions (decided 2026-10-11). The `is_owned` flag the rest of the system reads
    agrees with trust (owned only when owned). Web content cannot be created owned without a user
    decision, here or over sync **[choice]**. A document created by sync takes the sender's trust
    and provenance; an older peer that sends no trust gives undetermined, with provenance *paired
    device*. Rows from before the service read as owned or external from `is_owned`, provenance
    *unrecorded*. *Open in C-WIRE:* the decision as a wire field, added with a default.
13. **Delete is trash.** Trash keeps everything (content, versions, links, personal-data
    sources), keeps the row so sync carries the tombstone (ch. 05), and hides the document from
    lists and pending suggestions; restoring brings it back exactly. Emptying the trash, purging
    one item from it, or 30 days in it purges: the document, its versions, its key
    (crypto-shredding), its edges and suggested links in both directions, its personal-data
    source references — and a discovered record left with no source; a vault entry is never
    removed by a purge. What remains is a **purge marker carrying no content** (decided
    2026-10-11), so a purged document is never listed, read, restored, or brought back by sync —
    **with one exception (decided by Céline, 2026-10-23; ch. 05, C-WIRE *Deletions and purges*): a
    live state from a paired device with a later stamp than the deletion comes back as a new
    arrival.** **A purge is local** (same decision, replacing that of 2026-10-15): it removes the
    document from this device only, the 30 days run on this device's clock from the moment the
    document entered this trash, and what travels is the deletion, never the purge. A purge
    interrupted at any point completes when run again. Nothing else is touched. **The 30 days follow
    the device's clock, with no guard** (Céline, 2026-11-18, choosing « Keep the clock, say it »): a
    clock set wrong can empty the trash early. A clock moved forward by more than 30 days purges the
    whole trash at the next hourly check, for good; an entry dated while the clock was behind, then
    corrected, is purged early. The release notes say so.
14. **Token links come from a Sovereign paste only** (ch. 09, *Where a token resolves*). A save
    adds the document to a record's sources only when it comes from the editor, names the record
    among its pasted tokens, the record is linked to the document the token was copied from, and
    the token is in the saved title or text fields — the link happens **at the save**, not at
    the paste (decided 2026-10-11; the editor's save request carrying its pasted tokens is a
    later C-TEXT change). Token text typed by hand, or written by the assistant, a
    skill, import or sync, never links; a pasted token naming no record creates nothing.
15. **The resolver rule.** Text in token form is a token only in a document listed among its
    record's sources; there, a deleted or unreadable record shows as a placeholder. Anywhere
    else — another document, an unknown record — it is ordinary text. The same holds in the title,
    captions and alt texts; the view gives a labelled title as well as a labelled text.
16. **One event per change.** `DocumentChanged{id, kind}` — kinds created, saved, migrated,
    trashed, restored from trash, purged, version restored, trust changed, remote applied — is
    emitted exactly once per completed change, after the change is stored, and never for a read,
    a refused, failed or identical write.
17. **Readers get a typed view** — the blocks; the plain text (text blocks joined by a blank
    line, tokens in canonical form) for search, the assistant and skills; the labelled text for
    display — so no caller parses stored content. Lists and the trash carry no content. The debug
    form of every type that holds document text or titles prints sizes, never the text.
18. **Media hashes are the service's.** At any write, a media block without `ref.sha256` whose
    file can be read gets the file's hash; a present hash is never changed (it is what detects
    an altered file); an unreadable file leaves the block valid without one. *As built; C-MEDIA
    (above) replaces the first clause in v0.0.13:* a hash is computed only inside the media store,
    never from a path that content names, and conversion hashes nothing outside the store.
19. **Sync applies without losing anything.** The service applies the state sync decided on.
    Before a peer's state replaces the local one, the local state is a version; **no review flag
    is raised** (changed 2026-10-15, C-WIRE 13 — an alert only when the incoming-change analysis
    detects something, G31); a peer's tombstone moves the document to the trash, restorably; a
    live peer state brings a trashed document back; a purged id is ignored unless a live state
    with a later stamp arrives (G27, as amended 2026-10-23); a state equal to the local one changes nothing. Legacy content from an
    older peer is stored as v2, newer-format content verbatim (read-only). Sync applies through
    `apply_wire` (G24–G31) and a backup through `restore_backup` (G30); `apply_remote` keeps these
    rules, and no host calls it.

### Duplicate, adoption gestures, content from outside (G20–G23)

**Status:** ◐ Partial — specified 2026-10-15, choices marked **[choice]** to be reviewed by
Céline. From the decisions of 2026-10-15 (ch. 04, *Decided (Céline, 2026-10-15) — defaults at
creation and adoption gestures*). Tests in `crates/sovereign-docs/tests/g20_*`–`g23_*`, written
before the operations. The operations are built in the service (code:
`crates/sovereign-docs/src/service.rs`); a skill's copy of a document it read goes through
`duplicate` (code: `crates/sovereign-ai/src/docs_host.rs`); no surface offers the adoption
gestures yet.

New operations: `duplicate(Duplicate { source, title?, thread?, origin })` ·
`preview_adoption(AdoptionSelector)` → `AdoptionPreview` · `apply_adoption(&AdoptionPreview)` →
`AdoptionReport`. New provenance: *copy of* a document, carrying the source's provenance
**[choice]**.

20. **Duplicate** makes a copy of a document: its content as stored, the title and thread asked
    for (the source's by default), its own first version, ingest as for any create (G10). Its
    personal-data tokens stay what they were: the copy is linked to every record that is linked
    to the source and whose token is in the source's text fields (title, text, captions, alt
    texts) — the G14 conditions, a copy taking the place of a paste — and to no other; a
    placeholder stays a placeholder, ordinary text stays ordinary text. Copying never raises or
    lowers trust: the copy has the source's trust, so a copy of her own document (written here,
    or adopted) is owned and a copy of external or undetermined content keeps that state. Its
    provenance records the copy — *copy of <source id>*, with the source's provenance as it was
    **[choice]** — so a copied web page still says where it came from. The source is unchanged
    (content, versions, time, trust; only its records gain the copy as a source). A trashed,
    purged or read-only (newer format, unreadable) source is refused and nothing is created. Only
    `duplicate` makes copies: `create` refuses a *copy of* provenance.
21. **An adoption gesture is previewed before anything changes.** The selector names documents
    — a list (one document or a bulk selection), a thread, an import folder, an original
    creation date or a last modification date before a chosen instant (strictly before) — and
    the preview lists exactly those that are candidates now: not in the trash, not owned. **Thread,
    import-folder and date gestures take undetermined documents only**; an explicit selection of
    documents (one, or a bulk pick of ids) takes undetermined and external ones — an external
    document (web, shared web) is adopted only when the user picks it herself (decided by Céline,
    2026-10-15: adopting a thread must never silently move fenced web pages into the trusted
    plane). Each candidate is listed with its current trust. An **import
    folder** names the documents whose provenance is an import whose path lies inside that
    folder, compared by path components after turning `\` into `/` and dropping a trailing
    separator (`/in/corpus` does not name `/in/corpusX`); provenances that are not paths
    (`command line`, `skill:<name>`) and copies of imported documents name no folder
    **[choice]**. A date gesture's preview carries the warning *a date is not proof of
    authorship*. Previewing changes nothing.
22. **Applying a preview adopts exactly the documents it listed** that are still candidates,
    and reports the others as skipped (already owned, in the trash, purged, gone) — the
    selection is the preview's, never recomputed: a document arriving or becoming a candidate
    after the preview is not adopted. Each adopted document gets **its own trust decision**
    (owned, this device, the time of the apply), which travels with sync (G12), and one
    `TrustChanged` event; nothing else about it changes — content, title, thread, times,
    provenance. Each document is adopted whole or not at all: after a failure at any point, every
    previewed document is either adopted (trust, decision and the fence flag together) or as it
    was, and applying again completes the rest. A gesture is **never a standing rule**: a
    document created later in the same thread, folder or date range lands as its creator says.
    Adoption only raises trust; lowering stays a per-document `set_trust` **[choice]**.
23. **Nothing from outside is created owned.** A document whose provenance is the web or an
    import cannot be created owned (refused); it lands external or undetermined and becomes owned
    only by a user decision (G12, G22). Which allowed state a creator asks for — owned for what
    the user writes from scratch (a new document, a journal entry, what the assistant writes at
    her request), undetermined for imports and a skill's external file, external for the web — is
    the **host's default**, passed in `NewDocument.trust`, not the service's: the service
    cannot tell "written from scratch" from the request, and enforces only the bound.

### What sync needs from the service (G24–G31)

**Status:** ✅ Built — specified 2026-10-15 for C-WIRE (ch. 05; Céline's decisions the same day),
tests first (`crates/sovereign-docs/tests/g24_*`–`g31_*`), built in C-DOC adoption step 4 and
released in v0.0.11; G27 and G28 as amended by v0.0.12 item 3 (purges are local), G25 by v0.0.12
lot 1 (the bound inside the clock). (code: `crates/sovereign-docs/src/wire.rs`) Nothing
changes an existing type: the wire metadata travels beside a `RemoteDocument` in a `WireMeta`
(creation time, stamp, previous stamp, author device, the decision's stamp).

New operations: `apply_wire(remote, meta)` · `stored_state(id)` / `stored_states()` ·
`purge_markers()` · `apply_purge_marker(marker)` · `restore_backup(documents, markers)` ·
`raise_alert` / `alerts` / `dismiss_alert`; and a pure function, `normalized_hash(state)` (a
purge marker carries no fingerprint: purges are local, G27).

24. **The wire's fields are kept, and a missing one never overwrites a known one.** `apply_wire`
    keeps the original creation time, the author device (the sender when none is named) and the
    stamps; a field the sender does not send leaves the local value; a document new here without
    trust or provenance lands undetermined, provenance *paired device*.
25. **Stamps, not clocks.** Every write and every trust decision gets a hybrid-logical-clock
    stamp greater than every stamp this device has issued or received, whatever its clock says.
    A remote state is applied by stamp: one that follows the local state (its previous stamp is the
    local stamp) applies; an older one is ignored; one more than 5 minutes ahead of this device's
    clock is refused and writes nothing **[choice: 5 minutes]**. *(v0.0.12 lot 1: the bound is
    inside the clock — a stamp more than 5 minutes ahead of this device's wall clock never moves
    it, whatever carries it — so "greater than every stamp received" holds for stamps within the
    bound; a stamp beyond it already stored keeps winning concurrent conflicts on its document
    until real time passes it. Ch. 05, *Stamps instead of the wall clock*.)*
26. **Concurrent edits keep both.** A remote state whose previous stamp is not the local stamp,
    while the local state changed since, is concurrent: the greater stamp is current, the other is
    kept as a version (a remote state with the lower stamp is stored as a version without becoming
    current), and the outcome says it was concurrent.
27. **Purge markers** *(amended 2026-10-23, Céline: purges are local; built in v0.0.12 item 3)*. A
    purge leaves a local marker — id and deletion stamp, no fingerprint, never content. It travels
    as a deletion only: a peer's live copy with an earlier stamp goes to that peer's trash and its
    state takes the deletion's stamp; a trashed copy stays there, its trash date and stamp
    unchanged; with no copy, the peer records the deletion (id and stamp, the later of two kept),
    so an older copy arriving from a third device is ignored; no peer copy is purged. A purged id
    ignores a peer's trashed copy and any live state whose stamp is earlier than or equal to the
    deletion's, and comes back, live, as a new arrival with its own first version, from a live
    state with a later stamp (the exception to G13). The 30 days run on each device's clock from
    the moment the document entered its trash; the days left the trash shows are counted from the
    same date (the local trash date, floored at an upgrade by "trash since"). A clock set wrong can
    empty the trash early: forward by more than 30 days, the whole trash; behind when an entry was
    dated, that entry (G13; Céline, 2026-11-18, « Keep the clock, say it »). A backup's markers
    are id and stamp, never a fingerprint, and are recorded as deletions at restore. A
    fingerprint a v0.0.11 marker still holds is erased at every unlock, each persona at its own
    (ch. 05, *Deletions and purges*).
28. **Normalised hash.** The hash covers title, thread, trash state, trust, trust
    decision, provenance and the content as v2 (legacy forms converted, seeded by the document id):
    equal for two copies of one document whatever form each stores, different when any of these
    differs, independent of the sending device and the modification time. It is lowercase hex and
    never contains the text. *(The purge fingerprint this guarantee also described is gone since
    v0.0.12 item 3: purges are local, and a marker holds no fingerprint.)*
29. **The stored state, for sending.** `stored_state` gives everything that defines a document
    without the caller reading `content`; applied on a device without the document it reproduces
    it (same hash, trust, decision, provenance, creation time, copies included). Trashed documents
    are included; a purged one is `Purged`.
30. **A backup restores only onto an empty workspace** (Céline, 2026-10-15). `restore_backup`
    refuses a workspace holding any document (trash included), writes nothing and says why; on an
    empty one every document comes back with exactly one version "Restored from backup", its trust,
    decision, provenance, times and trash state, and the backup's purge markers are recorded.
31. **No review flag; alerts.** Applying a remote state raises no review flag. An alert from the
    incoming-change analysis is recorded with the document, the device, the finding and the
    previous version, listed until dismissed; dismissing changes nothing in the document; an alert
    goes with its document when it is purged.

### Adoption — who stops touching `content`

**Status:** ✅ Done — steps 1–6 below, released in v0.0.11; sync and backup adopted `apply_wire`
and `restore_backup` (G24–G31). Ownership is the exception: the assistant's "adopt" still calls
`adopt_document` (*Ownership*). What follows is the plan as measured and ordered on 2026-10-10.

Measured 2026-10-10 by grep over every crate but `sovereign-db`: **141 production call sites**
read or write document content or lifecycle directly (sovereign-app 47, sovereign-skills 34,
sovereign-ai 26, sovereign-shell 18, sovereign-p2p 14, sovereign-import 2). By operation: 25
parse `content` (four with hand-rolled envelope parsers that disagree), 23 read it raw without
parsing, 22 create documents, 10 overwrite, 10 commit, 6 restore versions, 3 soft-delete, 1
hard-deletes, 1 restores from trash, 1 purges, 11 run or feed personal-data ingest, 5 resolve
tokens. The full table (file, line, operation, form) is kept with the plan (private).

| Caller | Replaced by |
|---|---|
| Every reader of `content` (shell canvas and windows, mobile `get_document`, assistant tools and summarize, consolidation, peer audit, search snippets, history previews, skills' `SkillDocument` and `SkillDbAccess`, WASM host) | `read` → typed view; `plain_text()` for search, the assistant and skills; `labelled_text()` for display; `read_version` for history |
| Every creator (shell new document and web clip, mobile create / import file / share / web clip, assistant `create_document`, skills via `SkillDbAccess` and WASM, import `execute`, duress and demo seeding, CLI) | `create` with trust and provenance |
| Every overwrite (shell `save_body`, shell skill apply, mobile `save_document`, PII sweep, CLI `update_doc`) | `save` (editor, assistant) / `apply_skill_output` |
| Skill output (`ContentUpdate`) | `apply_skill_output` with text by block id, or plain text |
| Commits (`AutoCommitEngine`, shell per-save commits, mobile close, CLI) | the commit policy, `end_session` |
| Version restore (shell, mobile, assistant `restore`, peer-review restore) | `restore_version` |
| Delete, restore from trash, purge (shell, mobile, sync, the hourly purge, CLI hard delete) | `trash`, `restore_from_trash`, `empty_trash` / `purge_from_trash`, `purge_expired` |
| Sync apply (create under origin id, overwrite with pre-sync commit, tombstone, revive) and backup restore | `apply_remote`; as built, `apply_wire` (sync) and `restore_backup` (backup) |
| Ownership (`adopt_document`) | `set_trust` |
| Document PII ingest glue (`maybe_ingest_document_body`, sweep body rewrite) | the `PiiIngest` hook, called by the service |
| Token resolution in documents | `token_status`, `read().tokens` |

**Order.** (1) **Readers first**, behind the typed view: shell, mobile, the assistant's tools,
consolidation, peer audit, skills' inputs. Reads become format-agnostic, so v2 can appear. (2)
**The editor's save path** — the shell adopts `save` and `end_session` with the new editor;
mobile adopts `save`. (3) **Creators** — import (gives every imported document a version and
an undetermined trust), web clips, the assistant, skills, seeding. (4) **Sync and backup**
adopt `apply_remote` — with the wire fields of C-WIRE (trust and provenance in the snapshot,
the purge marker). (5) **Delete and purge** in every host, and the hourly `purge_expired`. (6)
**`migrate_all`**, once every writer goes through the service; then the tolerant read
(`ContentFields::parse`) and the four hand-rolled parsers are removed. Shell and mobile can
move in parallel within each step; skills follow C-SKILL.

---

## Memory consolidation

**Status:** ◐ Partial — the engine, lifecycle and idle scheduling are built and run in the Tauri
host, which ships as the mobile app [whether the mobile build has a model to run it with:
unverified]; the native shell does not run consolidation; the adaptive gate does not receive link
feedback (below).

A background process looks for documents that should be linked and proposes links — above all
between something the user saved from outside and something the user wrote. It proposes; it
never creates a relationship by itself (code: `crates/sovereign-ai/src/consolidation.rs`,
`orchestrator.rs`).

**One cycle:**

1. Load all active documents (fewer than two: stop), all `related_to` edges, all pending
   suggestions.
2. Build candidate pairs, skipping any pair already linked or already suggested in any status
   (so dismissed pairs never come back). Priority: **external ↔ owned** pairs first, then owned ↔
   owned across different threads; within each group, most recently modified first. Pairs in
   the same thread, and external ↔ external pairs, are not considered.
3. Take at most **5 pairs** and score them in **one call to the router model (3B)**. Each
   document is reduced to a fingerprint — title and the first 200 characters of the body, with
   its ownership — and the fingerprint is **fenced as untrusted data**, since titles and bodies
   can come from web pages, imports or synced documents (ch. 08).
4. The model returns, per pair, related or not, a type among `supports`, `references`,
   `contradicts`, `continues`, `derivedfrom`, a strength and a one-sentence reason. Strength is
   clamped to 0.0–1.0.
5. Pairs with **strength ≥ 0.4** become `pending` suggestions; each new suggestion is announced
   to the UI.

**Scheduling.** Opportunistic, never on a fixed timer: every 30 seconds an idle watcher checks
that the model is not generating, and that at least 5 minutes have passed since the last cycle;
user tasks always take precedence (code: `crates/sovereign-app/src/lib.rs`). A cycle can also be
triggered by hand through the Tauri host's debug command `trigger_consolidation`.

◐ Not built from the plan: the extra idle conditions (orchestrator bubble idle, no user
interaction for a cooldown, default 60 s) and "at least one document modified since the last
run" — today the watcher relies on model idleness and the 5-minute cooldown alone.

**Adaptive gate.** Once five or more consolidation suggestions have been shown, a cycle runs only
while the user's acceptance rate is high enough; with the built thresholds this means an
acceptance rate ≥ 0.7 (code: `crates/sovereign-core/src/profile.rs`,
`AdaptiveParams::from_acceptance_rate`). ◐ Accepting or dismissing a link does not yet feed the
counter the gate reads, so after the first five suggestions the gate holds at the neutral rate
and consolidation stops proposing [by code reading; not observed at runtime].

---

## Bulk import

**Status:** ◐ Partial — increment 1 is built as an engine and a CLI (v0.0.9); the CLI lives in
the `sovereign-tauri` binary, frozen and not shipped, so no shipped build offers import today;
the full provenance record (only the kind *import* and the source path are recorded), credential
routing, per-file overrides, image tier, stub upgrade and the shell import window are not built (code: `crates/sovereign-import/src/lib.rs`,
`crates/sovereign-app/src/{cli.rs,commands.rs}`).

### Doctrine — stub-first

**Status:** ✅ Built.

Migration is a **landing problem, not a conversion problem.** 100 % of a corpus lands on day
one — on the timeline, in lanes, searchable, linked to its source — and fidelity varies by
format and improves later without re-running the migration. Nothing in the tree is silently
dropped: every file is either imported, at some tier, or listed with the reason it was skipped.
Email is not a migration channel (decided 2026-07-06): it would bring the user's own documents
in as external content, through a cloud detour.

### Engine and manifest contract

**Status:** ✅ Built.

The importer is a host-agnostic library: **scan → plan → execute** over a `&dyn GraphDB`, with
no CLI parsing and no UI. `plan(root, options)` is pure — it reads the filesystem and returns a
`Manifest`; only `execute(db, manifest)` writes. **The `Manifest` is the contract** between
the engine and any frontend (decided 2026-07-06): the CLI is frontend #1, and the future import
window renders the same manifest as a review list. Nothing is built CLI-only.

- `Manifest`: scan root, the planned files, and the sorted list of thread names to create or
  reuse.
- `PlannedFile`: absolute and root-relative path, title (file stem), extension, size, original
  modified and created times, target thread (none if skipped), and a `Disposition`.
- `Disposition`: `Import(Text | Extract | Stub)`, or one of `SkipDerivedExport`,
  `SkipCredential`, `SkipEmpty`, `SkipNonDocument`, `SkipOversize`.

The scan walks recursively in deterministic order (so dry-run and execute agree), skips dotfiles
and dot-directories, and skips dependency trees wholesale (`node_modules`, `__pycache__`,
`venv`, `bower_components`). Names a business corpus might legitimately use (`build`, `dist`,
`target`, `vendor`) are deliberately not skipped.

### Tiers

**Status:** ◐ Partial.

| Tier | Formats (as built) | Result |
|---|---|---|
| **Text** | `md markdown mdown txt text csv tsv rst org tex adoc` | The file's text is the document body, editable and searchable |
| **Extract** | `html htm` (to markdown), `pdf`, `docx`, `pptx` (one section per slide) | Extracted at execute time; **degrades to a stub** when nothing usable comes out (e.g. a scanned PDF with no text layer) — a bad file never fails the import |
| **Stub** | everything else: images, legacy `.doc`/`.ppt`, spreadsheets, unknown formats | Title from the file name, original timestamps, and a short note in the body with the original path, format and size |

Text and extract files over **25 MB** are skipped as oversize; other files are never read, so
their size does not matter. Office files are read with bounded decompression and PDF extraction
cannot crash the import (code: `crates/sovereign-import/src/lib.rs`).

The stub's note lives inside the document's body, which the encrypted layer seals.

Not built: `xls`/`xlsx` extraction (planned via `calamine`), an image tier (images land as
stubs; OCR or captioning would be a later enrichment pass over imported documents), and **stub
upgrade** — re-processing a stub in place (same id, same timeline position, history kept) once a
parser for its format exists. Until it exists, a stub stays a stub, although its note says it
will be filled in automatically.

*Replaces v0.0.9 plan:* `json` listed in Tier 1 and "images → Tier 2" — `json` is never
imported (next section); images are stubs.

### What is never imported

**Status:** ✅ Built.

**Just documents, no code** (Céline's rule). Source code, scripts, stylesheets, configuration,
data serialization and build files — including `json`, `yaml`, `toml`, `xml`, `sql`, `css`, `sh`,
`ps1` — 78 extensions in all — are skipped entirely: not imported, not even as stubs.
The reason is trust, not tidiness: owned content is not screened for injected instructions
(ch. 08), and an imported document becomes owned once the user adopts it (below), so code or
config carrying such a payload must not enter where it could pass for something the user wrote. The dry-run counts these files
instead of listing them.

Zero-byte files are skipped as empty. Credential-like files are handled separately (below).

> Note: the v0.0.9 release notes say code and config "land as a findable, re-processable stub";
> the code skips them. The code is the design; the release-note sentence is inaccurate.

### Lanes

**Status:** ✅ Built.

- **Folder → lane** is the default: each file's immediate containing folder becomes its thread;
  files at the scan root go to a thread named after the root folder. Imperfect is acceptable —
  it is a first sorting (decided 2026-07-06).
- **`--single-thread <name>`** puts everything in one named thread.
- A lane is matched **by name**: distinct folders with the same name map to the same lane, and
  an existing live thread with exactly that name (ignoring case and surrounding spaces) is reused;
  a name two live threads share refuses the whole import, before anything is written (since
  v0.0.12; *Thread*, *Find or create by name*). In v0.0.13 the import asks which one instead, and
  remembers the answer; it refuses only where nobody can answer (*Two threads of the name sought*,
  FC5; built in v0.0.13, item 9).
- When two importable files would land in the same lane with the same title from **different
  folders**, their titles are prefixed with the folder that contains the lane folder (and a
  numeric suffix as last resort), so every distinct document lands and stays legible. Same-title
  files within **one** folder are format variants of one artifact (for example a part exported
  as `.glb`, `.step` and `.stl`); one of them lands.

### Timestamps

**Status:** ✅ Built.

The original file's modified time becomes `modified_at`, and its creation time (or the modified
time where the platform has none) becomes `created_at`, so the archive lands across the timeline
instead of piling up on "now". The import event itself is meant to live in the provenance record
(below). Both times are clamped to the interval from 1970 to the moment of import: a file time
cannot pin a document to the future or bury it before the epoch.

◐ Whether Google Takeout exports keep modification dates was to be checked before importing a
Google corpus [unverified]; Google Docs, Sheets and Slides come in through a one-time Takeout
export into the same funnel — Sovereign has no Drive API.

### Derived-export deduplication

**Status:** ◐ Partial — automatic rule built; per-file override not built.

A corpus often holds collaboration copies — `x.pdf` or `x.docx` exported from `x.md`. Importing
both duplicates every document. When a text source and a same-stem file in an export format
(`pdf docx doc odt rtf html htm`) sit in the same folder, the export is skipped and listed as a
derived export — **only if it is not older than the source**: a real export is generated from its
source, so an older same-stem file is probably a different document and is imported. The plan
called for a per-file override in the review; the CLI has none.

### Credential-like files

**Status:** ◐ Partial — surfaced and skipped; vault routing not built.

Files such as `.pem`, `.key`, `.pfx`, `.p12`, `.keystore`, `.jks` are never imported as document
content. A found key is a discovery, not an import item: the dry-run lists each one. Intended
(v0.0.9 plan, increment 2): the manifest **proposes** each one for the vault (ch. 09) — default
*vault*, alternatives *skip* or *stub* — and nothing is moved there without the user's choice;
from the CLI they would land under the user's own entity, re-attributed later in the PII
dashboard. Today they are skipped and listed, nothing more.

### Dry-run and the trust notice

**Status:** ✅ Built.

Dry-run is the default. `import --dir <path>` prints the plan — documents to import by tier, the
lanes and their counts, and every skipped file other than code with its reason — and writes
nothing. `--execute` prints the same plan, asks for the passphrase, and imports into the primary
workspace, encrypted at rest.

Every plan, dry-run and execute alike, ends with a **trust notice**: imported files are stored
as *undetermined* content, and until the user adopts a document the model reads it as data,
screened for prompt injection, never as her instructions; a file authored by someone else can
carry hidden instructions, and adopting it makes them read as her own; the notice asks her to
adopt only what she wrote or trusts. The notice is restated just before the passphrase prompt, which serves as
the affirmative consent (code: `crates/sovereign-import/src/lib.rs`,
`crates/sovereign-app/src/commands.rs`).

### Execute and idempotency

**Status:** ✅ Built (increment-1 form).

Execute resolves each lane name to an existing thread (reused) or a new one, then creates the
planned documents with their original timestamps, and reports threads created and reused,
documents created, stubs created, and documents skipped — **with their paths**, so a skip is
auditable and never a silent truncation. A re-run does not duplicate: a file whose title already
exists in its target thread is skipped. Content-hash change detection replaces this in
increment 2 (below).

### Originals

**Status:** ✅ Built.

The importer only reads. Originals stay where they are; the imported text is the working copy,
and Sovereign is the source of truth from import forward. No blob store is built: a stub points
to its original by path.

### Ownership at import

**Status:** ◐ Partial — imports land undetermined (C-DOC adoption step 3, 2026-10-15); the
adoption gestures are built in the document service (G21, G22) and offered by no surface yet.

Imported documents land **undetermined** (fenced until the user adopts them), with provenance
"import" and their source path; the import notice says so. This replaces the decision of
2026-07-18 (imports landed owned, to spare a migrating user re-owning a corpus one file at a
time): the burden it named is now met by **adoption gestures** — per document, per thread, per
import folder, in bulk, or by original creation or last modification date — each with a preview
and none becoming a standing rule (ch. 04, *Ownership is not decided at landing*, decided
2026-10-15).

### Provenance record

**Status:** ◐ Partial — the provenance kind and the source path are recorded; the rest is
increment 2, not built.

Every imported document carries a structural provenance record: `source_path`, `source_format`,
original modified and created times, `imported_at`, `imported_by` (CLI or UI, with version),
`tier`, and a **content hash of the original**. The record is what makes stub-first a pipeline
rather than a one-shot: re-runs skip unchanged files by hash, and stubs are upgraded in place by
source path. As built, every imported document records provenance *import* with its source
path (`provenance_kind`, `provenance_detail`); the stub body also notes the original path, for
stubs (code: `crates/sovereign-import/src/lib.rs`, `crates/sovereign-docs/src/service.rs`).

### Import window

**Status:** ○ Specified, not built.

A shell import window renders the manifest as a review list — proposed items, per-item accept
or skip, apply — on the pattern of the peer-review panel (ch. 05, ch. 12). Drag-and-drop onto
the canvas reuses the engine for single files.

---

## Spatial positioning

**Status:** ✗ Retired — replaced by the 2D time × lane canvas (ch. 12).

v1.x placed documents in 3D space — x/y by semantic proximity, z by time depth, 99 % generated
by the AI, stored per snapshot. The built canvas places documents by time on one axis and by
thread lane on the other; the stored `spatial_x` / `spatial_y` fields and their use are
described in ch. 12.

---

## Conflicts and open questions in this chapter

| Item | Kind | Where |
|---|---|---|
| `content` encoding: the native shell writes both raw text and the JSON envelope, import writes raw, mobile reads and writes the envelope; sync carries both forms everywhere | ✅ Decided 2026-10-10: versioned typed-block JSON, migration from both current forms | *Content encoding* |
| Document delete: soft (mobile) vs immediate, and undone by sync (native shell) | ✅ Decided 2026-10-10 (K3): soft delete everywhere, with a trash; 2026-10-17: one click + Undo, trash in Settings, emptying it is the only permanent delete, a deleted thread's documents go to the trash; built in adoption step 5 | *Soft delete and 30-day purge*, *Thread deletion* |
| Multi-party ownership model: target or stale | ✅ Decided 2026-10-10 (D3, ch. 15): trust is built separately; multi-party ownership waits for a future collaboration chapter | *○ Deferred (D3)* |
| Undetermined ownership at import | ✅ Decided 2026-10-15: imports land undetermined; adoption per document, thread, import folder, bulk or date (ch. 04) | *Ownership*, *Ownership at import* |
