# EPUB Export

## Purpose
Package the current document (or a thread of documents) into an EPUB ebook. EPUB is essentially a zipped HTML+metadata bundle — useful for long-form writing destined for e-readers.

## Capabilities
- `read_document` — minimum, for single-doc export.
- `read_all_documents` — required for thread-level export (each doc becomes a chapter).

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `export_doc` | Export Document as EPUB | JSON: `{"title": "...", "author": "..."}` (both optional) | `File` |
| `export_thread` | Export Thread as EPUB | Same params; uses current doc's thread | `File` |

## Sample input → output

`export_doc` returns:
```
File {
  name: "My Doc.epub",
  mime_type: "application/epub+zip",
  data: <epub bytes>,
}
```

## Complexity estimate
Medium. The hard part is the EPUB packaging (zip layout, manifest XML, navigation document); rendering markdown to HTML is already solved by the existing HTML Export skill.

## Host-db extensions needed
None.

## Suggested implementation notes
- Use the `epub-builder` crate. Mature, handles the OPF manifest, NCX navigation, and zip packaging.
- Reuse the markdown→HTML rendering logic from `crates/sovereign-skills/src/skills/html_export.rs` (consider extracting it into a shared `markdown_render` module).
- For thread export: each document becomes a chapter. Use document title as chapter title. Order by `modified_at` ascending.
- Default author/title: pull from the user profile if available, otherwise use `"Sovereign GE User"` and the document title.
- Embed minimal CSS (the HTML Export's CSS is a fine starting point).
- Output filename should be safe for filesystems — reuse the `sanitize_filename` from html_export.rs.
