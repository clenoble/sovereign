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
- Limit parallel compilation to avoid OOM-crashing WSL: use `cargo test -j 2` (safe) or `-j 4` (faster, test if stable)

## User Confirmation Required
- When a problem can be solved either by installing a missing system package or by changing the code, **ask the user** which approach they prefer before proceeding
- Never run `sudo` commands to install packages without explicit user approval

## Git & NAS Push/Merge Workflow

The bare repo lives on the NAS. From WSL:

```bash
# 1. Mount the NAS (if not already mounted — requires sudo, ask user)
sudo mount -t drvfs 'Z:' /mnt/nas

# 2. Ensure git trusts the NAS path (one-time)
git config --global --add safe.directory '/mnt/nas/03 - user-centered OS'
git config --global --add safe.directory '/mnt/nas/03 - user-centered OS/.git'

# 3. Set remote to WSL-accessible path (if still set to Z:\)
git remote set-url origin '/mnt/nas/03 - user-centered OS'

# 4. Push
git push origin main
```

The remote URL is stored as a WSL path (`/mnt/nas/03 - user-centered OS`), not the Windows `Z:\` path, because WSL git cannot resolve Windows drive letters.

To pull updates back to the working copy after editing on the NAS side:
```bash
git pull origin main
```

## Code Style
- Rust: edition 2021, prefer safe code, minimize unsafe blocks
- Keep spike code simple and focused — no over-engineering
- Comments only where logic isn't self-evident
