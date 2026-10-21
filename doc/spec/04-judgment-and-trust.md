# 04 — Judgment, Attention & Ownership of Trust

How judgments about a document (reliability, attention) are stored, who may declare a
document degraded, how attention is recorded, how ownership is decided, and what the general
export owes the user.

*Consolidates:* specification v1.3 §4 (2026-08-04), unchanged in substance; status lines added
2026-10-09.

**Status:** ○ Specified, not built — nothing in this chapter exists in code yet. The legacy
`reliability_*` fields on `Document` (ch. 03) encode the contract these records replace.

**Provenance.** These decisions were settled across three nodes — Sovereign GE, Spotter and
Crabe — on 2026-08-04, arbitrated by Céline, in the project's private coordination channel.
This chapter is the canonical record; the channel carries the path that produced it.

### Judgments are records, not fields

**Status:** ○ Specified, not built.

A judgment about a document — Crabe's reliability verdict, Spotter's attention vector — is stored as its own record, not as columns on the document.

```
Assessment
  document_id   : String
  judge         : String            // "crabe" | "spotter" | …
  judge_version : String            // the CONTRACT version, not the product's
  model         : Option<String>    // a verdict without its judge is not comparable over time
  policy        : Option<String>    // fingerprint of the composition policy, when one applies
  assessed_at   : DateTime<Utc>
  degraded      : bool              // declared by the judge — see below
  payload       : String            // the judge's vector, in the judge's own form
  deleted_at    : Option<String>
```

Four reasons, in order of weight:

1. **A judge's contract changes.** Crabe moved from a single 0–5 score to a vector of four axes carrying evidence levels; `reliability_score: Option<f32>` cannot receive that without flattening away the distinction between *could not look* and *looked and found nothing*. `payload` receives any contract without a migration.
2. **Re-assessment needs a history.** With a single `assessed_at` that overwrites, *the user's model moved* and *the judge moved* leave the same trace. They are different facts and must stay distinguishable.
3. **Invariants become auditable at the storage layer.** Crabe's contract forbids an evidence level from rising: a `recalled` claim later confirmed becomes a separate `retrieved` claim, never a promotion. If verdicts overwrite, a legitimate pair and a silent promotion are indistinguishable from outside the producer. Any judge whose contract forbids a transition has this need.
4. **Sensitivity becomes separable.** An attention verdict and a reliability verdict do not deserve the same encryption or the same sync scope. As two field sets on one document they inherit the same by construction — that is, without anyone deciding.

**Legacy.** `Document.reliability_classification` / `reliability_score` / `reliability_assessment` / `assessed_at` are the earlier inline form. They encode a contract Crabe no longer emits. Nothing is wired to Crabe today, so nothing is broken; they are superseded by the record above. When touched, `assessed_at` is renamed `reliability_assessed_at` (serde alias for compatibility) so it is not the one unprefixed member of its group.

### `degraded` is declared by the judge, never derived

**Status:** ○ Specified, not built.

Sovereign does not compute `degraded` from a payload. Only the judge knows which of its own states mean *I could not look*.

The distinction this protects: **`absent` is not a degradation.** *Looked, found nothing* (most web pages carry no byline) is a result; *could not look* is a failure. A flag raised on "an axis is not fully populated" marks nearly everything and stops meaning anything within a week. Deriving the flag here would encode today's reading of a judge's vocabulary into our schema, and be wrong the next time that vocabulary moves.

### Caches are consumer-driven, and never the verdict

**Status:** ○ Specified, not built.

- **A cache exists because a surface sorts or filters on it** — not because a judge exists. The column count tracks what the UI actually needs, not the number of judges.
- **Its form is whatever that judge can produce.** Crabe emits no composite, so its cache is an *evidence floor* — the weakest level present in the vector — which is derivable, invents nothing, and is what one filters on (*show only what rests on `retrieved` or better*). Spotter needs none: only surfaced entries enter the store, and a five-item digest does not need a denormalised sort key. A judge without a composite does not fabricate one to fill a column.
- **The vector is authoritative; any scalar is a derived cache**, recomputable, never gated on. What made the earlier `reliability_score` brittle was not storing a number — it was storing it *as the verdict*.
- **A composite that depends on a user-owned policy carries that policy's fingerprint, or is not stored.** An attention score is a function of weights the user sets. Change the weights and every stored composite is wrong with a recent date and no signal — computed under a rule that no longer exists. Reliability has no such dependency; the two are not symmetric.

### Attention entries and user signals

**Status:** ○ Specified, not built.

