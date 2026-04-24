# PII Management & Dashboard

## Context

The Sovereign GE roadmap lists "PII dashboard" as a not-yet-built feature. The user's framing: **a cross between KeePass (user-held secrets per entity) and a cookie manager (visualize personal data exposure)**, organized per business entity, producing a single view of what PII exists locally and where it has been shared.

Decisive constraint from user: there are no real users or data yet, so the feature is designed **greenfield, not as a migration**. PII must be **handled natively in the vault and ledger from day one — no stray PII**. This promotes vault and ledger from "later waves" to first-class storage destinations that the ingest pipeline writes into on every document/message/contact touch. The dashboard is then a view over these structures, not a standalone side-project.

Intended outcome: at-rest, the Sovereign graph contains **references** to PII records, never raw values in unmanaged locations. Raw values live in encrypted PII records keyed to an entity. Rendering resolves references to display form under Action Gravity gates. The dashboard shows the entity axis, the inventory, the vault, and the sharing ledger as one navigable surface.

## Architectural decision: PII-by-reference with dual-encrypted storage

At ingest, text is scanned for PII. Each finding produces a `PiiRecord` encrypted under device key with metadata (kind, entity_id, discovered_at, source_ref). The source text is stored as two fields:

- **`body_canonical`** — with PII spans replaced by reference tokens: `[pii:<record_id>]`. This is what search, logs, AI context, exports use by default.
- **`body_raw_encrypted`** — full original text, encrypted at rest. Reveal is Level 3 (gated) and used for edit-original or emergency recovery.

This dual-storage prevents irreversible mangling from LLM-NER false positives while still keeping the default surface PII-free. Editing flows operate on `body_canonical`; if the user needs to edit underlying raw text (rare), they unlock the raw copy via Action Gravity confirmation.

The three pillars materialize as:

- **Inventory** = the set of `PiiRecord`s, joined to their `source_ref`s, grouped by `entity_id`.
- **Vault** = `PiiRecord`s with a `stored_secret: bool = true` flag (user-entered credentials, tokens, bank accounts — KeePass role). Same table, an axis of distinction.
- **Ledger** = `ShareRecord` edges: `PiiRecord → Entity` via `Message` at `shared_at`, written synchronously when outbound messages are sent.

One schema, three views.

## Data model additions

New schemas in [crates/sovereign-db/src/schema.rs](../../crates/sovereign-db/src/schema.rs):

```
Entity {
    id, name, kind (Org | Person | Service | Self),
    domains: Vec<String>,           // e.g. ["acme.com", "acme.ch"]
    contact_ids: Vec<String>,       // linked Contact nodes
    notes, created_at, modified_at, is_owned
}

PiiRecord {
    id,
    kind (Email | Phone | SSN | CreditCard | IPv4 | AVS | IBAN |
          Passport | DOB | Address | PersonName | OrgName | Other),
    value_encrypted: Vec<u8>,       // nonce + ciphertext
    value_nonce: [u8; 24],
    entity_id: Option<String>,      // the entity this PII belongs to (often Self)
    stored_secret: bool,            // true = vault entry (user-entered), false = discovered
    confidence: f32,                // 1.0 for regex, 0.0-1.0 for LLM-NER
    sources: Vec<SourceRef>,        // all places this record is referenced
    discovered_at, last_revealed_at: Option<DateTime<Utc>>,
    review_state: Unreviewed | Confirmed | Dismissed,
}

SourceRef { source_kind (Document | Message | Contact), source_id, span_start, span_end }

ShareRecord {
    id,
    pii_record_id, to_entity_id, via_message_id,
    shared_at, channel (Email | Signal | WhatsApp | SMS | Other),
    direction: Outbound,            // ledger only tracks outbound
}
```

Mutations to existing schemas:

- `Document`: add `body_canonical: String` (keep existing `content.body` as the canonical field — i.e. rename existing `body` to `body_canonical` in semantics), add `body_raw_encrypted: Option<EncryptedBlob>`. Existing dev data is disposable per user confirmation.
- `Message`: same split — `body_canonical` + `body_raw_encrypted`.
- `Contact`: `entity_id: Option<String>` (which entity this contact belongs to); `ChannelAddress.address` stays raw for now (addresses ARE the identifier) but a parallel `PiiRecord` is created at ingest.

## Detection pipeline

New module `crates/sovereign-ai/src/pii/` with stages:

