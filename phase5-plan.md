# Phase 5: Thread Management, Doc Assignment & Version Tracking

## Context

Phase 4 delivered MVP skills, document panel, orchestrator bubble, and dynamic taskbar. The remaining Phase 5 features are:
1. **Intent thread management** via AI orchestrator (create/rename/delete threads, move docs)
2. **Per-document version tracking** with auto-commit (git-like, per spec)
3. **Session log** (append-only JSONL)
4. **Canvas timeline layout** (horizontal axis = time, version cards)

These match the spec sections: Intent Thread Manifest (lines 189-231), Version Control (lines 260-283), Session Log (lines 1142-1162), and the user's preference for horizontal timeline with version cards as canvas cards.

---

## Sub-Phase 5.2: Thread CRUD via Orchestrator

### DB Layer Changes

**`sovereign-db/src/traits.rs`** — Add to `GraphDB` trait:
```rust
async fn update_thread(&self, id: &str, name: Option<&str>, description: Option<&str>) -> DbResult<Thread>;
async fn delete_thread(&self, id: &str) -> DbResult<()>;
async fn move_document_to_thread(&self, doc_id: &str, new_thread_id: &str) -> DbResult<Document>;
```

**`sovereign-db/src/surreal.rs`** — Implement the three new methods:
- `update_thread`: fetch+merge+update pattern (same as `update_document`)
- `delete_thread`: delete the thread record (docs remain with orphan thread_id — "Uncategorized" lane catches them)
- `move_document_to_thread`: update doc's `thread_id` field + `modified_at`

### AI Layer Changes

**`sovereign-ai/src/llm/prompt.rs`** — Expand `ROUTER_SYSTEM_PROMPT` actions:
```
"search|open|create|navigate|summarize|create_thread|rename_thread|delete_thread|move_document|unknown"
```

**`sovereign-ai/src/intent/parser.rs`** — Add heuristic keywords:
- `"create thread"`, `"new thread"`, `"new project"` → `create_thread`
- `"rename thread"`, `"rename project"` → `rename_thread`
- `"delete thread"`, `"remove thread"` → `delete_thread`
- `"move"`, `"assign"`, `"reassign"` → `move_document`

**`sovereign-core/src/interfaces.rs`** — New `OrchestratorEvent` variants:
```rust
ThreadCreated { thread_id: String, name: String },
ThreadRenamed { thread_id: String, name: String },
ThreadDeleted { thread_id: String },
DocumentMoved { doc_id: String, new_thread_id: String },
```

**`sovereign-ai/src/orchestrator.rs`** — Handle new intents in `handle_query()`:
- `"create_thread"` → `db.create_thread()` → emit `ThreadCreated`
- `"rename_thread"` → find thread by name → `db.update_thread()` → emit `ThreadRenamed`
- `"delete_thread"` → find thread → `db.delete_thread()` → emit `ThreadDeleted`
- `"move_document"` → find doc + thread by name → `db.move_document_to_thread()` → emit `DocumentMoved`

### UI Layer Changes

**`sovereign-ui/src/app.rs`** — Handle new `OrchestratorEvent` variants in tick callback:
- `ThreadCreated` / `ThreadDeleted` / `DocumentMoved`: log for now (canvas re-layout is Sub-Phase 5.4)
- `ThreadRenamed`: update canvas lane label if visible

### Tests (new)

Add tests in `surreal.rs` for update_thread, delete_thread, move_document_to_thread.
Add tests in `parser.rs` for new heuristic keywords.

---

## Sub-Phase 5.3: Per-Document Version Tracking

### Schema Changes

**`sovereign-db/src/schema.rs`** — Modify `Commit` and `DocumentSnapshot`:

```rust
pub struct Commit {
    pub id: Option<Thing>,
    pub document_id: String,         // NEW: which document this commit belongs to
    pub parent_commit: Option<String>, // NEW: parent commit ID (chain)
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub snapshot: DocumentSnapshot,   // CHANGED: single snapshot, not Vec
}

pub struct DocumentSnapshot {
    pub document_id: String,
    pub title: String,
    pub content: String,
}
```