- Spotter writes into **a Spotter thread** — generic or per topic, the user's choice.
- **Only what the editor surfaced enters the store**, as `is_owned: false`. The set is bounded (a finite digest), so it does not fill the Digital Master with what the user did *not* choose. Candidates that were never surfaced stay with Spotter.
- **Signals are timestamps, not a status enum**: `proposed_at`, `opened_at`, `last_read_at`, plus an open counter. A state machine overwrites the repetition (a third read is a stronger signal than a first) and the chronology (proposed on the 1st, read on the 9th — the gap says something). The status reads as the furthest field that is filled.
- **"Read" is an explicit user gesture, never inferred from display duration.** Three reasons, cumulative: duration is a poor proxy in both directions (a document left open on a second screen, a fast reader); an explicit mark also serves the user, who remembers what she has read; and **a timer that continuously measures what is on screen is a repointable capability** — it serves the mirror and would serve anything else equally well. A "read" gesture repoints onto nothing. Sovereign has no view instrumentation today and does not acquire one for this.

### The méta-miroir

**Status:** ○ Specified, not built.

- **Opt-in, and not retrospective.** Judgments are retained only once the user turns the mirror on, and it begins there. The alternative — collect silently, reveal on activation — is collection without consent. The accepted cost: the mirror can never answer a question about the past, which is exactly when one would want to switch it on.
- **The activation date is stored and displayed.** Otherwise the mirror computes "you read 40% of what contradicted you" over a window that silently begins mid-history — a percentage without its denominator.
- **The host regime is displayed alongside it.** Precision differs by host (below), so a mixed history must not be presented as uniform. The *nature* of a guarantee is part of what a host declares, not only its presence.
- **The challenge cursor is stored in Sovereign, and not in the browser extension.** Spotter's rule is that the cursor is never persisted — a register of *where I am tender* is precisely what should not be held on someone. In a field-encrypted, telemetry-free local store that judgement changes: it is safe to keep, and it makes the mirror materially more precise. The rule is therefore **host-dependent, not absolute**, and the extension keeps the stricter form.
- **Per-persona isolation is load-bearing here.** Cursor plus challenge values are the tender-points register; a coerced session must not expose them. This is automatic while they are stored as ordinary field-encrypted entities, since key DBs are already per persona (CRYPTO-001). It must not be bypassed later for a performance shortcut.
- **The history syncs across the user's own devices** — decided explicitly, not inherited from sync defaults.

### Ownership is not decided at landing

**Status:** ○ Specified, not built — **unblocked 2026-10-10** (D3 decided, ch. 15). Today ownership is decided for a whole import tree at landing (ch. 03).

**Decided (Céline, 2026-10-10).** A document's *trust* has three states — **owned**,
**external**, **undetermined** — and can move between them in both directions by the user's
explicit gesture (an adopt can be undone). **Undetermined is treated as external** (injection-
fenced) until the user decides. Next to it, a simple **provenance** record says where the
content came from (written here; web, with its URL; import, with its source path; received from
a paired device). Provenance travels with sync (ch. 05), so a document keeps its trust state on
every device. Ownership *between people* is a different question, left to a future collaboration
chapter.

**On the canvas (ch. 12)**, the halo follows the trust state, with a third visual for
undetermined.

**Decided (Céline, 2026-10-15) — defaults at creation and adoption gestures.**
- **What the user writes inside Sovereign from scratch is owned by default**: a new document, a
  journal entry, a copy of her own document, what the assistant writes at her request.
- **What comes from outside lands undetermined or external**: an import (file or bulk) and what a
  skill draws from an external file land **undetermined**; a web page or shared web content lands
  **external**. Nothing from outside is ever created owned.
