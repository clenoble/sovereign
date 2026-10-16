# 03 — Data Model

How Sovereign GE stores the user's corpus: the storage engine and where it lives, the records
(documents, threads, milestones, relationships, commits), how versions, deletion and AI-suggested
links work, and how an existing folder tree is brought in by bulk import.

*Consolidates:* specification v1.3 §JSON Schema Specification (Storage Model, Document Node
Structure, Intent Thread Manifest, Relationship Model, Spatial Positioning, Version Control) and
Open Questions 5–7 and 9; archived plans `consolidated-memory.md`, `phase5-plan.md`,
`v0.0.9-plan.md` (Track 2, bulk import), `sovereign_os_implementation_plan.md` (Spike 2, §1a.2);
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
50,000-document bulk insert < 30 s) all passed (source: `sovereign_os_implementation_plan.md`,
Spike 2).

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
was "batch insert within 2× of single-insert benchmarks" (source:
`sovereign_os_implementation_plan.md` §1a.2). As built, `create_relationship` issues one `RELATE`
per edge and no batch API exists on `GraphDB`. Nothing creates relationships in bulk today
(import creates none; consolidation creates at most five suggestions per cycle), so the
requirement is not yet load-bearing.

---

## Document

**Status:** ◐ Partial — the record is built and in use; its content encoding differs between
writers, inside the native shell itself and between the shell and mobile (⚠ below), and its
ownership model is a pending decision (D3).

A document is one node of the user's graph: a title, a body, the lane it belongs to, and the
metadata other subsystems attach to it. Fields are flat on the record (code:
`crates/sovereign-db/src/schema.rs`, `Document`).

### Fields as built

**Status:** ✅ Built — 28 fields. v1.3 had a conceptual schema only; seventeen of these fields
(soft delete, encryption, reliability, PII, pinning, peer review) have no counterpart in it.

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
| Web origin | `source_url` | Option\<String\> | Meant to be set when the document was fetched from the web; the native shell's "Save page" does not set it (it puts the URL in the body) |
| Reliability (legacy, ch. 04) | `reliability_classification` | Option\<String\> | "Factual" / "Opinion" / "Fiction" |
| | `reliability_score` | Option\<f32\> | 0.0–5.0 |
| | `reliability_assessment` | Option\<String\> | JSON rubric |
| | `assessed_at` | Option\<DateTime\> | Last assessment |
| PII (ch. 09) | `body_raw_encrypted` | Option\<String\> | Sealed pre-tokenization body, for gated reveal |
| | `body_raw_nonce` | Option\<String\> | Its nonce |
| | `pii_scanned_at` | Option\<DateTime\> | `None` = not yet scanned |
| Peer review (ch. 05) | `peer_review_pending` | bool | A paired device's sync overwrote this document; awaiting review |
| | `peer_review_peer` | Option\<String\> | Device that produced the change |
| | `peer_review_at` | Option\<DateTime\> | When it was applied |
| | `peer_review_prior_commit` | Option\<String\> | Commit holding the pre-overwrite version, for one-click restore |
| | `peer_review_assessment` | Option\<String\> | JSON verdict of the local audit, when a model is available |

All fields added after the first schema are `#[serde(default)]`, so older rows deserialize
without migration. `Document::new` creates the default body `{"body":"","images":[]}`.

The `reliability_*` group is superseded by the `Assessment` record of ch. 04 and kept until that
record exists.

### Content encoding

**Status:** ○ Decided 2026-10-10 (Céline), to be built — reads are already tolerant (lot 0).

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
- The block schemas are specified below, in *Document format v2 — blocks*.

What follows describes the forms in use today.

The intended form of `content` is a JSON envelope (code: `crates/sovereign-core/src/content.rs`,
`ContentFields`):

```json
{ "body": "markdown text", "images": [{ "path": "…", "caption": "…" }],
  "videos": [{ "path": "…", "caption": "…", "duration_secs": 12.0, "thumbnail_path": "…" }] }
```

Media are referenced by path; they are not stored in the database.

> ⚠ **Conflict — decision pending (Céline).** Writers disagree on what `content` holds. The
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
> One encoding has to be chosen — or the envelope made tolerant of a bare body — and every
> writer and reader aligned.


### Document format v2 — blocks

**Status:** ○ Specified 2026-10-10, to be reviewed by Céline — choices marked **[choice]**.

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