1. **Regex layer** (deterministic, synchronous). Extend [crates/sovereign-skills/src/skills/pii_detector.rs](../../crates/sovereign-skills/src/skills/pii_detector.rs) with:
   - Swiss AVS number (format `756.XXXX.XXXX.XX`)
   - IBAN (ISO 13616 including Swiss CH-prefix)
   - Passport pattern (9 alphanumeric, ISO 3166 country prefix)
   - ISO date of birth (`YYYY-MM-DD` in proximity of "DOB"/"né(e)"/"birth" keywords)
   - Swiss postal address pattern (street + 4-digit postcode + city)

   The existing `detect(text)` signature is preserved for backward compat with `RedactorSkill`; new extended function `detect_extended(text, locale)` returns a superset.

2. **LLM-NER layer** (escalation, 7B reasoning model, async). Runs when regex pass completes; receives the text with regex spans already excised. Extracts person names, organization names, free-form addresses. Uses the existing `llm/` orchestrator with a zero-shot NER prompt. Returns spans with confidence scores. Below a `confidence_threshold` (default 0.7), findings are marked `Unreviewed` and deferred to the review queue rather than committed.

3. **Entity disambiguation**. For each finding, attempt to auto-link to an `Entity`:
   - Email address → extract domain → match existing Entity by `domains[]`
   - Phone number → match Contact by `ChannelAddress`
   - Person/org name → fuzzy match against existing Entity names
   - No match → propose new Entity, queued for user confirmation

4. **Commit pass**. Write the `PiiRecord`(s), rewrite source text to `body_canonical` with `[pii:<record_id>]` tokens, encrypt raw original, write `SourceRef` back-links. If any finding is below threshold and no prior-user-seen marker exists, the commit writes the canonical form but stages the tokenization as **reversible pending review** — the source stays fully readable until the user confirms.

## Ingest integration (the "no stray PII" invariant)

Hook the pipeline into every content-creation path:

- `Document` create/update — [crates/sovereign-app/src/tauri_commands.rs](../../crates/sovereign-app/src/tauri_commands.rs) document commands
- Message ingest — [crates/sovereign-comms/](../../crates/sovereign-comms/) email IMAP fetch, Signal `presage` handler, WhatsApp webhook
- Contact import — wherever contacts are bulk-loaded
- Session log — user-typed chat to the AI (important: AI context should receive canonical form, not raw)

Idle sweep (leveraging `sovereign-ai/src/consolidation.rs` pattern) rescans any content missing a `pii_scanned_at` marker, useful for re-scans when taxonomy changes.

## Render / resolution path

New Tauri command `resolve_pii_tokens(doc_id, access_level) -> ResolvedBody`:

- `access_level: Preview` → renders tokens as `[Email]`, `[Phone]`, etc. (type only). L1 Observe.
- `access_level: MaskedSample` → renders tokens as masked form (`a***e@e***e.com`). L1 Observe.
- `access_level: Reveal` → renders full original from `value_encrypted`. L3 Modify — records `last_revealed_at`, surfaced in UI as a visible state change.
- `access_level: RawOriginal` → decrypts `body_raw_encrypted`. L3 Modify, confirmation required each call.

The Svelte `marked` + `DOMPurify` pipeline in [frontend/src/lib/utils/markdown.ts](../../frontend/src/lib/utils/markdown.ts) is extended to accept pre-resolved body (default) or to render tokens inline as styled chips on hover/click (dashboard context).

Tokens in AI context: the chat agent loop in [crates/sovereign-ai/src/orchestrator.rs](../../crates/sovereign-ai/src/orchestrator.rs) always receives `body_canonical`. If the user asks the AI to act on a PII-containing doc, the AI sees tokens, not values. Tools that need the value (e.g., `send_email` that must include a credit card) call `resolve_pii_tokens` with Action Gravity gating.

## Action Gravity mapping

Extend [crates/sovereign-core/src/security.rs](../../crates/sovereign-core/src/security.rs) minimally — no new levels, new action kinds:

