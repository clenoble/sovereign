# Strip Comments

## Purpose
Remove HTML comments (`<!-- ... -->`) from a document. Markdown allows HTML comments as inline notes that don't render; this skill cleans them out before sharing or exporting. Optional pass for code-block comments based on language.

## Capabilities
- `read_document` — needs the body to scan.
- `write_document` — returns a `ContentUpdate` with comments removed.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `strip_html` | Strip HTML Comments | `""` | `ContentUpdate` |
| `strip_all` | Strip All Comments (HTML + code) | `""` | `ContentUpdate` |

## Sample input → output

`strip_html` on:
```
# Title

<!-- TODO: rewrite this section -->
Real content.

<!--
multi-line
comment
-->
```

returns:
```
# Title

Real content.

```

## Complexity estimate
Small for HTML comments (regex or hand-rolled state machine). Medium for `strip_all` since "comment" syntax depends on the code fence language (`//` for Rust/JS, `#` for Python/YAML, `--` for SQL/Lua, etc.) — needs a small lookup table.

## Host-db extensions needed
None.

## Suggested implementation notes
- HTML comment regex: `<!--[\s\S]*?-->` (non-greedy, multiline-aware via `[\s\S]`).
- **Important:** Don't strip the Table of Contents skill's marker (`<!-- toc -->` / `<!-- /toc -->`) — these are intentional markers, not comments. Either skip a known-marker allowlist, or only run on user request, or add a `preserve_markers: bool` param defaulting to `true`.
- For `strip_all`, parse fenced code blocks via `pulldown-cmark` to know each block's language tag, then apply the language-appropriate comment stripper.
- Preserve surrounding whitespace conservatively — collapsing comment-stripped lines into the surrounding text often produces awkward double blank lines.
