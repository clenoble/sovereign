# v0.0.4 — PII management dashboard, canvas usability pass

This release ships the PII management dashboard — a native PII vault, sharing ledger, and entity-centric organization on top of Sovereign's encrypted graph — together with substantial canvas improvements (sticky date ticks, zoom up to 10-minute intervals, visual size cap on cards and message bubbles, redesigned scroll/zoom controls) and a set of orchestrator commands that toggle UI panels by voice or chat.

## Highlights

- **PII management dashboard.** A three-column dashboard organizing PII per **business entity** (kind: self / org / person / service). Left column lists entities with per-entity record counts and unreviewed badges. Center column shows four tabs per entity: **Inventory** (discovered findings, confirm or dismiss), **Vault** (user-stored secrets — passwords, API tokens, bank accounts, document IDs, secure notes — encrypted under the device key), **Shared** (the disclosure ledger of what PII you've sent to that entity, when, and through which channel), and **Cookies** (entity-domain cookies in the embedded browser, listed with reveal/copy/delete + bulk clear). Right column is the global review queue across all entities. Press `P` or click the shield icon in the taskbar to open it.
- **Browser-PII flows.** The embedded browser now scans login forms and offers two complementary flows: **Save credentials** (signup-capture: extract fields, edit labels, optionally generate a password, commit as a vault entry tied to the page's host as a new or existing entity, with a Web-channel ShareRecord per field) and **Fill from vault** (autofill: match the page's host against entity domains with proper subdomain handling, list candidate vault entries, decrypt and inject under L3 confirmation).
- **Sticky date ticks on the canvas.** Tick labels and the "Now" marker render in screen space at the top of the canvas — they no longer scroll off when you pan or zoom in Y. A 24px semi-transparent strip keeps them legible over busy content.
- **Zoom to minute intervals.** `ZOOM_MAX` raised from 5 to 20 so the timeline can compress to 10-minute ticks. Past zoom 1.5× (`MAX_VISUAL_ZOOM`), card and message-bubble visual sizes cap so the time axis can keep compressing without elements taking over the viewport.
- **Wheel remap.** Plain wheel pans vertically; `Shift+wheel` pans horizontally; `Ctrl/Meta/Alt+wheel` zooms (Ctrl is the standard canvas pinch-to-zoom emulation). Trackpad horizontal scroll is honored without a modifier.
- **Orchestrator panel commands.** Five new actions wired through the LLM router: `open the PII dashboard`, `open the model panel`, `open inbox`, `open the browser` / `browse`, `open settings`. A post-classification override catches the common case where the router maps these phrases to action="open" — the panel-toggle vocabulary doesn't need to be in the prompt yet.

## PII pipeline (under the hood)

- **Greenfield-native PII storage.** PII is stored as encrypted `PiiRecord`s keyed to an entity. Document and message bodies are stored as canonical text with `[pii:<record_id>]` tokens; the original PII-inline body is encrypted into `body_raw_encrypted` / `body_raw_nonce`. The resolution API expands tokens at one of four access levels: `preview`, `masked_sample`, `reveal`, `raw_original`. Reveal bumps `last_revealed_at` server-side.
- **Detection pipeline.** Hybrid regex + LLM-NER classifier with confidence scoring. Records below threshold land as `Unreviewed`; the dashboard's right-column queue lets the user confirm or dismiss. The PII detector skill exposes the pipeline as a regular skill so it can run on documents on demand.
- **Sharing ledger.** Every outbound disclosure (Web form submit, email send) writes a `ShareRecord` linking the `PiiRecord` to the recipient `Entity` with channel + timestamp. The Shared tab on each entity surfaces this as "you sent <kind> on <date> via <channel>".
- **Vault entries** are `stored_secret = true` records with `confidence = 1.0` and `review_state = Confirmed`, encrypted under the device key via `EncryptedBlob::encrypt_str`. `create_vault_entry` is L3 (Modify) and gated behind the encryption feature.
- **Cookies tab** uses the `cookie_api` module to list/delete cookies attributable to an entity via its `domains[]`. Bulk clear returns the count actually removed (L5 Destruct).
- **Password generator** (`password_gen` module) — configurable policy (length, character classes, ambiguous-char exclusion). Default is 24 chars with all classes minus ambiguous.
- **Accessibility (step 9).** Focus trap action applied to every floating panel: PII dashboard, Inbox, Contact, Model, Skills, Settings, ConfirmAction. Tab cycles within the panel; Escape closes; the previously-focused element gets focus back on close. `P` keyboard shortcut to toggle the dashboard.
- **Seed data.** First-launch seed creates 5 PII entities (one of each `EntityKind`), 7 discovered records spanning all three review states, 5 vault entries (encrypted under the device key for a real reveal round-trip), 3 ShareRecords on the disclosure ledger, and 2 PII-bearing documents whose canonical body carries `[pii:<id>]` tokens with the original PII text encrypted in `body_raw_encrypted`. Lets the dashboard be exercised end-to-end from a fresh DB without going through every flow manually.

## Canvas (continued)

- **Visual size cap on cards.** `CanvasCard.svelte` derives `cardScale = MAX_VISUAL_ZOOM / zoom` past the threshold and applies it as `transform: scale(...)` from the top-left, anchored to world position. Cards stop growing at ~300×120 px on screen.
- **Visual size cap on message bubbles.** Radius, border `lineWidth`, and click hit-test all multiply by `MAX_VISUAL_ZOOM / zoom` past the threshold.
- **Relationship arc fix.** Relationship lines anchor to the *visual* card center (CARD_W/2, CARD_H/2) × `cardScale` so arcs re-attach to capped cards at extreme zoom. Curve "lift" of 30 and `lineWidth` scale by the same factor.
- **Spread seeded message timestamps.** Each conversation now starts ~73 minutes after the previous one and every message gets a per-message seconds jitter, so the message circles no longer pile up at one minute boundary when zoomed in to hour or minute level.
- **+ button on canvas toolbar fixed.** `handleCanvasPointerDown` now skips drag start when the click target is in `.canvas-toolbar` / `.new-thread-popup` / any interactive element. Same `setPointerCapture` ate-the-click bug fixed for the floating panels — this just extends the skip list.

## Floating panels

- **Close-button bug fixed across `PiiDashboardPanel`, `InboxPanel`, and `ContactPanel`.** The draggable header's `pointerdown` handler called `setPointerCapture` on the header div, which stole subsequent click events from descendant buttons (including the ×). The drag start now skips when the target is inside an interactive element (`button, input, select, textarea, a`).

## Frontend testing

- **First component test suites for the PII dialogs.** 9 tests for `VaultAddDialog`, 10 for `AutofillPrompt`, 9 for `SignupCapturePrompt` — covering render gating, form validation, generate-password, commit happy path, error handling, and (for autofill) domain-matching including a suffix-attack guard ("ample.com" must not match "example.com").
- **PII store tests.** 33 tests for `pii.svelte.ts` covering all 11 exported helpers — load/refresh, all filters (`recordsForEntity`, `recordsByState`, `inventoryForEntity`, `vaultForEntity`, `piiCountForEntity`, `unreviewedCount`, `kindForRecordId`), cache semantics (`shareRecordsByEntity` and `cookiesByEntity` — load is idempotent on cache hit, refresh always re-fetches), and error-swallowing.
- **6 new override + 5 heuristic parser tests** for the orchestrator panel-toggle actions, including a guard that "show models" still routes to `list_models`.
- **2 new backend seed tests** for `seed_pii_if_empty` verifying entity / record counts, all review states represented, decrypt round-trip, and idempotency.
- **Total:** 149 frontend tests pass (88 pre-existing + 61 new); seed module test count up to 4 (2 pre-existing + 2 new).

## Browser

- **DuckDuckGo as homepage.** Replaces `google.com` everywhere it was referenced (`events.ts`, `BrowserPanel.svelte` URL bar default + search-query URL, `Taskbar.svelte`, `+page.svelte`).

## Documentation

- `RELEASE_NOTES_v0.0.3.md` brought into the repo root (was created during the v0.0.3 release pass).
- `doc/plans/canvas-zoom-density.md` captures the open issue with the canvas (lane height still scales with zoom even though cards and message bubbles cap) and the design for **Option B+** (non-uniform parent scale: time axis uncapped, vertical capped) plus **Option C** (user-configurable density via settings panel).
- `doc/plans/pii-management-dashboard.md` updated to mark all 9 roadmap steps shipped.
- `README.md` and `CONTRIBUTING.md` refreshed during v0.0.3 — no further updates needed for v0.0.4.

## Build / packaging

No changes to the release-build path. `_release_build.bat` and the same feature set as v0.0.3 (`cuda,encryption,p2p,comms-email,web-browse`) produce the binary. CUDA 13 runtime DLLs (`cudart64_13.dll`, `cublas64_13.dll`, `cublasLt64_13.dll`) still need to be on PATH or bundled with the exe.

## Not in this release

- **Canvas lane-height cap.** Lanes still scale with zoom — at extreme zoom only 1–2 lanes fit on screen. Design in `doc/plans/canvas-zoom-density.md`. Targeted for a follow-up.
- **`comms-signal`** still excluded from the release build (known build issue carried over from v0.0.3).
- **User-configurable canvas density / wheel bindings.** Hardcoded for now — settings panel surface is a follow-up.
- **`PiiDashboardPanel.svelte` component test.** The dashboard panel itself isn't unit-tested (its drag/select/tab interactions are complex enough to push to E2E). Store + sub-dialogs cover the bulk of the logic.

## Upgrading from v0.0.3

- **Onboarding required for the PII features.** The vault and the PII pipeline both need a loaded `DeviceKey`, which is set up during onboarding. If you skipped onboarding on a v0.0.3 install, run it again. The dashboard renders fine without a device key (you can review entities and see seeded discoveries) but vault writes and reveal will fail with a clear error.
- **Existing DB is preserved.** `seed_pii_if_empty` only seeds PII data on a database with no entities. If you've already created entities (or seeded on a v0.0.4-pre build), the seed is a no-op.
- The `EncryptedGraphDB` decorator now has 8 additional PII-related methods. No data migration; the new methods only fire when the encryption feature is on and the calling code targets PII tables.
