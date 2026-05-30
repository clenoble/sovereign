# Spell Check

## Purpose
Find misspelled words in the document and propose corrections. Read-only: returns a list of unknown words with offsets and suggestions; does not modify the document.

## Capabilities
- `read_document` — needs the body to scan.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `check` | Check Spelling | JSON: `{"language": "en_US"}` (optional, default `"en_US"`) | `StructuredData` |

## Sample input → output

`check` on `"The qiuck brown fox jumps over the lazi dog."` returns:
```json
{
  "misspellings": [
    {"start": 4, "end": 9, "word": "qiuck", "suggestions": ["quick", "quack", "quick"]},
    {"start": 35, "end": 39, "word": "lazi", "suggestions": ["lazy", "lazis"]}
  ],
  "count": 2,
  "language": "en_US"
}
```

## Complexity estimate
Large. Dictionary-based spellcheck needs the dictionary bundled (~1MB+ for English alone), and edit-distance suggestion ranking is non-trivial.

## Host-db extensions needed
None.

## Suggested implementation notes
- Use the `hunspell-rs` crate (bindings to Hunspell) or a pure-Rust alternative like `spellbook` (loads `.dic`/`.aff` Hunspell dictionaries). Bundle the dictionary or have the user supply it via a documented filesystem path.
- For multilingual support, accept a `language` param (`en_US`, `fr_FR`, `de_DE`, etc.) and bundle dictionaries lazily.
- Skip code blocks, URLs, email addresses, and inline code spans (use the markdown stripper from Readability Score / Grammar Check).
- Skip tokens that look like proper nouns (capitalized mid-sentence) — high false-positive rate otherwise.
- Maintain a per-user word allowlist (read from the user profile dir) so additions persist across sessions. Adds a minor host extension if you want to expose "Add to dictionary" — see Suggested implementation extension below.

## Suggested implementation extension
For "Add to dictionary" UX, host needs a small key-value store accessor that's not currently in `SkillDbAccess`. Could be implemented via a dedicated file in the user profile dir (no host extension needed); skill reads/writes it directly if `read_filesystem` + `write_filesystem` are granted.
