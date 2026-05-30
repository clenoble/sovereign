# Slug Generator

## Purpose
Convert the document title (or any provided text) into a URL-friendly slug. Useful for blog post permalinks, file names, anchor IDs.

## Capabilities
- `read_document` — needs the document title (and optionally the body for "first heading" mode).

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `from_title` | Slug from Title | JSON: `{"separator": "-", "max_length": 60}` (both optional) | `StructuredData` |
| `from_first_heading` | Slug from First Heading | JSON same as above | `StructuredData` |

## Sample input → output

`from_title` with title `"My First Blog Post! (Draft)"`:
```json
{"slug": "my-first-blog-post-draft", "source": "My First Blog Post! (Draft)"}
```

## Complexity estimate
Small. Lowercase, replace non-alphanumerics with separator, collapse repeats, trim. The Table of Contents skill already has a `slugify()` function — extract and share.

## Host-db extensions needed
None.

## Suggested implementation notes
- Default separator `-`, alternative `_`. Reject other separators with a clear error.
- Default `max_length` 60; truncate at a word boundary, not mid-word.
- Unicode handling: use `unicode-normalization` to NFD-decompose, then strip combining marks (so `café` → `cafe`). For non-Latin scripts, transliterate via `deunicode` or fall back to keeping the script if transliteration would lose meaning.
- This is a 50-line skill that mostly reuses the existing `slugify()` in `crates/sovereign-skills/src/skills/table_of_contents.rs`. Consider extracting that into a shared utility module first.
