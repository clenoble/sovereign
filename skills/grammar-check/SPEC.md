# Grammar Check

## Purpose
Surface rule-based grammar and style suggestions for the document body — passive voice, weak verbs, common confusables ("its/it's", "your/you're"), wordy phrasings. Read-only: returns a list of issues with offsets and suggestions; does not modify the document.

## Capabilities
- `read_document` — needs the body to scan.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `check` | Check Grammar | `""` | `StructuredData` |

## Sample input → output

`check` on `"Their going too the store. The report was wrote by Alice."` returns:
```json
{
  "issues": [
    {"start": 0, "end": 5, "text": "Their", "rule": "their_there_they're", "suggestion": "They're", "severity": "high"},
    {"start": 12, "end": 15, "text": "too", "rule": "too_to", "suggestion": "to", "severity": "high"},
    {"start": 32, "end": 40, "text": "was wrote", "rule": "past_participle", "suggestion": "was written", "severity": "high"}
  ],
  "count": 3
}
```

## Complexity estimate
Large. Rule-based grammar engines need substantial rule sets to be useful; the rules also have to handle markdown-aware skipping (don't flag inside code blocks, URLs, etc.).

## Host-db extensions needed
None.

## Suggested implementation notes
- Don't try to write a grammar engine from scratch. Two viable paths:
  1. Bundle a smaller rule set focused on the most common errors (~50 rules). Keeps the WASM size modest. Inspired by the `nlprule` crate or the LanguageTool rule format.
  2. Wrap an existing local engine (LanguageTool via subprocess if present on user's system; would require `network` capability since LanguageTool's HTTP API is one option).
- Strip markdown before checking (use the same approach as Readability Score).
- Severity levels: `low` (style), `medium` (questionable), `high` (likely-error).
- Mind the user's locale — English-only is OK for v1; flag in the description.
