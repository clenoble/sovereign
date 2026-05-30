# Changelog Generator

## Purpose
Walk every commit on every document in a thread (or the whole workspace) and produce a chronological changelog of what changed when. Useful for "what did I work on last week / this sprint / this month" reviews.

## Capabilities
- `read_all_documents` — needs to enumerate documents and their commit history.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `for_thread` | Changelog for Thread | `""` (uses current doc's thread) | `ContentUpdate` or `File` |
| `for_workspace` | Changelog for Workspace | JSON: `{"since": "2026-04-01", "until": "2026-04-24"}` | `ContentUpdate` or `File` |

## Sample input → output

`for_thread` invoked from a doc in `thread:research` returns a markdown body like:
```
# Changelog — research

## 2026-04-24
- **Day 3 notes** (commit abc1234): Add findings from interview B
- **Day 3 notes** (commit def5678): Initial draft

## 2026-04-23
- **Day 2 notes** (commit 9876fed): Restructure into question/answer format
- **Day 1 notes** (commit fedcba9): First commit
```

## Complexity estimate
Medium. The hard part is the host-db extension (commit history isn't currently exposed to skills).

## Host-db extensions needed
**Yes — significant.** The current `SkillDbAccess` does not expose commits. Would need:

```rust
fn list_document_commits(&self, doc_id: &str)
    -> anyhow::Result<Vec<(String, String, chrono::DateTime<Utc>)>>;
    // (commit_id, message, timestamp)
```

The underlying `GraphDB::list_document_commits` already exists at `crates/sovereign-db/src/traits.rs:162`. Just need to bridge it through.

## Suggested implementation notes
- Group output by date (YYYY-MM-DD), then by document.
- Time range params (`since`, `until`) are inclusive on both ends; default to "all time" for `for_thread` and "last 7 days" for `for_workspace`.
- Output variant choice: `ContentUpdate` if invoked on an existing changelog doc (to refresh in place), `File` if generating fresh. Two actions might be cleaner — `refresh` vs `export`.
- Commit messages can be empty (auto-commits often are). Show "(no message)" rather than skipping the commit.
