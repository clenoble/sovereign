# Remove Duplicates

## Purpose
Strip repeated lines or paragraphs from a document. Useful when consolidating notes from multiple sources, cleaning up generated lists, or reviewing a rough import.

## Capabilities
- `read_document` — needs the body to scan for duplicates.
- `write_document` — returns a `ContentUpdate` with deduplicated content.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `dedupe_lines` | Remove Duplicate Lines | `""` | `ContentUpdate` |
| `dedupe_paragraphs` | Remove Duplicate Paragraphs | `""` | `ContentUpdate` |

## Sample input → output

`dedupe_lines` on:
```
apple
banana
apple
cherry
banana
```

returns:
```
apple
banana
cherry
```

`dedupe_paragraphs` treats blank-line-separated blocks as units and keeps the first occurrence of each.

## Complexity estimate
Small. Single linear scan with a `HashSet<String>` for membership checking; preserve first-occurrence order.

## Host-db extensions needed
None.

## Suggested implementation notes
- Decide whether comparison is case-sensitive or case-insensitive — recommend case-sensitive default with a `case_insensitive: bool` param if you want to extend.
- Trim trailing whitespace before comparing so `"foo"` and `"foo "` are considered the same; preserve the first variant in the output.
- Don't dedupe across code fences — a list inside ` ``` ` likely has intentional duplicates.
