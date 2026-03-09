# Code Review — 2026-03-09

Full codebase review across all 10 crates + Svelte frontend.

**Status**: Issues #1-15 resolved. Performance (#16-18) and code quality (#19-23) deferred to next pass.

---

## High Priority — Bugs & Correctness

### 1. `purge_deleted` always returns 0 instead of actual count
- **File**: `crates/sovereign-db/src/surreal.rs`
- **Impact**: Silent data integrity issue — caller never knows how many records were purged
- **Fix**: Parse DELETE response to return actual count

### 2. JSON deserialization silently falls back to wrong defaults
- **File**: `crates/sovereign-db/src/surreal.rs` (lines ~564, ~631)
- **Impact**: Incorrect data returned when deserialization fails — bugs are masked
- **Fix**: Log warnings on fallback, or propagate errors instead of silently defaulting

### 3. Browser store `openBrowser()` missing `bounds` parameter
- **File**: `frontend/src/lib/stores/browser.svelte.ts`
- **Impact**: Type mismatch with Tauri command — call will fail at runtime
- **Fix**: Add required `bounds` parameter to match Tauri command signature

### 4. Canvas `nowTimer` interval never cleaned up on unmount
- **File**: `frontend/src/routes/+page.svelte`
- **Impact**: Memory leak — interval keeps running after navigation/unmount
- **Fix**: Store interval ID, clear in `onDestroy` or return cleanup from `$effect`

---

## Medium Priority — Duplicate Code

### 5. `TOOLS` constant duplicates `READ_TOOLS` + `WRITE_TOOLS`
- **File**: `crates/sovereign-ai/src/tools.rs` (lines ~93-146)
- **Savings**: ~50 lines
- **Fix**: Derive `TOOLS` by concatenating `READ_TOOLS` and `WRITE_TOOLS` at init

### 6. Duplicate `parse_thing` closure
- **File**: `crates/sovereign-db/src/surreal.rs` (lines ~488, ~566)
- **Savings**: ~15 lines
- **Fix**: Extract to a named helper function

### 7. 30+ repetitive table validation checks
- **File**: `crates/sovereign-db/src/surreal.rs`
- **Savings**: ~60+ lines of boilerplate
- **Fix**: Create a `validate_table(thing, expected_table)` helper or macro

### 8. Identical conversation aggregation in `canvas_load()` and `list_contacts()`
- **File**: `crates/sovereign-app/src/tauri_commands.rs`
- **Savings**: ~30 lines
- **Fix**: Extract shared aggregation function

### 9. Duplicate approval/rejection logic in Chat.svelte and ConfirmAction.svelte
- **Files**: `frontend/src/lib/components/Chat.svelte`, `frontend/src/lib/components/ConfirmAction.svelte`
- **Savings**: ~20 lines
- **Fix**: Extract shared approval handler or unify into one component

### 10. Duplicate helper methods across email/signal/whatsapp channels
- **Files**: `crates/sovereign-comms/src/` (email, signal, whatsapp modules)
- **Savings**: ~120 lines (~40 lines each)
- **Fix**: Move shared logic to trait default methods or a shared helper module

### 11. Bidirectional pair insertion duplicated in consolidation.rs
- **File**: `crates/sovereign-ai/src/consolidation.rs` (lines ~94-109)
- **Savings**: ~10 lines
- **Fix**: Extract helper for bidirectional pair creation

---

## Medium Priority — Dead Code & Unused Exports

### 12. Unused `duress.rs` module (~143 lines)
- **File**: `crates/sovereign-app/src/duress.rs`
- **Savings**: 143 lines
- **Fix**: Remove module and `mod duress` declaration (or defer if planned for future use)

### 13. 8+ unused API command exports
- **File**: `frontend/src/lib/api/commands.ts`
- **Functions**: `greet`, `searchQuery`, `acceptSuggestion`, `dismissSuggestion`, `listDocuments`, `listThreads`, `listAllSkills`, possibly others
- **Fix**: Remove exports that have no callers, or mark with `// TODO: wire up` if planned

### 14. Injection scanning marked `#[allow(dead_code)]` with TODO
- **File**: `crates/sovereign-ai/src/orchestrator.rs` (line ~1261)
- **Fix**: Either wire the injection scanner into the pipeline or remove the dead method

### 15. Duplicate `VoiceEvent` type in sovereign-ui mirroring sovereign-ai
- **File**: `crates/sovereign-ui/src/` (voice-related module)
- **Fix**: Import from sovereign-ai instead of duplicating the enum

---

## Medium Priority — Performance

### 16. N+1 queries in `canvas_load()`
- **File**: `crates/sovereign-app/src/tauri_commands.rs`
- **Issue**: 1 query per thread for milestones + 1 query per conversation for messages
- **Fix**: Batch queries or add `list_all_milestones()` / `list_all_recent_messages()` DB methods

### 17. Missing DB indexes on `is_owned` and `deleted_at`
- **File**: `crates/sovereign-db/src/schema.rs` (init_schema)
- **Fix**: Add `DEFINE INDEX` statements for frequently filtered fields

### 18. Canvas `$effect` using `.map()` just to trigger reactivity
- **File**: `frontend/src/lib/stores/canvas.svelte.ts`
- **Fix**: Use proper reactive dependency tracking instead of `.map()` side-effect

---

## Low Priority — Code Quality

### 19. `.map_err(|e| e.to_string())?` repeated 40+ times
- **File**: `crates/sovereign-app/src/tauri_commands.rs`
- **Fix**: Implement `From<SovereignError> for String` or use a helper trait

### 20. tauri_commands.rs is 2,262 lines — monolithic file
- **File**: `crates/sovereign-app/src/tauri_commands.rs`
- **Fix**: Split into domain modules (documents, threads, contacts, canvas, suggestions, etc.)

### 21. 174 `.unwrap()` calls in sovereign-ai production code
- **File**: `crates/sovereign-ai/src/` (various)
- **Fix**: Audit and replace with proper error handling where panics are unacceptable

### 22. reqwest version mismatch: workspace 0.13 vs sovereign-comms 0.12
- **Files**: root `Cargo.toml`, `crates/sovereign-comms/Cargo.toml`
- **Fix**: Align to workspace version (0.13)

### 23. Redundant `.or_else(|| None)` pattern
- **File**: `crates/sovereign-ai/src/tools.rs` (line ~297)
- **Fix**: Remove — `.or_else(|| None)` is a no-op

---

## Clippy Warnings (auto-fixable)

### sovereign-core
- `config.rs:181` — `impl Default for AppConfig` can be derived
- `config.rs:247` — redundant closure: `|| Self::project_root()` → `Self::project_root`
- `profile.rs:29` — `impl Default for BubbleStyle` can be derived

### sovereign-db
- `schema.rs:163,211,217,244,260` — redundant closures: `|t| thing_to_raw(t)` → `thing_to_raw`
- `schema.rs:302` — `impl Default for ReadStatus` can be derived
