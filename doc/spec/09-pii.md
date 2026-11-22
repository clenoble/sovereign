# 09 — Personal Information (PII)

How Sovereign holds personal information: by reference, in an encrypted vault keyed to the
entity it belongs to, with a ledger of where it was disclosed. This chapter specifies the design
and then states, separately, how much of it runs today.

*Consolidates:* the PII management dashboard design in the project's archived plans (private;
main design source, 2026-04-30); `RELEASE_NOTES_v0.0.4.md` (dashboard, vault, ledger, browser flows), `v0.0.5`
(AccountKey), `v0.0.8` (PII-001/PII-002 locale and read-time redaction), `v0.0.9` (shell
session-log parity); the v0.0.8 release plan, archived (private; COMMS-001 disposition);
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

**Status:** ◐ Partial — data model and resolver built; since v0.0.13 (lot LS(a)) both hosts show a token as its label in titles (cards, windows, alerts, suggested links) and in the phone's reader, the phone's edit box excepted for v0.0.13 (SW5); enforcement at ingest is partial (see *The "no stray PII" ingest invariant*).

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

**Status:** ✅ Built — (code: `crates/sovereign-ai/src/pii/{commit,ingest}.rs`; `crates/sovereign-ai/src/docs_host.rs`).

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

- **Documents:** ingest runs **regex-only**, at the Swiss locale, in the document service's hook, on
  every write that changes a document — create, save, import, a skill's output, the restore of a
  version and sync — in the native shell and on mobile, whenever the persona's account key is
  installed; without it the write is stored *not scanned* (G10). Each ingest takes every text
  field: the title, the text blocks, captions and alt texts. (code:
  `crates/sovereign-ai/src/docs_host.rs`, `crates/sovereign-docs/src/service.rs`)
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

**Status:** ◐ Partial — decided 2026-10-10, replaced 2026-11-17 (below); the document half built in v0.0.13 (lot LS(a), merged 2026-11-17), the message half in v0.0.14.

**Built in two parts, designed once** (Céline, 2026-11-13):

> on coupe le balayage des données personnelles, première partie sur les docs, cela découple cette
> fonction du mail, tout en obligeant à la concevoir aussi pour le mail.

- **The parts:** documents in v0.0.13, after C-HOST; messages in v0.0.14, with email.
- **One contract covers both kinds of content.** It is stated for a document and for a message, and
  the review queue, its store and its persona are designed once, for both.