**`sovereign-db/src/schema.rs`** — Add `head_commit` to `Document`:
```rust
pub struct Document {
    // ... existing fields ...
    pub head_commit: Option<String>,  // NEW: pointer to latest commit
}
```

### DB Layer Changes

**`sovereign-db/src/traits.rs`** — Replace global commit with per-doc:
```rust
// Remove: async fn commit(&self, message: &str) -> DbResult<Commit>;
// Remove: async fn list_commits(&self) -> DbResult<Vec<Commit>>;

// Add:
async fn commit_document(&self, doc_id: &str, message: &str) -> DbResult<Commit>;
async fn list_document_commits(&self, doc_id: &str) -> DbResult<Vec<Commit>>;
async fn get_commit(&self, commit_id: &str) -> DbResult<Commit>;
async fn restore_document(&self, doc_id: &str, commit_id: &str) -> DbResult<Document>;
```

**`sovereign-db/src/surreal.rs`** — Implement:
- `commit_document`: snapshot current doc state, create commit with `parent_commit = doc.head_commit`, update doc.head_commit
- `list_document_commits`: `SELECT * FROM commit WHERE document_id = $doc_id ORDER BY timestamp DESC`
- `get_commit`: select single commit by ID
- `restore_document`: get commit's snapshot, update document's content/title/modified_at, create new commit with message "Restored from {commit_id}"
- Add index: `DEFINE INDEX idx_commit_doc ON commit FIELDS document_id`

### Auto-Commit Engine

**New file: `sovereign-ai/src/autocommit.rs`**

```rust
pub struct AutoCommitEngine {
    db: Arc<SurrealGraphDB>,
    edit_counts: HashMap<String, u32>,    // doc_id -> edit count since last commit
    last_commit_times: HashMap<String, Instant>,
    last_activity: Instant,
}

impl AutoCommitEngine {
    pub fn record_edit(&mut self, doc_id: &str);
    pub async fn check_and_commit(&mut self);  // called periodically
}
```

**Policy** (from spec):
- High activity: commit after 50 edits OR 5 minutes since last commit
- Low activity: commit on context switch (document close) or session end
- Always commit before thread operations that affect the document

**Integration in `sovereign-app/src/main.rs`**:
- Create `AutoCommitEngine` alongside orchestrator
- Call `record_edit()` from the save callback (every time user saves)
- Call `check_and_commit()` from a periodic tokio interval (every 30s check)
- Call commit on document panel close (via new `SkillEvent::DocumentClosed`)

### New SkillEvent Variant

**`sovereign-core/src/interfaces.rs`**:
```rust
pub enum SkillEvent {
    OpenDocument { doc_id: String },
    DocumentClosed { doc_id: String },  // NEW: triggers auto-commit
}
```

### Tests

Update existing `test_commit_snapshots_documents` and `test_list_commits` for per-doc API.
Add: `test_commit_document_creates_chain`, `test_restore_document`, `test_list_document_commits`.

---

## Sub-Phase 5.4: Session Log + Canvas Timeline + History Intents

### Session Log

**New file: `sovereign-ai/src/session_log.rs`**

```rust
pub struct SessionLog {
    writer: BufWriter<File>,  // append-only
    path: PathBuf,
}

impl SessionLog {
    pub fn open(dir: &Path) -> Result<Self>;
    pub fn log_user_input(&mut self, mode: &str, content: &str, intent: &str);
    pub fn log_action(&mut self, action: &str, details: &str);
}
```

- Path: `~/.sovereign/orchestrator/session_log.jsonl` (plaintext for MVP, encryption post-MVP per spec)
- Each line: `{"ts":"ISO-8601","type":"user_input|orchestrator_action","..."}`
- Integrate into `Orchestrator`: log every query + every action result

### Canvas Timeline Layout

**`sovereign-canvas/src/layout.rs`** — Modify `place_cards_in_lane`:
- Sort documents within each lane by `modified_at` ascending (left = oldest, right = newest)
- X position = `LANE_HEADER_WIDTH + time_index * (CARD_WIDTH + CARD_SPACING_H)`
- This makes horizontal axis = timeline (per user preference)

