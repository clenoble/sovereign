# Word Frequency

## Purpose
Compute the most-used words across the entire workspace (or a chosen thread). Useful for understanding what your notes are actually *about* over time, finding overused phrases, or spotting topic drift.

## Capabilities
- `read_all_documents` — needs to read every doc.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `workspace` | Word Frequency Across Workspace | JSON: `{"top_n": 50, "min_length": 3}` (both optional) | `StructuredData` |
| `thread` | Word Frequency in Current Thread | Same params | `StructuredData` |

## Sample input → output

`workspace` (defaults) returns:
```json
{
  "top_terms": [
    {"term": "rust", "count": 412, "doc_count": 38},
    {"term": "skill", "count": 287, "doc_count": 24},
    {"term": "graph", "count": 201, "doc_count": 19}
  ],
  "doc_count_scanned": 142,
  "total_tokens": 87432
}
```

## Complexity estimate
Medium. The computation is straightforward; the cost is in I/O — `get_document` for every doc.

## Host-db extensions needed
None for the basic version.

For the `thread` action: `list_documents(Some(thread_id))` already exists; `get_document(id)` already exists. Both compose without extension.

## Suggested implementation notes
- Tokenization: lowercase, drop punctuation, drop tokens shorter than `min_length` (default 3).
- Stop words: drop a standard English stop word list (use the `stop-words` crate). Make this a `params.drop_stop_words` toggle for users who actually want to see "the" / "and" rankings.
- Track per-term: `count` (total occurrences) and `doc_count` (number of distinct docs containing the term). The latter helps distinguish "concentrated in one doc" from "common across the corpus".
- Strip markdown before tokenizing (reuse the helper from Readability Score if extracted).
- For workspaces with many docs (1000+), consider streaming the result and emitting progress events. Beyond v0 scope.