**Media reference** (`ref`): `{ "path": "<local path>", "sha256": "<hex>" }` today — media are not
copied into the encrypted store (originals stay where they are); the hash lets a moved or altered
file be detected. A media store would later replace `path` by an id.

**Reading the older forms (migration).**
- The v1 envelope `{"body", "images", "videos"}` → one `text` block holding `body`, then one
  `image` / `video` block per entry, in that order. v1 media have neither a description nor a
  hash: the migrated image block carries an empty `alt` flagged *needs a description*, and
  `ref.sha256` stays absent until the document service computes it (the editor never reads
  files). Such a block is valid in that state. *(Decided 2026-10-10.)*
- A raw-text body → one `text` block holding the whole string.
- Conversion happens at the document's next write, after a commit; a one-time pass converts the
  rest; then the tolerant read is removed.

**What the format does not do.** It does not encrypt (the encrypted layer encrypts `content`
whole, ch. 07); it does not hold the title, thread or trust state (record fields, ch. 03, ch. 04);
it does not version itself (history is the commit chain).
### Ownership

**Status:** ◐ Partial — a single boolean, set at creation; ◐ synced documents currently arrive
owned (tracked in the private backlog); the undetermined state is specified in ch. 04, not built.

`is_owned = true` marks the user's own content: control plane, trusted, not injection-fenced,
drawn as a rounded rectangle. `false` marks external content: data plane, fenced, drawn as a
parallelogram (ch. 08, ch. 12).

The flag is set when the record is created by the surface that creates it. `update_document`
changes only title and content; the one way to change ownership afterwards is
`adopt_document`, which sets it to `true` (adopt external content) — there is no reverse
operation (code: `crates/sovereign-db/src/{traits.rs,surreal.rs}`). Requirement: a document's
ownership is the same on every device. ◐ Synced documents currently arrive owned; tracked in the
private backlog.

The tri-state — *undetermined, pending the user's judgment*, failing closed — and when
ownership is decided for imported content are specified in ch. 04 (*Ownership is not decided at
landing*). See also *Bulk import → Ownership at import* below.

### ○ Target (pending D3) — multi-party ownership

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

> ⚠ **Conflict — decision pending (Céline).** The code has `is_owned: bool` and `source_url`.
> Federation and multi-user sync are on the roadmap, so this richer model may be the target
> rather than a description that fell behind. Whether it is the target or stale is **D3**,
> open. It blocks the undetermined-ownership tri-state of ch. 04, because "undetermined" means
> something different under a boolean than under a provenance chain. The v1.x thread
> `members` block (see *Thread*) is the same question at thread level [link to D3 inferred, not
> stated in D3].

Not resolved here. Until D3 is answered, the built model above is the current design.

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
| `pending_edits` | ✗ Retired — replaced by peer review of synced writes (`peer_review_*`, ch. 05) |

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
| `modified_at` | DateTime UTC | Last writer wins across devices (ch. 05); defaults to now for rows older than sync |
| `deleted_at` | Option\<String\> | Soft-delete marker |
| `name_nonce`, `description_nonce` | Option\<String\> | Nonces; `None` = plaintext |
| `name_token_hashes` | Vec\<String\> | Blind index for find-by-name |

Operations (`GraphDB`): create, rename / re-describe (`update_thread`), find by name, move a
document to another thread, **merge** (all documents of the source move to the target, then the
source is soft-deleted), **split**, soft delete and restore. As built, the orchestrator's
`split_thread` intent creates the new, empty thread and moves no documents; the user is meant
to move documents afterwards (code: `crates/sovereign-ai/src/orchestrator.rs`).

Requirement: a thread's name and description are sealed at rest whichever operation creates the
thread (ch. 07). ◐ — not every creation path does so yet; tracked in the private backlog.

### Thread deletion and the fallback lane

**Status:** ◐ Partial — the native shell draws orphaned documents in an Unfiled lane; mobile
does not show them.

Deleting a thread soft-deletes the thread record only. Its documents keep their `thread_id` and
are not moved or deleted. Any document whose `thread_id` does not resolve to an active thread is
meant to be drawn in one shared fallback lane, "Unfiled" in the native shell (code:
`crates/sovereign-shell/src/canvas.rs`). As built:

