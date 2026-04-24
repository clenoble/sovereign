# Wrap / Unwrap Lines

## Purpose
Reflow paragraph text to a fixed column width, or undo wrapping by joining hard-broken lines back into single-line paragraphs. Useful when authoring documents that mix prose with tools that prefer one specific shape (email clients, code review systems, plaintext exports).

## Capabilities
- `read_document` — needs the body to reflow.
- `write_document` — returns a `ContentUpdate` with the new layout.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `wrap` | Wrap at N Columns | `"<int>"` (column width, default 80) | `ContentUpdate` |
| `unwrap` | Unwrap Paragraphs | `""` | `ContentUpdate` |

## Sample input → output

`wrap` with params `"40"` on:
```
This is a long single line of prose that exceeds forty columns without any breaks.
```

returns:
```
This is a long single line of prose that
exceeds forty columns without any breaks.
```

`unwrap` on the wrapped version restores the single-line paragraph.

## Complexity estimate
Small for the core algorithm. Medium once you respect markdown structure (don't reflow inside code fences, lists, blockquotes, tables).

## Host-db extensions needed
None.

## Suggested implementation notes
- Word-boundary aware: never split mid-word; use `unicode-segmentation` if you need grapheme correctness for non-ASCII.
- Preserve double-newline paragraph breaks; only join *single*-newline breaks during unwrap.
- Skip fenced code blocks, indented (4-space) code, list items (start with `- `, `* `, `1. `), blockquotes (start with `>`), and tables (start with `|`). Each of these has structural meaning that wrapping would corrupt.
- Recommend the `textwrap` crate for the wrap implementation — handles line breaking nicely.
