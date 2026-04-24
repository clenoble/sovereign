# Template Stamp

## Purpose
Treat the current document as a template containing `{{variable}}` placeholders, and replace each placeholder with the value supplied via params. General-purpose template-fill primitive that other automation skills (Meeting Notes, Daily Journal variants, etc.) can build on.

## Capabilities
- `read_document` — needs the body to substitute into.
- `write_document` — returns a `ContentUpdate` with substitutions applied.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `stamp` | Fill Template | JSON: `{"vars": {"name": "Alice", "date": "2026-04-24"}}` | `ContentUpdate` |

## Sample input → output

Document body (template):
```
# {{title}}

Date: {{date}}
Author: {{name}}

Hello, {{name}}.
```

Invoked with params `{"vars": {"title": "Welcome", "date": "2026-04-24", "name": "Alice"}}`:

```
# Welcome

Date: 2026-04-24
Author: Alice

Hello, Alice.
```

## Complexity estimate
Small. Just regex replacement.

## Host-db extensions needed
None.

## Suggested implementation notes
- Placeholder syntax: `{{varname}}` (Mustache-style, common and safe). Whitespace inside the braces tolerated: `{{ varname }}`.
- Allowed variable name characters: `[a-zA-Z0-9_-]`. Reject anything else with a clear error.
- Behavior on missing variable: error by default, with a `strict: bool` param (default `true`) that can be set to `false` to leave undefined placeholders untouched.
- Built-in variables: consider auto-providing `{{today}}` (YYYY-MM-DD), `{{now}}` (RFC3339 timestamp), `{{user}}` (from user profile if available) without requiring them in `vars`.
- This is a building block. Meeting Notes Template can be implemented as Template Stamp invoked against a built-in template string.
