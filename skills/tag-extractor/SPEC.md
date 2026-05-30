# Tag Extractor

## Purpose
Suggest a small set of representative keywords/tags for the document based on term frequency and basic relevance heuristics. Useful for retroactive tagging of imported notes, suggesting tags for new posts, or quick topic identification.

## Capabilities
- `read_document` — needs the body to scan.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `extract` | Extract Tags | JSON: `{"max_tags": 8}` (default 8) | `StructuredData` |

## Sample input → output

`extract` on a document about Rust async returns:
```json
{
  "tags": [
    {"term": "async", "score": 0.92},
    {"term": "tokio", "score": 0.81},
    {"term": "future", "score": 0.74},
    {"term": "rust", "score": 0.68}
  ],
  "count": 4
}
```

## Complexity estimate
Medium. Pure-heuristic implementation is small; doing it well requires either an LLM or corpus statistics (TF-IDF needs the rest of the workspace, not just one doc).

## Host-db extensions needed
None for the heuristic version. For TF-IDF, would need a `read_all_documents` upgrade to compute IDF over the workspace.

## Suggested implementation notes
- Tokenize: lowercase, strip punctuation, drop stop words (use a bundled list — `stop-words` crate has good defaults).
- Score by term frequency, with a boost for terms appearing in headings (parse via `pulldown-cmark`).
- Drop very-short tokens (≤ 2 chars), pure-numeric tokens, and tokens that look like code identifiers (have `_` or `::`).
- Optional LLM-backed mode: if `llm_inference` is granted, send the doc to the local LLM with a prompt like "List 5 tags for this document, comma-separated, no commentary." Keep the heuristic as the default since it's fast and deterministic.
- Don't auto-apply tags to the document; this is a *suggestion* skill. A separate "Apply Tags" skill or UI flow consumes the output.