| Action | Level | Notes |
|---|---|---|
| View dashboard, list entities, list findings (masked) | Observe (1) | Default panel state |
| Run manual rescan | Observe (1) | Read-only operation |
| Tag / categorize finding | Annotate (2) | E.g., "this AVS is a duplicate" |
| Confirm new entity, merge entities | Annotate (2) | Graph structure edits |
| Reveal PII value (unmask) | Modify (3) | Records `last_revealed_at`; requires confirmation |
| Add / edit vault entry | Modify (3) | State transition |
| Copy-to-clipboard | Modify (3) | Triggers 30s auto-clear |
| Redact / purge finding | Destruct (5) | Irreversibly removes from `body_canonical`, deletes record |
| Export dashboard audit | Transmit (4) | Per existing precedent |

Plane enforcement via `action_gate.rs`: any PII operation originating from Data-plane content (e.g., an email that contains `"please reveal my password"`) is automatically blocked at L3+, as it already is for other actions.

## Dashboard UI

New floating panel `PiiDashboardPanel.svelte` under [frontend/src/lib/components/](../../frontend/src/lib/components/), lifecycle modeled on `InboxPanel.svelte`:

- Toggle via `app.piiDashboardVisible` in [frontend/src/lib/stores/app.svelte.ts](../../frontend/src/lib/stores/app.svelte.ts)
- Taskbar entry in [frontend/src/lib/components/Taskbar.svelte](../../frontend/src/lib/components/Taskbar.svelte) with keyboard shortcut (e.g. `P`)
- Accessibility: **fix existing panel gaps** — add `role="dialog"`, `aria-modal`, focus trap, escape handler. Set a new-panel precedent other panels can adopt.

Layout (three-column):

1. **Entity list** (left) — scrollable, entities sorted by PII count descending, grouped by `kind`. Provenance cue: shape differentiation (rounded rectangle for `Self`, parallelogram for external — extending the Sovereignty Halo convention).
2. **Entity detail** (center) — tabs: *Inventory* (discovered PII), *Vault* (stored secrets), *Shared* (ledger entries for this entity). Each row has masked value by default, reveal button (L3), copy button (L3), redact/purge button (L5).
3. **Review queue** (right, collapsible) — `Unreviewed` findings awaiting user confirmation. One-click confirm-entity or dismiss.

New store `pii.svelte.ts` mirrors `contacts.svelte.ts` shape with filter/group helpers.

## Vault add path

Separate "New secret" action opens a form:

- Entity selector (auto-complete over existing entities, create-new inline)
- Kind (Password | APIToken | Note | BankAccount | DocumentID | Other)
- Label, Value (masked input)
- Optional tags

Creates a `PiiRecord` with `stored_secret=true, review_state=Confirmed, sources=[]`. Encryption under device key. Integrates into the same inventory tab; vault entries are visually distinct (a "stored by you" shape/icon).

## Ledger write path (synchronous on outbound send)

When the user sends an outbound message via any `sovereign-comms` channel:

1. Scan outgoing `body_canonical` for `[pii:<id>]` tokens and any newly-detected values.
2. For each referenced `PiiRecord`, write a `ShareRecord { pii_record_id, to_entity_id: <recipient>, via_message_id, shared_at, channel }`.
3. User confirmation surfaces in the send-confirmation UI ("This message contains 2 PII items that will be logged to your sharing ledger"). Action level: L4 Transmit, already gated.

Historical messages are not retroactively scanned (per user: no real data yet).

## Critical files

New:
- [crates/sovereign-crypto/src/vault.rs](../../crates/sovereign-crypto/src/vault.rs) — dual-encrypted blob primitive (mirrors `key_db.rs` pattern)
- [crates/sovereign-ai/src/pii/mod.rs](../../crates/sovereign-ai/src/pii/mod.rs) + `regex.rs`, `ner.rs`, `entity_disambig.rs`, `pipeline.rs`
- [crates/sovereign-core/src/entity.rs](../../crates/sovereign-core/src/entity.rs) — `Entity`, `PiiRecord`, `ShareRecord` shared types
- [frontend/src/lib/components/PiiDashboardPanel.svelte](../../frontend/src/lib/components/PiiDashboardPanel.svelte)
- [frontend/src/lib/components/EntityListItem.svelte](../../frontend/src/lib/components/EntityListItem.svelte), `PiiRow.svelte`, `VaultAddDialog.svelte`, `ReviewQueueItem.svelte`
- [frontend/src/lib/stores/pii.svelte.ts](../../frontend/src/lib/stores/pii.svelte.ts)