- **The queue's cadence and form** (Céline, 2026-11-13, the two points left open on 2026-10-10):
  - **cadence, « Un signal discret »:** a count in the status bar when findings wait (« 12 personal
    data to review »), and a notice at login if the queue has grown since the last one. No window
    ever opens by itself;
  - **form, « Par type de donnée »:** the review window groups findings by type (addresses,
    numbers, IBAN…) across all content. Each value is confirmed or dismissed, alone or with others
    selected; never a whole type at once (Céline, 2026-11-17: « Je ne donnerai pas la possibilité à
    l'utilisateur d'écarter tout un type d'un coup. Au plus, il peut sélectionner (checkboxes) de
    multiples records et les écarter d'un coup. » « Par type » is how the window groups.)

**Decided 2026-11-17 (Céline), replacing the decision of 2026-10-10 below: the sweep is the same
process as a save.** « Oui, le balayage en arrière-plan remplace cette décision. C'est le même
processus. C'est juste qu'à l'enregistrement, c'est visible directement par l'utilisateur, et donc
confirmé par défaut. Si c'est une erreur, l'utilisateur peut aller rejeter dans le gestionnaire des
PII. En arrière-plan, il ne le voit pas, donc cela rentre dans la file à confirmer. » The sweep
replaces each value by a token, as a save does, and marks it « PII suspected - TBC », to tell it
from a value replaced under her eyes. C-SWEEP below says how.

**Decided 2026-10-10 (Céline), replaced on 2026-11-17: detect without rewriting, review in a queue,
mask on the fly until reviewed.**
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
tokens and labels cleanly; masking at read time is the single path by which document content
reaches the model (summaries, chat tools, skills, consolidation). Where they stand: the first is
met for documents (above); the native shell's editor shows tokens as labels, mobile does not
(*Resolution and access levels*, below); the third is not met.

The v1 design rescans, when the system is idle, any content lacking `pii_scanned_at`, and re-scans
when the detection taxonomy changes. A sweep routine exists in code but is not wired to any trigger;
it was unwired on 2026-05-03 and deferred since. It is also compiled only with the `comms` feature,
so it is absent from the mobile build, and it rewrites bodies outside the document service. It is
retired in v0.0.13 (SW9). What the sweep does with what it finds is the decision of 2026-11-17
above. (code: `crates/sovereign-app/src/pii_sweep.rs`)

**Guarantees (C-SWEEP, each one a test).** ◐ The document half is built in v0.0.13 (lot LS(a), merged
2026-11-17), on both hosts. Its second run read each host: the sweeps after login and after a sync
round, the incoming-change analysis by document, the review state on arrival, the shell's window from
the review routine, the phone's commands, the masking of titles and bodies toward the model, and one
label table. The message half is specified with it and built in v0.0.14, behind the message hook.
Specified 2026-11-13, revised 2026-11-17 against the code and Céline's answers of that day.
- **SW1 — One process, two states.** The sweep runs, on content not yet analysed, the ingest a save
  runs: each detected value is replaced by its token, the raw original is kept encrypted, the content
  gets one new version, and it is marked analysed.
  - **The state follows the write's origin, which ingest is given.** A value replaced where she sees it
    happen, a save or a creation through the editor, is confirmed by default (Céline, 2026-11-17:
    « uniquement l'enregistrement via l'éditeur. Pas l'import de document. »). A save or a creation by
    the assistant shares the editor's write path, not its origin: it is suspected.
  - **A value replaced on any other path is suspected,** shown in words on each row as « PII
    suspected - TBC » (Céline, 2026-11-17: « Suspected seul ne dit pas ce qui est suspected »), and
    waits in the review queue: she did not see it. That is the sweep, an import, a skill's output, the
    restore of a version, the assistant's writes, content shared into Sovereign from another app on
    the phone (which takes the import's origin, not the editor's), a page saved from the browser
    (likewise), and content received by sync that no device had analysed.
  - **The two other states take the same form,** « PII confirmed » and « PII dismissed » (Céline,
    2026-11-17: « on garde la forme longue pour les libellés PII »).
  - **A state is never lowered by another device's ingest.** On the sync path, a record that already
    exists, received or local, keeps its state; only a value the hook replaces with no record yet is
    suspected. A document and its records may arrive in either order: when a confirmed or dismissed
    record arrives after its document, its state replaces the suspected one the hook wrote.
  - Only her review changes a suspected value's state.
  - **When it runs:** after a login, once the session is open, and after a sync round that brought
    content not yet analysed, in bounded batches. On the phone, only while the app is open.
- **SW2 — One identity for a finding.** A finding is keyed by its content's identity, the content's
  version and the span, within its field.
  - The identity is the document's id, or a message's identity (ch. 13).
  - The version is the normalised hash, or the stamp (C-WIRE).
  - Two devices that scan the same version produce the same record, and sync converges instead of
    duplicating.
  - **The token's record id is derived from that identity,** not minted by the store, so two devices
    that sweep the same version write the same new version, which converges by its hash instead of
    making two concurrent versions.
  - A finding whose version is no longer current is found again, or dropped, at the next scan.
- **SW3 — Masked where it is read, for every path to the model.**
  - **The mask is computed on the text as it is read, each time,** with the content's id, by the same
    detection. It rests neither on the sweep having passed nor on stored spans. It is applied where
    content is read for a prompt (the chat's workspace context and tools, the summary, the
    incoming-change analysis), not at the model call, where the content's id is gone.
  - **What gets masked:** a document whole, its body, title, captions and alternative texts.
    Detected values are replaced by their type labels; a token stays a token; a value she
    **dismissed** (SW5) is left as written, so that her dismissal reaches the assistant too.
  - **What it does not cover, said here and in the notes:** thread and contact names, her own message
    to the assistant, the browser page sent to the reliability check, and the two callers that run
    in no shipped build (consolidation and `thread_summary`).
  - **The gate check:** a source scan, as AR8's, fails on any call to the model outside the named
    ones (the masking readers, the classifier of her own message, the detector). It reads names, not
    data flow: its limits are written beside it.
  - **The summary no longer refuses an unscanned document:** masking at read covers it.
  - **The tests:** a dismissed IBAN reaches the model as written, and an undismissed one as its label,
    in a body and in a title; the gate check fails on a new call outside the named ones.
- **SW4 — The queue belongs to its persona.** Findings, dismissals and the count seen at the last
  login live in the persona's area (C-AREA, ch. 07). A duress session never sees the primary's
  queue. The walks know them by name.
- **SW5 — Review, grouped by type, on both hosts.**
  - **Any record can be confirmed or dismissed,** whether the sweep suspected it or a save replaced
    it: one, or several selected at once. No gesture acts on a whole type (Céline, 2026-11-17).
  - **Confirming a suspected value** keeps its token and makes it confirmed.
  - **Dismissing a value** restores it as written from the encrypted original, one new version per
    document. Her cases: « une erreur d'identification, ou un exemple générique ».
  - **A dismissal holds for as long as that value stands in that content,** wherever an edit moves it.
    It is the value's record, in the state « PII dismissed », matched against the record's encrypted
    value, so the store holds no raw value. **Ingest reads the dismissals
    at every save, on every device,** and does not replace that value again.
  - **Both can be taken back** in the window. Taking back a dismissal replaces the value by its token
    again, confirmed, since she has now seen it; one new version per document. Taking back a
    confirmation, whoever confirmed it (her review, or a save in the editor), makes the value
    suspected again, and its token stays.
  - **The review state and the dismissals travel by sync** (Céline, 2026-11-17: « Voyage, +
    téléphone complet »).
    - **A review received from another device is read as a time before it can win** (v0.0.13 rc2;
      Céline, 2026-11-20: « oui, à faire »). ✅ Built (rc2's lot R3): in `sovereign-p2p`, `merged_review` and the new-record branch of
      `apply_pii_record_row`, the one path a PII row takes. A review merges by its own stamp, whichever copy
      of the record wins the rest, and a later review wins over an earlier one. Its stamp is taken only when
      it is in the form a review writes (RFC 3339, UTC, to the microsecond, ending in `Z`) and no later
      than the bound on future times that every synced row has (ch. 05, *Documents*). Otherwise the review
      it carries is ignored: the local review state and stamp stand, and the rest of the record is handled
      as before. A review of the past, or within the bound, travels as before. A record that arrives new, with
      a review whose stamp is not taken, is created unreviewed and with no stamp, for her to review on this
      device (the Fable maille, by delegation of Céline, 2026-11-20).
    - **The tests:** a stamp far ahead in the review's form, one two days ahead, and strings not in that
      form are each ignored, over ingest's state and over a review of hers; a review an hour old, and one an
      hour ahead, travel; after an ignored stamp, her next review stands; a new record with a stamp not
      taken arrives unreviewed and unstamped, and her review of it then stands.
  - **Both hosts show a token as its type's label,** never raw (Céline, 2026-11-17, for the phone:
    « Étiquettes sur téléphone »).
  - **A label has one form,** the one the model reads (`[IBAN]`, `[Card]`, `[IP]`, `[Name]`…), in titles,
    cards, the phone and the model's text alike. A code's own name (`[Iban]`) is never shown (the Fable
    maille, by delegation, 2026-11-17).
  - **« One new version per document »** is the review's own version. A document whose current state is
    not yet a version first gets the usual restore point (the Fable maille, by delegation, 2026-11-17).
  - **One exception, the phone's edit box, for v0.0.13** (Céline, 2026-11-17: « oui, 1 pour la
    v0.0.13 »). The box holds the stored form `[pii:…]`, since a label there would be saved as text;
    labels everywhere else. When the body holds a token, and only then, one line above the box says:
    « In this box, personal data shows as marks like [pii:…]. Leave a mark as it is to keep its
    value. » It is the box's description for a screen reader, not a live region (the Fable maille,
    by delegation, 2026-11-17). An editor on the phone that shows labels comes later.
  - **The test:** she dismisses a value, edits another paragraph and saves, here and on a second
    device. The value is not back in the queue, and it reaches the model as written.
