# Memory Consolidation — AI-Suggested Document Links

**Status**: Implemented — commit f9cd5ad (2026-03-09)

## Context

Inspired by Google's Always-On Memory Agent "sleep consolidation" pattern: a background process that periodically scans the workspace, finds cross-document patterns, and suggests relationships — particularly between web (external) documents and owned documents. Key requirement: **AI-created relationships must be structurally distinct from user-created ones in the database**.

## Design Decisions

1. **Separate SurrealDB edge table** (`suggested_link`) rather than extending `related_to` with a `source` field. Keeps AI suggestions structurally distinct, avoids migration, enables clean queries ("show all AI suggestions").

2. **Lifecycle**: Pending → Accepted (promotes to real `related_to` edge) or Dismissed (never re-suggested for same pair).

3. **Opportunistic scheduling**: No fixed timer. Consolidation runs only when the system is idle — the LLM isn't generating, the bubble is in `Idle` state, and the user hasn't interacted for a configurable cooldown (default 60s). User tasks always take priority. A lightweight idle-watcher loop checks every 30s whether conditions are met, and additionally requires at least 1 doc modified since the last run to avoid wasted inference.

4. **3B model prompt**: Batch 5 candidate pairs with short fingerprints (title + 200 chars). Output ~200 tokens. No 7B escalation needed.

5. **Cross-pollination priority**: External↔owned pairs ranked first, then same-owned across different threads.

---

## Phase 1 — Schema & DB Layer

### Files: `sovereign-db/src/{schema.rs, traits.rs, surreal.rs, mock.rs}`

**New types in `schema.rs`:**
```rust
pub enum SuggestionSource { Consolidation, Chat }
pub enum SuggestionStatus { Pending, Accepted, Dismissed }

pub struct SuggestedLink {
    pub id: Option<Thing>,
    pub in_: Option<Thing>,    // target doc
    pub out: Option<Thing>,    // source doc
    pub relation_type: RelationType,
    pub strength: f32,
    pub rationale: String,     // LLM explanation
    pub source: SuggestionSource,
    pub status: SuggestionStatus,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}
```

**New trait methods in `traits.rs`:**
- `create_suggested_link(from_id, to_id, relation_type, strength, rationale, source) -> SuggestedLink`
- `list_pending_suggestions() -> Vec<SuggestedLink>`
- `list_suggestions_for_document(doc_id) -> Vec<SuggestedLink>`
- `resolve_suggestion(id, status) -> SuggestedLink` — when Accepted, also creates a real `related_to` edge
- `suggestion_exists(from_id, to_id) -> bool` — checks any status, bidirectional

**SurrealDB** (`surreal.rs`): `RELATE $from->suggested_link->$to SET ...`, add index on `status` field in `init_schema`.

**Tests** (6):
- Create and list suggested links
- Resolve as accepted → verify `related_to` edge created
- Resolve as dismissed → verify no `related_to` edge
- `suggestion_exists` prevents duplicates
- `suggestion_exists` is bidirectional (A→B and B→A)
- List pending excludes resolved

---

## Phase 2 — Consolidation Engine

### New file: `sovereign-ai/src/consolidation.rs`

**`ConsolidationEngine`** struct with one public method:

```rust
pub async fn run_cycle(&self, db, router, formatter) -> Result<Vec<SuggestedLink>>
```

**Algorithm:**
1. Fetch all active documents + all existing relationships + all existing suggestions
2. Build candidate pairs: prioritize `is_owned=true` ↔ `is_owned=false` (web↔owned), then cross-thread owned↔owned
3. Filter out pairs that already have a `related_to` edge or any `suggested_link`
4. Rank by recency (at least one doc modified in last 24h)
5. Select top 5 pairs
6. Build fingerprints: `title + first 200 chars of content`
7. Single LLM call to 3B router — batch all 5 pairs, ask for JSON array of scores
8. Filter results where `strength >= 0.4`
9. Persist passing pairs as `SuggestedLink` with `status: Pending`
10. Return new suggestions for event emission

**Prompt template:**
```
System: Given document pairs, determine if they are meaningfully related.
Output a JSON array. For each pair:
{"pair":N,"related":true/false,"type":"supports|references|contradicts|continues|derivedfrom","strength":0.0-1.0,"reason":"one sentence"}

User:
Pair 1:
A (owned): "Project Roadmap" — We plan to implement offline-first sync using CRDTs...
B (web): "CRDTs for Mortals" — Conflict-free replicated data types provide...
[up to 5 pairs]
```

**Tests** (6):
- Candidate selection prefers cross-owned pairs
- Candidate selection skips existing relationships
- Fingerprint truncation
- JSON response parsing (valid + malformed)
- Empty DB → no suggestions
- Fully connected graph → no suggestions