Modified:
- [crates/sovereign-db/src/schema.rs](../../crates/sovereign-db/src/schema.rs) — new schemas + `body_canonical` / `body_raw_encrypted` on `Document`, `Message`; `entity_id` on `Contact`
- [crates/sovereign-skills/src/skills/pii_detector.rs](../../crates/sovereign-skills/src/skills/pii_detector.rs) — Swiss/EU regex additions, new `detect_extended`
- [crates/sovereign-skills/src/skills/redactor.rs](../../crates/sovereign-skills/src/skills/redactor.rs) — consume `detect_extended`
- [crates/sovereign-core/src/security.rs](../../crates/sovereign-core/src/security.rs) — new action kinds (reveal_pii, redact_pii, vault_add, etc.) mapped to existing levels
- [crates/sovereign-app/src/tauri_commands.rs](../../crates/sovereign-app/src/tauri_commands.rs) — new commands: `list_entities`, `list_pii_records`, `resolve_pii_tokens`, `create_vault_entry`, `reveal_pii`, `redact_pii`, `purge_vault_entry`, `list_review_queue`, `confirm_entity`
- [crates/sovereign-comms/src/](../../crates/sovereign-comms/src/) — ingest hooks on inbound; ledger write on outbound
- [frontend/src/lib/stores/app.svelte.ts](../../frontend/src/lib/stores/app.svelte.ts) — `piiDashboardVisible`
- [frontend/src/lib/components/Taskbar.svelte](../../frontend/src/lib/components/Taskbar.svelte) — entry point
- [frontend/src/lib/api/commands.ts](../../frontend/src/lib/api/commands.ts) — wrappers
- [CLAUDE.md](../../CLAUDE.md) — architecture notes under Orchestrator + new UX principle: "PII-by-reference: canonical bodies hold tokens, not raw values"

## Implementation order (not phased releases — sequential dependencies inside one feature)

1. Data model — schemas in `sovereign-db`, shared types in `sovereign-core`, migration/seed scripts.
2. Crypto `vault.rs` primitive — round-trip encrypt/decrypt of blobs, unit tests.
3. Detection pipeline — regex extensions + tests, LLM-NER integration + tests, entity disambiguation + tests.
4. Ingest hooks — wire into document/message/contact creation paths. Session log integration last.
5. Resolution API — `resolve_pii_tokens` Tauri command, action-gate integration.
6. Dashboard UI — panel shell, stores, taskbar entry, three-column layout, add-secret dialog, review queue.
7. Ledger write path — outbound hooks in `sovereign-comms`, UI confirmation surface.
8. Accessibility audit — set the new panel as the reference implementation for ARIA + focus trap, backport pattern to other panels as follow-up.

## Verification

Unit:

- Regex tests for each new kind (AVS, IBAN, passport, DOB, address) — positive + negative cases.
- Vault round-trip: encrypt → persist → app restart → decrypt → compare.
- Entity disambiguation: deterministic cases (domain match) and fuzzy-name edge cases.
- LLM-NER: snapshot tests over canned Swiss/French/English text samples for stable confidence scoring.
- Action gate: attempt reveal from Data-plane content, assert block; attempt from Control-plane, assert confirmation path.

Integration:

- Document create with mixed PII → assert `body_canonical` contains only tokens, `body_raw_encrypted` decrypts to original, `PiiRecord`s written with correct entity linkage.
- Outbound message send containing `[pii:*]` tokens → assert `ShareRecord` rows appear in ledger.
- AI orchestrator chat referencing a PII-containing doc → assert the model receives `body_canonical` only.

E2E (manual, via the Tauri app):

- Build + launch sovereign.exe with `--features tauri-ui`.
- Create a document with email, phone, Swiss IBAN, a person name. Verify dashboard shows 4 findings, correctly grouped by entity.
- Add a vault entry (bank password for Credit Suisse). Reveal, confirm Action Gravity dialog, copy, verify 30s clipboard clear.
- Send an email containing a vault-referenced token. Verify ledger entry appears under that entity.
- Keyboard navigation through dashboard: tab order, escape closes, focus returns to taskbar.

## Open items for future consideration (out of scope for this plan)

- Browser auto-fill integration via embedded Tauri webview (KeePass-style).
- OTP generation / TOTP codes for 2FA secrets.
- Per-entity risk scoring based on PII volume + share frequency.
- Audit log hash-chain for dashboard operations (tied to the broader `audit log hash chain` roadmap item).
- Cross-device sync of vault via `sovereign-p2p` (requires mobile client).
