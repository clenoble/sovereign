# Slides Export

## Purpose
Convert a markdown document into an HTML slide deck. Each `##` heading (or each `---` horizontal rule, configurable) becomes a new slide. Useful for turning a meeting agenda or talk outline into a presentable deck without leaving Sovereign GE.

## Capabilities
- `read_document` — needs the body to convert.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `export` | Export as HTML Slides | JSON: `{"split_on": "h2", "theme": "default"}` (`split_on` ∈ {`h1`, `h2`, `hr`}, default `h2`; `theme` ∈ {`default`, `dark`, `solarized`}, default `default`) | `File` |

## Sample input → output

`export` (default params) on:
```
# Talk Title

## Slide 1
Content for slide 1.

## Slide 2
- bullet
- bullet
```

returns a single-file `talk.html` containing a deck (using the chosen JS slide framework) with two slides.

## Complexity estimate
Medium. The slide framework (reveal.js or similar) is bundled into the output; the markdown→HTML rendering reuses the existing HTML Export logic.

## Host-db extensions needed
None.

## Suggested implementation notes
- Use `reveal.js` as the slide framework — most mature, MIT license, can be inlined into a single HTML file. Bundle the minified JS + a base CSS theme.
- Splitting:
  - `split_on: "h1"` — each `# Heading` starts a new slide.
  - `split_on: "h2"` — each `## Heading` starts a new slide. Sensible default.
  - `split_on: "hr"` — each `---` (horizontal rule) starts a new slide. Lets the author control it explicitly.
- Each slide's content is the markdown between split points, rendered to HTML by reusing the markdown→HTML pipeline.
- For nested presentations (vertical slides in reveal.js), `### Heading` could indent under the current `##`; gate this behind a `nested: true` param to avoid surprising users.
- Output: single self-contained `.html` file with all CSS/JS inlined. ~200KB minimum due to reveal.js bundle.
- Output filename: `<sanitized title>.html`.