- **Adoption gestures**, each an explicit user decision recorded on every document it touches (the
  trust decision of ch. 03, C-DOC, which travels with sync):
  - one document;
  - every document of a thread;
  - every document of one import (the documents whose provenance names that import's folder);
  - a selection made in bulk;
  - **by date**: every undetermined document whose original creation date, or last modification
    date, is older than a date the user chooses (for example, more than five years old).
  - **Thread, import and date gestures take undetermined documents only**; an external document
    (web, shared web) is adopted only when the user picks it herself — one document, or an
    explicit selection — so adopting a thread never silently moves fenced web pages into the
    trusted plane.
- **Two guards on every bulk gesture.** A preview first — how many documents, and which — before
  anything changes. And the gesture applies to the documents selected *now*; it never becomes a
  standing rule for a thread, a folder or a date range (the same rule as *Inheritance in depth
  only, never in time*, below). **A date is not proof of authorship**: a file's dates come from
  wherever it came from, and an archive can carry any date its sender chose; adopting by date is
  a convenience for a corpus the user knows, and the preview says so.

`is_owned` today is set at construction, and **no update path can mark a document external afterwards**: `update_document` takes title and content only, and the one ownership setter, `adopt_document`, only sets it to owned (ch. 03). `update_entity`, by contrast, does take `is_owned` — the general mutability exists for entities and not for documents, which is a gap, not a doctrine. Bulk import sets it `true` for everything, inferring the content's provenance from its container: true of the *file*, false of the *content*. A folder of saved articles, a mail export, someone else's document all land *owned, trusted, control-plane, never injection-fenced* — and it is the fence that protects against external content. `just docs, no code` excludes executables; it does not exclude hostile prose.

**Neither `true` nor `false` is an acceptable default**: `false` would make the halo lie in the other direction and treat the user's own corpus as foreign. Both values are wrong for part of any real batch, which is the argument for posting neither.

- **A third state — ownership undetermined, pending judgment.** What has not been judged is not defaulted; it is marked unjudged. If landing decides nothing, there is nothing to correct afterwards, and the (unbuilt) stub-reprocessing mechanism stops being load-bearing.
- **Undetermined fails closed**: treated as external and fenced. Otherwise the defect has merely been renamed.
- **Precedent:** `PiiRecord.review_state` already models a record awaiting human judgment, with its review queue and dashboard. This is that pattern on another field, not a new invention.
- **Resolution is a user declaration, made at dry-run**, when she is already reviewing the manifest. Asking her about a path is a *declaration*; deriving ownership from a path is an *inference* — the distinction is the whole point.
- **Keyed on the folder path, never on the lane name.** `FolderAsLane` maps a file to the *name* of its immediate parent, so distinct folders sharing a name are one lane — the codebase's own example is twenty-four separate `.../Fig N/guidelines/` folders collapsing to lane `guidelines`. A per-lane answer would apply to folders the user never looked at.
- **Inheritance in depth only, never in time.** A declaration covers what is in the manifest now; it does **not** persist as a rule for that path. A path is a mutable container that third parties write into — a sync client, another application, a download landing in a declared folder — so a persistent declaration grants trust to everything anyone will ever place there. **The resolved decision is recorded on the document, and never re-derived later from the container.**
- **A per-entry trust classification in the import `Manifest` is deliverable first, independently.** `PlannedFile` already carries path, title, extension, size, dates, lane and disposition; the dry-run warning is global where the risk is per file. One field turns a blanket warning that arrives when the user wants to proceed into information attached to the thing that carries it.

### Export

**Status:** ○ Specified, not built.

**Sovereign builds a general export.** The reason is Céline's and it is short: users must not be locked into Sovereign. Until now the workspace shipped a bulk *import* engine and no counterpart — the doctrine asserted abandonability (*questionable, replaceable, abandonable*) while the mechanism did not exist. A specialised archive format written by one component would otherwise have become the ecosystem's only export path, by accident.

**Plaintext export is a major security exposure and is treated as such.**

- **Encrypted by default.** The archive key is **derived from the persona's passphrase**, never from a fresh export passphrase: a separate export passphrase is a second door to the real persona, and the duress design does not model it. The cost — no restore without the original passphrase, no handing the file to someone else — is the trade already accepted elsewhere (a lost passphrase means re-authoring, not recovery).
- **Any regime or provenance declaration lives inside the ciphertext**, never in the filename or a plaintext header. An archive that describes itself is an archive that announces itself. The cost is deliberate: no magic-number header, so no format auto-detection — the user designates her archive on import instead of a tool recognising it.
- **What an archive may contain is bounded by the protection its host can provide.** A host without encryption exports *less*, rather than exporting the same content unprotected.
- **The artifact leaves our control surface.** Once written it is a file of the operating system, visible to whoever holds the session regardless of which persona is open; it cannot be made persona-invisible, and any mitigation claiming otherwise is a promise. This is surfaced to the user at creation rather than mitigated away.
- **Export bookkeeping, if any, lives in the persona's own store or does not exist.** Precedent: `MODELTRUST-003-PERSONA`, where a shared `model_tofu.json` was read and rewritten by both personas.
- **A dedicated importer sets `is_owned` itself** and inherits nothing from the bulk path's ownership semantics. An archive of judgments about web pages is external content; laundering it into the trusted plane is the failure this whole section exists to prevent.

**ATREST-011 — direction, not yet a fix.** Our per-persona files are distinguished by name (`index.key` / `index.duress.key`, `model_tofu.json` / `model_tofu.duress.json`), so the *partition* is readable by whoever holds the disk while only the *content* is encrypted. The principle that names the fix: **transparency is owed to whoever holds the key, not to whoever holds the disk.** The distinction belongs in what the passphrase opens — indistinguishable key slots in one container — rather than in filenames that differ before any attempt is made.

### Not settled

**Status:** Open by design.

- **Action Gravity has no level for power over a person.** Its five levels grade power over *data*, with friction scaling with irreversibility. Shaping attention is a different axis, not a sixth level. Spotter's attention allocation is the first use case. Deferred deliberately; nothing is wired.
- **Host divergence versus the intimacy of what differs.** The cursor rule makes the widest gap between the browser and Sovereign fall on the most intimate artifact, while the stated aim is a minimal gap. Declaring the regime makes it legible; it does not remove it.
- **These decisions attach to a `Document` structure this specification does not currently describe** — see *Open Questions*.

---

