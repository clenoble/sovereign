# Meeting Notes Template

## Purpose
Create a date-stamped meeting-notes document pre-filled with the standard sections (title, attendees, agenda, decisions, action items, next steps). Reduces friction for users who take meeting notes regularly and want consistent shape across them.

## Capabilities
- `write_all_documents` — creates a new document via `create_document` host call.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `new` | New Meeting Notes | `""` (or JSON: `{"title":"...", "thread_id":"..."}`) | `StructuredData` |

## Sample input → output

`new` (no params) creates a document titled `Meeting — YYYY-MM-DD HH:MM` in a thread named "Meetings" (auto-created if absent), with body:

```
# Meeting — 2026-04-24 14:00

## Attendees
- 

## Agenda
1. 

## Discussion

## Decisions
- 

## Action items
- [ ] 

## Next steps

```

Returns `StructuredData(meeting_template)` with `{"doc_id": "document:...", "thread_id": "thread:...", "created": true}`.

## Complexity estimate
Small.

## Host-db extensions needed
None — uses existing `find_or_create_thread` and `create_document` from `SkillDbAccess`. Mirror the Daily Journal pattern.

## Suggested implementation notes
- Default thread name "Meetings". Allow override via params.
- Default title format is `Meeting — YYYY-MM-DD HH:MM` in the user's local time. UTC is also acceptable; document the choice.
- For params-based customization, accept JSON: `{"title": "...", "thread_id": "...", "attendees": ["Alice", "Bob"], "agenda": ["item1", "item2"]}`. Pre-fill the relevant sections from those fields.
- This is essentially a parameterization of the Daily Journal pattern. If Template Stamp lands first, Meeting Notes can be implemented as a Template Stamp invocation with a built-in template.
