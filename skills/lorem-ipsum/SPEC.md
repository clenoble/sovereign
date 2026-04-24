# Lorem Ipsum

## Purpose
Insert placeholder text of configurable length, count, and unit (words, sentences, paragraphs). Useful for layout testing, sample documents, mockups.

## Capabilities
- `read_document` — needs current body to know where to append (or replace).
- `write_document` — returns a `ContentUpdate` with placeholder text added.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `insert` | Insert Lorem Ipsum | JSON: `{"unit": "paragraphs", "count": 3}` (`unit` ∈ {`words`, `sentences`, `paragraphs`}; default 3 paragraphs) | `ContentUpdate` |
| `replace` | Replace with Lorem Ipsum | Same params | `ContentUpdate` |

## Sample input → output

`insert` with params `{"unit": "paragraphs", "count": 1}` appends:

```
Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor
incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis
nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.
```

Returns `ContentUpdate(<original body + lorem ipsum>)`.

## Complexity estimate
Small. Maintain a fixed corpus of Latin-ish placeholder words; sample.

## Host-db extensions needed
None.

## Suggested implementation notes
- Use the `lipsum` crate — well-tested, deterministic with seeded RNG, supports word/sentence/paragraph generation.
- `replace` mode wipes existing body. Probably want a confirmation in the UI, but that's an action-gravity concern, not the skill's.
- For honest UX, never auto-prefix with "Lorem ipsum dolor sit amet" unless the count is > 5 words — short generations should look more random.
- Optional: support `style` param (`"latin"`, `"hipster"`, `"corporate"`) if you want to be fancy. Lipsum has variants.
