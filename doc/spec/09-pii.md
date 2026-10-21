# 09 — Personal Information (PII)

How Sovereign holds personal information: by reference, in an encrypted vault keyed to the
entity it belongs to, with a ledger of where it was disclosed. This chapter specifies the design
and then states, separately, how much of it runs today.

*Consolidates:* `doc/plans/archive/plans/pii-management-dashboard.md` (main design source,
2026-04-30); `RELEASE_NOTES_v0.0.4.md` (dashboard, vault, ledger, browser flows), `v0.0.5`
(AccountKey), `v0.0.8` (PII-001/PII-002 locale and read-time redaction), `v0.0.9` (shell
session-log parity); `doc/plans/archive/plans/v0.0.8-release-plan.md` (COMMS-001 disposition);
specification v1.3 §Core Tenets (Identity Firewall); code in `crates/sovereign-ai/src/pii/*`,
`crates/sovereign-db/src/schema.rs`, `crates/sovereign-crypto/src/{vault,password_gen}.rs`,
`crates/sovereign-app/src/{pii_*.rs,browser_pii.rs,cookie_api.rs}`,
`crates/sovereign-app/src/tauri_commands/pii.rs`, `crates/sovereign-ai/src/{tools,orchestrator}.rs`.

The Identity Firewall of v1.x — and its conflict with the ethics appendix — is in **ch. 10**,
*Identity Firewall*, because what it would act on is the browser.

*Platforms.* Since 2026-10-09 the native shell is the only desktop UI; the Tauri desktop build is
frozen and not shipped, and Tauri remains the mobile host (Android, no embedded browser). In this
chapter, *mobile* means that host; features built only in the frozen Tauri desktop are marked as
not shipped.

---

## Principle: PII by reference

**Status:** ◐ Partial — data model and resolver built; no UI renders tokens yet; enforcement at ingest is partial (see *The "no stray PII" ingest invariant*).

At rest, the graph holds **references** to PII, never raw values in unmanaged places. Each piece of
personal information lives in an encrypted `PiiRecord` attached to the entity it belongs to; the
text it came from holds a token `[pii:<record_id>]` in its place. Rendering is to resolve tokens
under Action Gravity gates; as built, no UI calls the resolver to render tokens (see *Resolution and
access levels*). Search, logs, model context and exports use the tokenized form by default.

The framing is a cross between a password manager (secrets the user holds, per entity) and a cookie
manager (seeing what personal data has been exposed, and to whom), organized per entity.

## Dual encrypted storage

**Status:** ✅ Built — (code: `crates/sovereign-db/src/schema.rs`, `crates/sovereign-crypto/src/vault.rs`, `crates/sovereign-ai/src/pii/commit.rs`).

A scanned document or message keeps two bodies:

- **Canonical body** — the ordinary content field, with each PII span replaced by `[pii:<record_id>]`.
  This is the default surface for search, logs, the model and exports. Editing operates on it.
- **Raw body** (`body_raw_encrypted` + `body_raw_nonce`) — the full original text, encrypted under the
  AccountKey (ch. 07). Revealing it is a gated operation, used to edit the original or to recover
  from a false positive.

Keeping the raw body prevents a detection false positive from irreversibly mangling the source,
while the default surface stays free of PII. `pii_scanned_at` records that a source has been scanned.

## One schema, three views

**Status:** ✅ Built — (code: `crates/sovereign-db/src/schema.rs`).

```
Entity       { name, kind: self | org | person | service, domains[], contact_ids[],
               notes, is_owned, created_at, modified_at, deleted_at }

PiiRecord    { kind, value_encrypted, value_nonce, label?, entity_id?,
               stored_secret: bool,        // true = vault entry, false = discovered
               confidence,                 // 1.0 regex · 0.0–1.0 NER · 1.0 vault
               sources: [SourceRef], discovered_at, last_revealed_at?, use_count,
               review_state: unreviewed | confirmed | dismissed, deleted_at }

SourceRef    { source_kind: document | message | contact | session_log | user_input,
               source_id, span_start, span_end }   // offsets into the canonical body

ShareRecord  { pii_record_id, to_entity_id, via_message_id?, via_url? (encrypted),
               shared_at, channel: email | sms | signal | whatsapp | matrix | phone | web | other }
```

`PiiKind` covers: email, phone, SSN, credit card, IPv4, Swiss AVS, IBAN, passport, date of birth,
address, person name, organization name, and the vault kinds password, API token, bank account,
document id, note, other. `Contact` carries an `entity_id` and its own `pii_scanned_at`.

The three views are one table set:

- **Inventory** — `PiiRecord`s with their `SourceRef`s, grouped by entity.
- **Vault** — `PiiRecord`s with `stored_secret = true` (user-entered credentials, tokens, accounts).
- **Ledger** — `ShareRecord`s: which record was disclosed to which entity, when, through which channel.
  The ledger records outbound disclosures only; PII received from someone becomes a `PiiRecord`
  attributed to the sender.

The `Self` entity is the sentinel for the user's own PII.

## Detection pipeline

**Status:** ◐ Partial — all stages are built and tested; only the regex stage runs at ingest today.

(code: `crates/sovereign-ai/src/pii/{regex,ner,entity_disambig,pipeline}.rs`;
`crates/sovereign-skills/src/skills/pii_detector.rs`)

1. **Regex** — deterministic, confidence 1.0. A generic set (email, phone, SSN, credit card, IPv4,
   IBAN, passport, date of birth near a birth keyword) plus a Swiss locale adding AVS numbers and
   Swiss postal addresses. Findings are sorted and de-overlapped.
2. **LLM-NER** — person names, organization names and free-form addresses, run on the text with the
   regex spans removed, returning spans with a confidence.
3. **Entity disambiguation** — email → domain matched against `Entity.domains`; phone → digits
   matched against contact phone addresses, inheriting the contact's entity; person/organization
   name → conservative fuzzy match on entity names. An unmatched namable finding proposes a new
   entity, queued for confirmation. Structured kinds with no owner (IBAN, AVS, passport, DOB…) are
   attributed to `Self` at commit.
4. **Review threshold** — findings below **0.7** confidence are committed as `unreviewed` and stay
   readable in the source until the user confirms; findings at or above it are tokenized.

## Commit and review

**Status:** ✅ Built — (code: `crates/sovereign-ai/src/pii/{commit,ingest}.rs`; `crates/sovereign-app/src/pii_ingest.rs`).

Commit writes the `PiiRecord`s and any proposed entities, rewrites the source to its canonical body,
encrypts the raw original, writes the `SourceRef` back-links, and stamps `pii_scanned_at`.

**Review queue** (`review_state`). Unreviewed findings and proposed entities wait for a user decision:
confirm, dismiss, or delete. The queue is available in the native shell (records only, no review
of proposed entities) and on mobile.
(code: `crates/sovereign-app/src/tauri_commands/pii.rs`; `crates/sovereign-shell/src/app.rs`)

## The "no stray PII" ingest invariant

**Status:** ◐ Partial — **the invariant is not met today.**

**Requirement.** Every content-creation path runs the pipeline before content settles: document
create, update and import; message ingest on every channel; contact creation and import; the session
log. After ingest, a body holds tokens, not raw PII values, and its raw form exists only encrypted.
For documents, **the title, captions and alt texts are ingested as well as the body** (decided
2026-10-11; ch. 03, *Document service — contract (C-DOC)*, G10), so tokens can appear in them.

**What runs today.**

- **Documents:** ingest runs **regex-only**, at the Swiss locale, when a document is saved or imported
  through the app commands — that is, on mobile only. It runs once per document: a document already
  marked scanned is not rescanned. (code: `crates/sovereign-app/src/pii_ingest.rs`,
  `crates/sovereign-app/src/tauri_commands/documents.rs`)
- **NER is not invoked at ingest.** Person names, organization names and free-form addresses are
  therefore not detected at ingest.
- **Session log:** user input and chat responses are tokenized before they are written to the
  encrypted session log, in the native shell and on mobile, at the generic locale.
  (code: `crates/sovereign-ai/src/orchestrator.rs`)
- **Toward the model:** content reaching the model through the chat agent's read tools is redacted at
  read time — tokens become type-only labels, and an unscanned body is regex-redacted first.
  (code: `crates/sovereign-ai/src/tools.rs`, `crates/sovereign-ai/src/pii/resolve.rs`)

The invariant is not yet enforced on every write path, and the read-time redaction does not cover
every path by which content can reach a model. Bodies remain field-encrypted at rest regardless
(ch. 07); what is missing is tokenization and vault completeness, not at-rest encryption.

### Idle sweep

**Status:** ○ Decided 2026-10-10, to be built.

**Decided 2026-10-10 (Céline): detect without rewriting, review in a queue, mask on the fly
until reviewed.**
- A background scan **detects and inventories** personal data in every document not yet
  scanned (proposed records), **without rewriting the document**, and marks the document as
  analysed — so it can be summarised and read by the model.
- **Until the user has reviewed a document's findings, masking happens on the fly**: when the
  model reads the document, the detected values are replaced by their type labels at read time.
- Findings wait in a **review queue that is surfaced regularly** to the user (cadence and form to
  define). Reviewing confirms or dismisses; a confirmed value is then tokenized in the stored
  body (the raw original kept encrypted, as today).