### History & Restore Intents

**`sovereign-ai/src/llm/prompt.rs`** — Add actions: `"history"`, `"restore"`

**`sovereign-ai/src/intent/parser.rs`** — Heuristics:
- `"history"`, `"versions"`, `"changelog"` → `history`
- `"restore"`, `"revert"`, `"rollback"` → `restore`

**`sovereign-ai/src/orchestrator.rs`** — Handle:
- `"history"` → `db.list_document_commits(doc_id)` → emit `OrchestratorEvent::VersionHistory { doc_id, commits }`
- `"restore"` → `db.restore_document(doc_id, commit_id)` → emit `DocumentOpened`

**`sovereign-core/src/interfaces.rs`** — New event:
```rust
OrchestratorEvent::VersionHistory { doc_id: String, commits: Vec<CommitSummary> },
```

Where `CommitSummary` is a lightweight struct (id, message, timestamp) in `sovereign-core`.

### Version Cards on Canvas (stretch)

When `VersionHistory` event arrives:
- Create temporary `CardLayout` entries for historical versions
- Position along the same lane, earlier X positions (following timeline)
- Visually distinct (dimmed/ghost style via CSS class)
- Click opens a read-only document panel showing that version's content

---

## Implementation Order

1. **5.2** Thread CRUD (DB → AI → orchestrator → UI) — foundational, unblocks everything
2. **5.3** Per-doc version tracking (schema change → DB → auto-commit → integration)
3. **5.4** Session log + timeline layout + history intents + version cards

---

## Files Modified

| File | Sub-Phase | Change |
|------|-----------|--------|
| `sovereign-db/src/schema.rs` | 5.3 | Commit per-doc, parent_commit, Document.head_commit |
| `sovereign-db/src/traits.rs` | 5.2, 5.3 | update_thread, delete_thread, move_doc, per-doc commit API |
| `sovereign-db/src/surreal.rs` | 5.2, 5.3 | Implement all new trait methods + indexes |
| `sovereign-core/src/interfaces.rs` | 5.2, 5.3, 5.4 | New OrchestratorEvent variants, SkillEvent::DocumentClosed, CommitSummary |
| `sovereign-ai/src/llm/prompt.rs` | 5.2, 5.4 | Expand ROUTER/REASONING system prompts with new actions |
| `sovereign-ai/src/intent/parser.rs` | 5.2, 5.4 | Heuristic keywords for thread/history/restore intents |
| `sovereign-ai/src/orchestrator.rs` | 5.2, 5.4 | Handle new intents (thread CRUD, history, restore) |
| `sovereign-ai/src/autocommit.rs` | 5.3 | **New**: AutoCommitEngine |
| `sovereign-ai/src/session_log.rs` | 5.4 | **New**: SessionLog (append-only JSONL) |
| `sovereign-ui/src/app.rs` | 5.2, 5.3 | Handle new OrchestratorEvents, DocumentClosed event |
| `sovereign-canvas/src/layout.rs` | 5.4 | Sort docs by modified_at within lanes (timeline) |
| `sovereign-app/src/main.rs` | 5.3, 5.4 | Wire AutoCommitEngine, SessionLog, DocumentClosed handling |

---

## Verification

### Per sub-phase:

**5.2 — Thread CRUD:**
- `cargo test --workspace` — new DB tests + parser tests pass
- `sovereign run` → type "create thread called Testing" → new lane appears on canvas
- Type "move Research Notes to Testing" → card moves to new lane

**5.3 — Version Tracking:**
- `cargo test --workspace` — per-doc commit tests pass
- Open doc → edit → save → close → reopen → edit → save
- CLI: `sovereign list-commits --doc-id document:xyz` shows commit chain
- Auto-commit triggers after 50 edits or 5 min

**5.4 — Session Log + Timeline + History:**
- `~/.sovereign/orchestrator/session_log.jsonl` populated after queries
- Canvas cards ordered left-to-right by modified_at
- Type "show history of Research Notes" → version cards appear on canvas
- Click version card → read-only panel shows historical content