### Wire in `sovereign-ai/src/lib.rs`: add `pub mod consolidation;`

---

## Phase 3 — Orchestrator Integration

### File: `sovereign-ai/src/orchestrator.rs`

New public methods:
```rust
/// Run one consolidation cycle. Called by the idle watcher when conditions are met.
pub async fn consolidate_memory(&self) -> Result<()>

/// Returns true if the LLM is not currently generating (classifier mutex is not locked).
pub fn is_model_idle(&self) -> bool
```

`consolidate_memory()` flow:
1. Check adaptive gating for `"consolidation"` in `UserProfile.suggestion_feedback`
2. Call `ConsolidationEngine::run_cycle()`
3. For each new suggestion, emit `LinkSuggested` event
4. Update profile feedback counter

`is_model_idle()`: tries `classifier.try_lock()` — if it succeeds, the model is free.

### File: `sovereign-core/src/interfaces.rs`

New event variants:
```rust
LinkSuggested { suggestion_id, from_doc_id, from_title, to_doc_id, to_title, relation_type, strength, rationale }
LinkSuggestionResolved { suggestion_id, accepted: bool }
```

### Tests (2):
- `consolidate_memory` with low acceptance rate suppresses emission
- `consolidate_memory` emits `LinkSuggested` events

---

## Phase 4 — Tauri Bridge

### File: `sovereign-app/src/tauri_commands.rs`

New commands:
- `accept_suggestion(id)` — calls `db.resolve_suggestion(id, Accepted)`, emits `LinkSuggestionResolved`
- `dismiss_suggestion(id)` — calls `db.resolve_suggestion(id, Dismissed)`, emits `LinkSuggestionResolved`
- `list_pending_suggestions()` — returns `Vec<SuggestionDto>`

### File: `sovereign-app/src/tauri_events.rs`

New payload struct + match arm for `LinkSuggested` and `LinkSuggestionResolved`.

### File: `sovereign-app/src/main.rs`

Spawn an **idle-watcher** task:

```rust
tauri::async_runtime::spawn(async move {
    let idle_cooldown = Duration::from_secs(60);  // user inactive for 60s
    let check_interval = Duration::from_secs(30); // poll every 30s
    let mut last_user_activity = Instant::now();

    loop {
        tokio::time::sleep(check_interval).await;

        // Skip if model is busy (user query, chat, reliability assessment)
        if !orch.is_model_idle() {
            last_user_activity = Instant::now(); // reset cooldown
            continue;
        }

        // Skip if bubble is not Idle (proposing, executing, etc.)
        // (bubble state tracked via shared atomic or channel)
        if bubble_state.load() != BubbleVisualState::Idle {
            last_user_activity = Instant::now();
            continue;
        }

        // Skip if cooldown hasn't elapsed since last user activity
        if last_user_activity.elapsed() < idle_cooldown {
            continue;
        }

        // All clear — run consolidation (low priority, won't block user)
        let _ = orch.consolidate_memory().await;

        // After running, wait at least 5 minutes before checking again
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
});
```

The watcher also listens for user activity events (any Tauri command invocation resets `last_user_activity`). This ensures consolidation never competes with the user.

Register new commands in `.invoke_handler()`.

---

## Phase 5 — Frontend

### New file: `frontend/src/lib/stores/suggestions.svelte.ts`

Svelte 5 rune store: `$state({ pending: [], visible: false })` + mutation functions.

### File: `frontend/src/lib/api/commands.ts`

Add `acceptSuggestion(id)`, `dismissSuggestion(id)`, `listPendingSuggestions()` + `SuggestionDto` type.

### File: `frontend/src/lib/api/events.ts`

Add `link-suggested` listener → push to suggestions store.
Add `link-suggestion-resolved` listener → remove from pending.

### File: `frontend/src/lib/components/Bubble.svelte`

Add badge showing count of pending suggestions. Click opens suggestion list.

### File: `frontend/src/routes/+page.svelte`

Wire suggestion panel visibility (dropdown or floating panel near bubble).

### Suggestion card UI:
- From doc title (owned) ↔ To doc title (external)
- Relationship type badge + strength indicator
- Rationale text (LLM explanation)
- Approve / Dismiss buttons

---

## Verification

1. **Unit tests**: `cargo test -p sovereign-db` (6 new), `cargo test -p sovereign-ai --no-default-features` (8 new)
2. **Compile check**: `cargo check -p sovereign-app --features tauri-ui,web-browse --no-default-features`
3. **Frontend check**: `cd frontend && npm run check`
4. **Manual test**: Seed a mix of owned + web documents, trigger consolidation manually via Tauri command, verify suggestions appear in DB and UI
