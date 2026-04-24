# Duplicate Finder

## Purpose
Identify documents in the workspace whose content is identical or near-identical (e.g. accidental imports, paste duplicates, multiple drafts of the same note that drifted apart). Returns clusters of similar docs with a similarity score.

## Capabilities
- `read_all_documents` — needs to read every doc's content.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `find` | Find Duplicates | JSON: `{"threshold": 0.85}` (Jaccard similarity, 0..1, default 0.85) | `StructuredData` |

## Sample input → output

`find` returns:
```json
{
  "clusters": [
    {
      "similarity": 1.0,
      "kind": "exact",
      "docs": [
        {"id": "document:abc", "title": "Notes from kickoff"},
        {"id": "document:def", "title": "kickoff"}
      ]
    },
    {
      "similarity": 0.91,
      "kind": "near",
      "docs": [
        {"id": "document:xyz", "title": "Q1 plan"},
        {"id": "document:uvw", "title": "Q1 plan (draft)"}
      ]
    }
  ],
  "doc_count_scanned": 142,
  "cluster_count": 2
}
```

## Complexity estimate
Medium-Large. Naive pairwise comparison is O(N²) which is OK for small workspaces (< 1000 docs) but not large ones. Better algorithms exist (MinHash + LSH for near-duplicates) but add dependency weight.

## Host-db extensions needed
None.

## Suggested implementation notes
- Tokenize each doc into a set of normalized words (lowercase, drop punctuation, drop very common stop words). Use Jaccard similarity on the resulting sets.
- For exact-duplicate detection (similarity = 1.0), hash each doc body with SHA-256 and group by hash. O(N) and catches the most common case (paste duplicates).
- For near-duplicate detection, the `simhash` crate or a hand-rolled MinHash works. Recommend hand-rolled MinHash with k=128 hash functions; gives reasonable accuracy/perf tradeoff.
- Strip markdown before comparing (use the same approach as Readability Score) so `# Foo` and `Foo` are treated as the same content.
- Skip very short docs (< 50 words) — too easy to false-positive on common phrases.
- Surface `cluster_count` and the largest cluster's size in the result so users know if the operation found something worth reviewing.
