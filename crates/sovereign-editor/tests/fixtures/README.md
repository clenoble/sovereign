# Test fixtures

## Cantarell-Regular.ttf

- **What:** Cantarell Regular, version 0.05 (31,236 bytes,
  SHA-256 `d15f25bcdaba2e25f54c32d4e29e68fedeb1b7a1d8fbc83f7c4475916e93b55f`).
- **Copyright:** Copyright (c) 2009-2011, Understanding Limited
  (dave@understandinglimited.com), Copyright (c) 2010-2011, Jakub Steiner
  (jimmac@gmail.com) — as stated in the font's own `name` table.
- **Licence:** SIL Open Font License, Version 1.1 (stated in the font's `name`
  table, IDs 13 and 14; full text in `OFL.txt` next to this file). Bundling the
  font unmodified with software is permitted by the OFL. The font file does not
  embed the licence text itself (only the notice and the URL); `OFL.txt` was
  written from the standard OFL 1.1 text — compare it with
  https://openfontlicense.org before this crate is ever published.
- **Where it came from:** the copy shipped inside the `sctk-adwaita` 0.10.1
  crate (`src/title/Cantarell-Regular.ttf`), already in this workspace's cargo
  registry. Unmodified.
- **Why:** caret geometry (guarantee 5's "correct visual caret", guarantee 6's
  caret area) must not depend on the fonts of the machine running the tests.
  The tests open the editor with `Fonts::Only([this font])`.
- **Limit:** Latin only. Hebrew, Arabic, CJK and emoji lay out with the font's
  `.notdef` glyph. That glyph has a positive advance, so the tests' geometric
  claims (ordering of caret x positions in left-to-right and right-to-left
  runs) hold; they say nothing about shaping quality, ligatures or glyph
  coverage.
