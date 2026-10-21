# Sovereign GE — instructions for Claude

Minimal on purpose: what the system is lives in the spec, how to build it in
`doc/DEVELOPING.md`. Do not copy either here — a copy goes stale.

## Where the truth lives

- **`doc/spec/` is the single source of truth** for what the system is and does. Start at
  `doc/spec/README.md`. Every section carries a status line (built / partial / specified /
  conflict / retired); check it against the code before relying on it.
- **Every change to the spec gets an entry in `doc/spec/CHANGELOG.md` in the same commit**,
  naming who decided.
- **Plans say how and when, never what.** If a plan and the spec disagree, the plan is wrong.
  (2026-07-10: two plan documents quietly redefined guardian recovery; the code followed them
  and built the wrong feature, because nothing compared plans with the spec.)
- **If `doc/plans/` exists in your checkout, read `doc/plans/NODE.md` first**: how this project
  is worked on, by whom, and the rules that come with it. It is imported below; on the public
  mirror, where it is absent, the import is ignored. Open work lives in
  `doc/plans/BACKLOG.md`. `doc/plans/` is private and never published: security findings go
  there, never into `doc/spec/`, which is public.

## How work is done here

- **Decisions about values, behaviour a user sees, and risk belong to the project owner.**
  Ask, with options, their trade-offs and a recommendation. Technical choices you make and
  state. A doubt is made visible, never settled silently.
- **Contracts first, tests second, code third — by different authors.** A contract is a spec
  section with numbered guarantees, each one a test. The tests assert invariants only, never a
  desired outcome. Whoever implements may not edit the tests: a test that looks wrong is
  reported, not worked around.
- **A change is done when a second run says so.** Before a branch reaches `main`, someone other
  than its author re-runs the suites on the branch merged with current `main`.
- **Test as you go, and run the full suites before claiming green** — `cargo check` never
  compiles test targets. Commands and feature flags: `doc/DEVELOPING.md`.
- **Library versions: check, don't remember.** Before using a crate or package API, check its
  current version and documentation; APIs change between versions.
- **Install or change code?** When a problem can be solved either way, ask. Never run `sudo`
  without explicit approval.

## Code style

Rust edition 2021, safe code, comments only where the logic is not self-evident.

## The node (private checkouts only)

@doc/plans/NODE.md
