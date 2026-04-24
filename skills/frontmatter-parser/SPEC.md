# Frontmatter Parser

## Purpose
Detect and parse YAML or TOML frontmatter at the top of a markdown document and surface it as structured data. Common in static-site workflows (Jekyll, Hugo, Zola, etc.).

## Capabilities
- `read_document` — needs the body to scan.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `parse` | Parse Frontmatter | `""` | `StructuredData` |

## Sample input → output

`parse` on:
```
---
title: My Post
date: 2026-04-24
tags: [rust, sovereign]
draft: false
---

# Body starts here
```

returns:
```json
{
  "format": "yaml",
  "fields": {
    "title": "My Post",
    "date": "2026-04-24",
    "tags": ["rust", "sovereign"],
    "draft": false
  },
  "body_offset": 92
}
```

If no frontmatter is found, returns `{"format": null, "fields": {}, "body_offset": 0}`.

## Complexity estimate
Small. Detection is straightforward (file starts with `---\n` or `+++\n`); parsing reuses existing crates.

## Host-db extensions needed
None.

## Suggested implementation notes
- YAML delimiter: `---\n...\n---\n`. TOML delimiter: `+++\n...\n+++\n`. JSON-frontmatter (`{...}` at top, used by some Hugo configs) is rare — skip unless requested.
- For YAML, use the `serde_yml` crate already in the workspace (added by Wave B's JSON/YAML Formatter).
- For TOML, use the `toml` crate (already in the workspace).
- Return `body_offset` so consumers can reliably split frontmatter from body without re-parsing.
- Don't fail on malformed frontmatter — return an `error` field in the output and `format: null`.