- **SW6 — A discreet signal, on both hosts** (Céline: « Un signal discret »). A count in the status
  bar shows when suspected values wait, and a notice at login says if the queue has grown since the
  last login. No window opens by itself.
- **SW7 — Ingest is not an act of the assistant** (D9, as Céline stated it on 2026-11-17: « C'est le
  même processus »). The sweep is the ingest that comes late; what it replaced waits for her as
  suspected, and only her review confirms it.
- **SW8 — Detection is regex-only, and the notes say so.** Named entities are not detected at
  ingest or in the sweep.
- **SW9 — One routine, in the crate both hosts link.** It is built on `sovereign-ai`'s ingest and run
  by the shell and the phone app (Céline, 2026-11-17). The app's `pii_sweep.rs` is retired.

### Message, contact and share hooks

**Status:** ○ Specified, not built — the hooks exist; no running path constructs them.

`PiiMessageHook` (inbound messages), `PiiContactHook` (contact addresses and notes) and `PiiShareHook`
(outbound ledger writes) are implemented and tested. They are not constructed by any running path;
communication channels are dormant since v0.0.9 and are to be wired together with their hardening.
(code: `crates/sovereign-app/src/{pii_message_hook,pii_contact_hook,pii_share_hook}.rs`)

## Resolution and access levels

