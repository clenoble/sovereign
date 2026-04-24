# Footnote Collector

## Purpose
Find inline footnote definitions scattered through a document and consolidate them at the end as a numbered endnotes section. Useful for long-form writing where you drop `[^note]` markers as you write and want the definitions tidied up before publishing.

## Capabilities
- `read_document` — needs the body to scan.
- `write_document` — returns a `ContentUpdate` with footnotes relocated.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `collect` | Collect Footnotes to End | `""` | `ContentUpdate` |

## Sample input → output

`collect` on:
```
First paragraph[^a].

[^a]: definition of a.

Second paragraph[^b].

Some text.

[^b]: definition of b.
```

returns:
```
First paragraph[^a].

Second paragraph[^b].

Some text.

---

## Footnotes

[^a]: definition of a.
[^b]: definition of b.
```

## Complexity estimate
Medium. Footnote syntax (CommonMark + GitHub Flavored Markdown extension) has subtleties: definitions can span multiple lines, references can repeat, footnotes can be nested. Use a real parser rather than regex.

## Host-db extensions needed
None.

## Suggested implementation notes
- Parse with `pulldown-cmark` and the `ENABLE_FOOTNOTES` option. Walk events to identify `Tag::FootnoteDefinition` regions; remove them from their original positions and append to the end.
- Preserve definition order based on first reference, not source order, so the numbering matches the reading flow.
- Don't renumber labels — `[^a]` stays `[^a]`. Renumbering is a separate concern.
- Edge case: if a footnote is referenced but never defined, leave the reference alone and don't fail — emit a warning in the log.
