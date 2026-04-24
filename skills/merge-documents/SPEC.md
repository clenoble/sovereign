# Merge Documents

## Purpose
Combine several documents from the same thread (or explicitly chosen) into a single consolidated document. Useful for end-of-week reviews, turning daily journal entries into a monthly summary, or stitching scattered research notes into a single piece.

## Capabilities
- `read_all_documents` — read each source document's content.
- `write_all_documents` — create the new merged document.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `merge_thread` | Merge All Docs in Thread | `""` | `StructuredData` |
| `merge_selected` | Merge Selected Docs | JSON: `{"doc_ids": ["document:a", "document:b", ...], "thread_id": "thread:x", "title": "Merged"}` | `StructuredData` |

## Sample input → output

`merge_thread` invoked from a doc in `thread:research`:

Source docs in thread (ordered by `modified_at`):
- "Day 1 notes"
- "Day 2 notes"
- "Day 3 notes"

Creates a new document titled `Merged — research — YYYY-MM-DD` in the same thread, with body:

```
# Merged — research — 2026-04-24

## Day 1 notes
<body of day 1>

---

## Day 2 notes
<body of day 2>

---

## Day 3 notes
<body of day 3>
```

Returns `StructuredData(merge_result)` with `{"doc_id": "document:new", "source_count": 3, "thread_id": "thread:research"}`.

## Complexity estimate
Small.

## Host-db extensions needed
None — `list_documents(Some(thread_id))` and `get_document(id)` both already exist in `SkillDbAccess`.

## Suggested implementation notes
- Default ordering: by document `modified_at` ascending (chronological). For deterministic output without that field exposed via SkillDbAccess, fall back to title order.
- Use `## <title>` as section dividers; separate with `---` horizontal rules.
- Don't include images/videos via the bridge — the SkillDbAccess `get_document` only returns `(title, thread_id, content)`. Note this in the merged doc with a footer ("(N images/videos in source docs not merged)") if any were detected.
- Don't delete the source documents. Merging is additive; deletion is a separate explicit step.
- For very large merges (>100KB combined), warn in the result payload — the new doc may be unwieldy.