**Status:** ◐ Partial — the resolver is built and used toward the model (type-only labels); since v0.0.13 both hosts show a token as its label wherever they draw a title, and the phone's reader does too; the phone's edit box keeps the stored form for v0.0.13 (SW5).

(code: `crates/sovereign-ai/src/pii/resolve.rs`; `crates/sovereign-app/src/tauri_commands/pii.rs`)

A token resolves at one of four levels:

| Level | Renders | Action Gravity |
|---|---|---|
| `preview` | type only, e.g. `[Email]` | Observe (1) |
| `masked_sample` | kind-specific mask, e.g. `a***e@e***e.com` | Observe (1) |
| `reveal` | the decrypted value; sets `last_revealed_at` | Modify (3) |
| `raw_original` | the decrypted raw body | Modify (3), confirmation each call |

### Where a token resolves

**Status:** ◐ Partial — decided 2026-10-10 (Céline); built for documents: the document service
and the native shell's editor treat token-form text as a token only in a document listed among
its record's sources (G13; code: `crates/sovereign-docs/src/service.rs`,
`crates/sovereign-shell/src/app/editor_v2.rs`). Not yet toward the model: the read-time redaction
renders any token-form text that names a record as its type-only label (code:
`crates/sovereign-ai/src/pii/resolve.rs`, `crates/sovereign-ai/src/tools.rs`).

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

- The idle sweep, as decided on 2026-10-10 (*Idle sweep*, above). The review queue's cadence and
  form are still to define.
- OTP / TOTP generation for second-factor secrets.
- Per-entity risk scoring from PII volume and share frequency.
- Hash-chained audit of dashboard operations (with the audit-log hash chain).
- Vault sync across the user's devices beyond what general sync carries [unverified — PII records and
  share records are among the synced tables; a vault-specific design is not specified].
- Credential files found by bulk import routed to the vault.