- Nothing in a document is rewritten without the user's confirmation.

Preconditions: ingest runs on every write path (shell, import, sync); an interface that shows
tokens and labels cleanly (none does today); masking at read time is the single path by which
document content reaches the model (summaries, chat tools, skills, consolidation).

The v1 design rescans, when the system is idle, any content lacking `pii_scanned_at`, and re-scans
when the detection taxonomy changes. A sweep routine exists in code but is not wired to any trigger;
it was unwired on 2026-05-03 and deferred since. It is also compiled only with the `comms` feature,
so it is absent from the mobile build. Céline decided on 2026-10-09 that what the sweep is
for — what is done with what it finds — is specified together with A5 (credentials held in the PII
manager feeding sign-in to the embedded browser). (code: `crates/sovereign-app/src/pii_sweep.rs`)

### Message, contact and share hooks

**Status:** ○ Specified, not built — the hooks exist; no running path constructs them.

`PiiMessageHook` (inbound messages), `PiiContactHook` (contact addresses and notes) and `PiiShareHook`
(outbound ledger writes) are implemented and tested. They are not constructed by any running path;
communication channels are dormant in v0.0.9 and are to be wired together with their hardening.
(code: `crates/sovereign-app/src/{pii_message_hook,pii_contact_hook,pii_share_hook}.rs`)

## Resolution and access levels

**Status:** ◐ Partial — the resolver is built and used toward the model (type-only labels); no UI renders tokens; mobile shows raw tokens in scanned documents.

(code: `crates/sovereign-ai/src/pii/resolve.rs`; `crates/sovereign-app/src/tauri_commands/pii.rs`)

A token resolves at one of four levels:

| Level | Renders | Action Gravity |
|---|---|---|
| `preview` | type only, e.g. `[Email]` | Observe (1) |
| `masked_sample` | kind-specific mask, e.g. `a***e@e***e.com` | Observe (1) |
| `reveal` | the decrypted value; sets `last_revealed_at` | Modify (3) |
| `raw_original` | the decrypted raw body | Modify (3), confirmation each call |

### Where a token resolves

**Status:** ○ Decided 2026-10-10 (Céline), not built — the resolver today resolves any token in
any text.

A token resolves **only in a document or message listed among its record's sources**. Anywhere
else, text in token form is ordinary text. Record ids are random and cannot be guessed, but they
are visible to whatever reads stored text in its tokenized form — the model, an import, a paired
device — so without this rule content written by any of them could name a real record and have it
rendered, revealed or sent from a place it never came from. One rule at the resolver covers every
write path (typing, pasting, the assistant, import, sync), which an escape in the editor did not.

Copying a token from one Sovereign document to another (ch. 12) keeps it a token: saving the
target document adds it to the record's sources. A paste from outside Sovereign never does.

Resolution is error-tolerant: an unknown or undecryptable token renders as a placeholder rather than
failing the whole body. The model always receives the canonical form; a tool that needs a value must
resolve it under the gate.

## Action Gravity mapping

**Status:** ◐ Partial — the mapping is the design; enforcement is not yet uniform across operations and UIs.

No new levels; new action kinds mapped to the existing five (ch. 08):

| Operation | Level |
|---|---|
| View dashboard, list entities and masked findings, manual rescan | Observe (1) |
| Tag a finding; confirm or merge entities | Annotate (2) |
| Reveal a value; add or edit a vault entry; copy to clipboard (30 s auto-clear) | Modify (3) |
| Export a dashboard audit | Transmit (4) |
| Redact / purge a finding; delete a cookie; clear an entity's cookies | Destruct (5) |

Any PII operation originating from Data-plane content is refused at level 3 and above, as for every
other action.

*Note:* **redact** is designed as an irreversible purge (level 5). As built it is a recoverable
soft-delete.

## Vault

**Status:** ✅ Built on mobile; absent from the native shell.

(code: `crates/sovereign-crypto/src/{vault,password_gen}.rs`; `crates/sovereign-app/src/tauri_commands/pii.rs`)

- **New secret:** entity (existing or created inline), kind, label, masked value. Creates a
  `PiiRecord` with `stored_secret = true`, `confidence = 1.0`, `review_state = confirmed`, no sources;
  the value is encrypted under the AccountKey. Level 3.
- **Reveal and copy:** level 3; copied values are cleared from the clipboard after 30 s.
- **Password generator:** OS-seeded CSPRNG; default length 24; configurable character classes;
  optional exclusion of ambiguous characters. Used by the vault dialog and by signup capture.

## Embedded-browser flows

