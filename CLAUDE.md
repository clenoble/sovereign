# Sovereign OS — Project Instructions

## Library Version Rule
Before writing or modifying any code that uses an external library (crate, pip package, npm module, etc.):
1. Check the **current latest version** on crates.io / PyPI / npm (use WebSearch or WebFetch)
2. Fetch the **latest API documentation** for that version — do NOT rely on memorized APIs from training data
3. Verify method signatures, constructors, and imports against the actual docs before writing code
4. If docs.rs fails to build for a crate, check the project's own hosted docs or GitHub source

This is critical — APIs change between versions and stale knowledge causes cascading build failures.

## Build & Development
- Platform: Windows host, code runs in WSL2/Linux
- Source lives on NAS mount (`/mnt/nas/Current/Projets/03 - user-centered OS/`)
- Copy to WSL native filesystem (`~/`) before building for performance
- Always `rm -rf` target directory before `cp -r` (cp into existing dir nests instead of overwriting)
- Rust linker is rust-lld — be aware of `--as-needed` link ordering issues

## Code Style
- Rust: edition 2021, prefer safe code, minimize unsafe blocks
- Keep spike code simple and focused — no over-engineering
- Comments only where logic isn't self-evident