- **Native shell:** the Unfiled lane exists, but the canvas has twelve lanes; with twelve or
  more threads the Unfiled lane falls on a real thread's lane, so orphaned documents appear in
  that thread, and documents created or dropped there can be filed in the wrong thread [by code
  reading].
- **Mobile:** lanes are built from active threads only, so orphaned documents are not shown
  (code: `frontend/src/lib/components/mobile/MobileCanvas.svelte`). The frozen Tauri desktop
  canvas puts them in its first lane.

Restoring the thread (`restore_soft_deleted_thread`) brings its documents back into its lane. No
UI exposes thread restore; only sync applies it (ch. 05). Once the thread is purged (30 days,
below), its documents stay in the fallback lane until moved.

The fallback lane also catches documents whose thread never existed locally.

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
them. The mobile app loads pending suggestions [whether it offers accept and dismiss:
unverified]. ◐ The native shell has no suggestion surface yet.

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

**Status:** ◐ Partial — not every commit-creating path stamps a MAC yet; tracked in the private
backlog (code: `crates/sovereign-db/src/encrypted.rs`).

Requirement: under the encrypted layer every new commit is stamped with a device-keyed MAC over
its canonical fields (document id, parent, message, snapshot including nonces and title hashes),
so local history is tamper-evident. Listing and reading history check the MAC and warn on
mismatch; **restore refuses** a commit whose MAC does not verify. Commits created before the MAC
existed carry none and remain readable; how long they stay accepted is tracked in the private
backlog. Keys and the MAC primitive: ch. 07.

### Restore

**Status:** ✅ Built.

Restoring a version writes the snapshot's title and content (with their nonces and title
hashes) back to the document and then **creates a new commit** "Restored from \<commit id\>" —
history is never rewritten, and the restore itself can be undone. Restore does not change lane
membership or deletion state. Under the encrypted layer, restore also refuses a snapshot that
predates nonce-carrying commits when the live row is encrypted, since it could not be decrypted.
Restore is reachable from the History window (native shell), the orchestrator's `restore`
intent, and peer review's one-click restore (ch. 05). The frozen Tauri desktop build also has a
history panel.

### Auto-commit policy

**Status:** ◐ Partial — two surfaces, two policies; the session-end commit is not built.

Intended policy: commit frequency adapts to activity. Under high activity, commit every 50 edits
or 5 minutes; under low activity, on context switch (document close) or at session end.

| Surface | As built |
|---|---|
| Mobile (Tauri host) | `AutoCommitEngine`: each save records an edit; a 30-second timer commits any document with ≥ 50 recorded edits or ≥ 5 minutes since its last commit; closing a document commits its pending edits ("Auto-commit on close") (code: `crates/sovereign-ai/src/autocommit.rs`, `crates/sovereign-app/src/lib.rs`) |
| Native shell | Every explicit save commits ("Saved edit"); a skill that rewrites a document commits ("Skill: \<name\>") (code: `crates/sovereign-shell/src/app.rs`) |

The native shell's per-save commits are a superset of the policy for its explicit-save editing
model. Neither surface commits at session end. Bulk import and the native shell's document
creation make no initial commit, so such a document has no version until its first commit. The
CLI `commit` command commits on demand (the CLI lives in the frozen `sovereign-tauri` binary).

### Branching and merge

**Status:** ○ Specified, not built — branching; ✗ Retired — merge through `pending_edits`,
replaced by last-writer-wins with peer review (ch. 05).

v1.x: the user checks out any historical snapshot, creates a branch from it, branches coexist
and show as a split on the timeline, merge conflicts go through `pending_edits` and are resolved
by the creator or an admin, with automatic merge when changes touch different sections. Always
commit before creating a branch.

Built: one linear chain per document, no checkout, no branch. Concurrent edits from paired
devices are resolved by last-writer-wins, and the overwritten local version is preserved as a
commit and surfaced for review (ch. 05). The `branchesfrom` relation type exists, but nothing
creates it. The timeline's branch visualisation, merge UI and historical scrubber (v1.3 Open
Question 5) are therefore not built either (ch. 12).

---

## Deletion

**Status:** ◐ Partial — soft delete and the 30-day purge run in the Tauri host (the mobile app); the native shell deletes permanently (⚠ below).

### Soft delete and 30-day purge

