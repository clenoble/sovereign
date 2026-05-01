# Contributing to Sovereign GE

Thank you for your interest in contributing to Sovereign GE.

This document describes the contribution process. It is intentionally short. Detailed legal terms are in [CLA.md](CLA.md).

## Before you contribute

**Read the CLA.** All contributions to Sovereign GE are subject to the [Contributor License Agreement](CLA.md). The CLA does not require you to assign your copyright. You retain authorship; you grant a broad license that allows the Project to redistribute your work under AGPL-3.0 or a compatible free license.

The CLA exists for two reasons:
1. To make sure the Project can continue to be redistributed under a free license (and only under a free license) without ambiguity.
2. To allow the eventual transfer of rights to the m4d Foundation, which will be the long-term steward of the Project.

If you have questions about the CLA before contributing, open a discussion in the repository or contact the maintainer directly.

## How to contribute

### Code contributions

1. **Fork the repository** and create a feature branch from `main`.
2. **Make your changes.** Follow the coding conventions visible in the existing code. If you are unsure, ask before investing significant effort.
3. **Test your changes.** Run the test suite and add new tests for new behavior where appropriate.
4. **Open a pull request** against `main`.
5. **CLA Assistant** will automatically check whether you have signed the CLA. If not, it will post a link in the pull request — follow the link to sign. Signature happens once and covers all your future contributions.
6. **Respond to review.** Maintainers may ask for changes. Discussion is the norm.

### Non-code contributions

Documentation, translations, design feedback, accessibility audits, and bug reports are valued contributions. They follow the same CLA process if they involve original creative work that is added to the repository.

Bug reports that simply describe a problem (without proposing a fix) do not trigger the CLA — they are simple communications.

### What we are looking for

- **Bug fixes** — always welcome.
- **Skill development** — Sovereign's modular architecture is built around Skills. New Skills are encouraged.
- **Accessibility improvements** — Sovereign commits to WCAG 2.1 AA. Accessibility audits and fixes are particularly valued.
- **Translations** — currently focused on French and English; other languages welcome.
- **Documentation** — both user-facing and developer-facing.

### What we are not looking for

- **Cryptocurrency integration.** Out of scope by design. See the Ethics & Misuse Analysis on the project website.
- **Tor integration.** Same.
- **Telemetry, analytics, or "phone home" features.** The architectural commitment is to local-first, no-account, no-tracking. Contributions adding any form of usage tracking will be declined.
- **Permissive license re-publication.** Sovereign is and remains AGPL-3.0. Contributions whose author wishes to constrain the license are not compatible with the Project.

## Code style

Rust code follows `rustfmt` defaults. Run `cargo fmt` before submitting.

Clippy warnings should be addressed unless there is a documented reason to suppress them.

UI code (Iced, Tauri frontend) follows the conventions in `sovereign-ui` and `sovereign-canvas`.

## Reviewing process

Maintainers aim to acknowledge pull requests within one week, though detailed review may take longer for substantial changes. Sovereign is a research-grade project; we prioritize correctness and architectural coherence over speed of merging.

If your pull request is not reviewed within two weeks, ping the maintainer in a comment — sometimes notifications get lost.

## Questions

Open a discussion in the repository, or email the maintainer through the channel listed in the project README.

---

*Sovereign GE is currently maintained by Céline Lenoble, pending transfer of stewardship to the m4d Foundation. Contributions help build something durable.*