**Status:** ○ Not shipped — built only in the frozen Tauri desktop; the shell has none and mobile has no browser. Both flows are **user-initiated**; nothing reads forms on its own.

(code, frozen Tauri desktop: `crates/sovereign-app/src/browser_pii.rs`; `crates/sovereign-app/src/tauri_commands/pii.rs`)

Silent form-snooping is the wrong default (*Plan Visibility*, *Action Gravity*), so every flow starts
from an explicit toolbar action. The browser itself is specified in ch. 10; sign-in from the PII
manager in the shell is part of A5 (ch. 10), decided and to be specified.

- **Save credentials (signup capture).** The user clicks *Save credentials* on a signup page. Form
  fields are read and classified by `type` / `autocomplete` / `name`; a password can be generated and
  typed into the form. A confirm-and-edit dialog infers the entity from the page's host (creating it
  if no domain matches). Each field becomes a vault `PiiRecord` linked to the entity.
- **Signup counts as a sharing event.** For each captured field a `ShareRecord` is written with
  channel **`web`** and the page URL (encrypted) — the user has just disclosed that information to
  that entity.
- **Fill from vault (autofill).** On a page whose host matches one of an entity's domains exactly
  (a leading `www.` is ignored on both sides; subdomains do not match), the user clicks *Fill from vault*, chooses an entry, confirms at level 3; the value is
  decrypted and injected into the form, and `last_revealed_at` is updated. The decrypted value is
  used for the fill only and never logged. Incrementing `use_count` on a successful sign-in is
  specified, not built.

## Cookies

**Status:** ○ Not shipped — frozen Tauri desktop only.

(code: `crates/sovereign-app/src/cookie_api.rs`)

Cookies stay in the webview's own cookie store. **Sovereign does not mirror cookies** into its
database: they are session state owned by the webview. The dashboard's *Cookies* tab is a view over
the webview cookie API, joined to an entity's `domains[]`: name, domain, masked value, expiry and
flags; reveal and copy at level 3, delete at level 5, and a per-entity *clear all* at level 5.

Cookie values that contain PII are shown as cookies, not duplicated as `PiiRecord`s. *Promote to
vault* — copying a cookie value into a new vault entry while leaving the cookie in place — is
specified; the current button directs the user to the manual *New secret* dialog.

## Share ledger

**Status:** ◐ Partial — storage and per-entity view built (mobile); no shipped path records a disclosure.

(code: `crates/sovereign-app/src/pii_share_hook.rs`; `crates/sovereign-app/src/tauri_commands/pii.rs`)

**Requirement.** When the user sends an outbound message, every referenced `PiiRecord` (and any newly
detected value) produces one `ShareRecord` per recipient entity, written synchronously with the send.
A pre-send confirmation states the count ("This message contains 2 personal items that will be logged
to your sharing ledger"); sending is already level 4.

**Built:** the `ShareRecord` model and storage and the dashboard's *Shared* tab per entity (mobile).
Web-form disclosures from signup capture are recorded only in the frozen Tauri desktop (not
shipped). The email send hook is implemented but dormant with the comms channels.

**Not built:** the pre-send confirmation; outbound hooks for Signal and WhatsApp; records for
disclosures to contacts not yet attributed to an entity (`to_entity_id` is required, so these are
skipped). Historical messages are not retroactively scanned.

## Dashboard

**Status:** ◐ Partial — full dashboard on mobile; review window in the shell. On mobile, which has no browser, the *Cookies* tab has nothing to read.

Three columns: **entities** (sorted by record count, grouped by kind, with the Sovereignty Halo shape
cue — rounded for `Self`, parallelogram for external); **entity detail** with tabs *Inventory*,
*Vault*, *Shared*, *Cookies*; and the **review queue**. Values are masked by default. Keyboard: `P`
opens the dashboard; panels are focus-trapped dialogs with Escape to close.

The native shell lists records with kind, review state, confidence, sources and origin, with
confirm / dismiss / delete; values stay masked there.

## Identity Firewall

**Status:** ⚠ Conflict — decision pending. See ch. 10, *Identity Firewall* (Appendix C is itself inconsistent on "synthetic"). The PII manager covers
part of its intent (credentials held per entity, disclosures recorded); nothing of the firewall
itself is built.

## Not settled / future

**Status:** ○ Specified, not built.

- What the idle sweep does with what it finds — with A5 (above).
- OTP / TOTP generation for second-factor secrets.
- Per-entity risk scoring from PII volume and share frequency.
- Hash-chained audit of dashboard operations (with the audit-log hash chain).
- Vault sync across the user's devices beyond what general sync carries [unverified — PII records and
  share records are among the synced tables; a vault-specific design is not specified].
- Credential files found by bulk import routed to the vault.