**Status:** ◐ Partial — soft delete and purge are built and scheduled in the Tauri host, which
ships as the mobile app; the native shell deletes documents permanently and schedules no purge.

Deleting a document or a thread sets `deleted_at`; every list, search and traversal excludes
soft-deleted records, and a soft-deleted record can be restored (`restore_soft_deleted_*`).
`purge_deleted(max_age)` permanently removes documents and threads whose `deleted_at` is older
than `max_age`. The Tauri host runs it every hour with a 30-day age, so purge runs on mobile
(code: `crates/sovereign-app/src/lib.rs`, `crates/sovereign-db/src/surreal.rs`).

- **Mobile (Tauri host):** document delete is a soft delete; thread delete is a soft delete
  (see *Thread deletion*).
- **Native shell:** deleting a document from its card asks for two clicks ("Delete…", then
  "Confirm delete") and removes the record immediately (`delete_document`), with no restore
  window; the action gate is not involved. A hard delete leaves no tombstone, so the next sync
  with a paired device that still holds the document brings it back (ch. 05). Threads are
  deleted only through the orchestrator's `delete_thread` intent, which soft-deletes. The shell
  schedules no purge, so what it soft-deletes is never purged on the desktop (code:
  `crates/sovereign-shell/src/app.rs`, `crates/sovereign-ai/src/orchestrator.rs`).

**Decided 2026-10-10 (Céline): delete is a soft delete everywhere, with a trash.** Deleting
moves the item to a trash the user can see; it stays recoverable for 30 days and is then purged.
The user can empty the trash at any time, which purges at once. There is no separate permanent
delete. ◐ The native shell's switch to soft delete is in progress; the trash view, a purge task
in the shell, and making a purge stick across paired devices (ch. 05) are open work.

- The orchestrator has no document-delete intent: `delete_document` is graded Destruct in the
  action gate, but no handler acts on it [by code reading].

Requirement: purge removes everything that carries a document's content. ◐ Being completed
(private backlog). Soft-deleted contacts, entities and PII records follow their own chapters
(ch. 09, ch. 13).

*Replaces v1.x:* v1.3 did not describe deletion.

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
the provenance record, credential routing, per-file overrides, image tier, stub upgrade and the
shell import window are not built (code: `crates/sovereign-import/src/lib.rs`,
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

The stub body lives inside the encrypted body, so the original path is never plaintext metadata.

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
The reason is trust, not tidiness: imported documents land as the user's own content (below),
and owned content is not screened for injected instructions (ch. 08), so code or config carrying
such a payload must not enter as if the user had written it. The dry-run counts these files
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
  an existing thread with that name is reused.
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

Every plan, dry-run and execute alike, ends with a **trust notice**: imported files become the
user's own content, owned content is treated as trusted and not screened for prompt injection,
and a file authored by someone else can carry instructions that steer the model; import only
what you trust. The notice is restated just before the passphrase prompt, which serves as the
affirmative consent.

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

**Status:** ◐ Partial — ownership is currently decided at import for the whole tree; see ch. 04.

Imported documents land as owned content (`is_owned = true`), so the user's own corpus is
neither fenced nor reliability-scored. Forcing a migrating user to re-own a corpus one file at a
time was judged a worse harm (decision 2026-07-18), and the burden is made explicit by the
trust notice. The intended model — ownership declared by the user per folder at dry-run, an
undetermined state that fails closed, and a per-file trust classification in the `Manifest` — is
specified in ch. 04 (*Ownership is not decided at landing*); its full form waits on D3.

### Provenance record

**Status:** ○ Specified, not built — increment 2.

Every imported document carries a structural provenance record: `source_path`, `source_format`,
original modified and created times, `imported_at`, `imported_by` (CLI or UI, with version),
`tier`, and a **content hash of the original**. The record is what makes stub-first a pipeline
rather than a one-shot: re-runs skip unchanged files by hash, and stubs are upgraded in place by
source path. Today only the stub body records the original path, and only for stubs.

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
| Document delete: soft (mobile) vs immediate, and undone by sync (native shell) | ⚠ Conflict — decision pending | *Soft delete and 30-day purge* |
| Multi-party ownership model: target or stale | ⚠ Open Question D3 (ch. 15) | *○ Target (pending D3)* |
| Undetermined ownership at import | Specified in ch. 04, blocked on D3 | *Ownership*, *Ownership at import* |
