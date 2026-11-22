# 01 — Vision

What Sovereign GE is for, and the tenets every other chapter has to answer to. A design
choice that contradicts a tenet here is either a bug or a decision that has to be taken
explicitly (and recorded in `CHANGELOG.md`).

*Consolidates:* specification v1.3 §Vision & Philosophy (Core Tenets, Paradigm Shift);
the February 2026 design decisions (archived) (canvas, threads, halo); `README.md` (positioning);
Appendix C, `../legal/sovereign_os_ethics.md` §1.3, §3, §4 (binding constraints);
the native-UI migration plan, archived (private; OS direction).

---

## The shift: from application-centric to content-centric

**Status:** ◐ Partial — the content model, canvas and skills exist; most work still happens
in built-in windows rather than through skills.

Software today imprisons data: a file belongs to the application that wrote it, and folders
are the application's filing system. Sovereign GE reverses the relation. **Content is the
primary citizen; software is a set of tools that operate on it.**

**Contacts and documents are the two fundamental concepts** (Céline, 2026-11-10 and 2026-11-13).
- **Intent threads** (projects, contexts) are how they are organised, not a third concept.
- **Messages** are the exchanges with contacts, and they are placed in the threads.
- All of them, with their relationships, live in one local graph owned by the user.
- They are placed in time and in threads, not in folders.
- They are shown on a spatial canvas where time runs left to right on the desktop and top to bottom
  on a phone (ch. 12).
- What a message is exactly (an immutable event, or a kind of document) is not decided.

Sovereign GE is a local-first graphical environment — today a desktop application (the
native shell) and an Android app over the same backend, with an on-device AI, field-level
encryption and peer-to-peer sync, built in Rust and published under AGPL-3.0. It is a
prototype: the v0.1 bar is *data you can trust + daily driver on desktop* (ch. 15).

## Core tenets

**Status:** ◐ Partial — status per tenet below.

| Tenet | Statement | Status |
|---|---|---|
| **Data sovereignty** | The user owns the *Digital Master*: the local graph is the source of truth. Proprietary formats are exports, never sources. | ◐ The local graph is the store (ch. 03); the general export that makes the graph abandonable is specified, not built (ch. 04). |
| **Content first** | Data exists independently of software. Skills are called on data, not the other way around. | ◐ Documents, threads and relationships are independent records (ch. 03); 24 core skills run on documents (ch. 11). |
| **Skill-based architecture** | Monolithic applications are replaced by granular skills, orchestrated by an AI agent that proposes and the user approves. | ◐ Skills and the orchestrator exist (ch. 08, ch. 11); the orchestrator invokes few of them, and editing, inbox, browser and settings are built-in windows. |
| **Identity firewall** | Personal data and tracking are managed by the system, not left to each service. | ⚠ See below. |
| **Distributed resilience** | No reliance on a central cloud: devices sync peer to peer, and trusted people can restore access. | ◐ Device sync and guardian *access* recovery are built (ch. 05, ch. 06); crowd *data* backup is Phase 2. |
| **User sovereignty** | The user keeps ultimate control over data, identity and computing environment. The code is open and auditable. | ✅ AGPL-3.0, no telemetry, no account; no service holds the user's data in the clear (the relay sees only ciphertext, ch. 05). Friction scales with irreversibility (Action Gravity, ch. 08). |

*Replaces v1.x:* "Digital Master (Local Graph JSON)" — the graph is an embedded SurrealDB
database (RocksDB on desktop, SurrealKV on Android), not a JSON file (ch. 02, ch. 03). The
tenet is unchanged; only the storage form is.

### Identity firewall

**Status:** ⚠ Conflict — decision pending. Not built.

The v1 tenet reads: *"automated, kernel-level management of PII and cookies with synthetic
identity generation for external services."* Appendix C §3.1 is a binding constraint that
scopes the firewall to the tracking-prevention layer (cookie isolation, fingerprint and
referrer handling, tracker blocking) and forbids generating synthetic names, addresses or IDs.

> ⚠ **Conflict — decision pending (Céline).** v1 tenet "synthetic identity generation for external services" vs Appendix C §3.1 "the Firewall prevents tracking, it does not fabricate personas".

What exists today is the PII layer (detection, vault, ledger; ch. 09) and the embedded browser
with its SSRF guard (ch. 10). "Kernel-level" belongs to the OS direction below and is not a
property of the current application.

## What the tenets refuse

**Status:** ✅ Built — as constraints on what is shipped (Appendix C §3).

The tenets are bounded by Appendix C, which is binding:

- **Privacy without anonymity.** No bundled Tor, VPN or anonymising relay (Appendix C §3.2).
  Sovereign GE protects data at rest and in use; it does not hide the user's network presence.
- **Sovereignty without fabrication.** See the identity firewall above.
- **Small on-device models.** The shipped models are in the 3–8B range (ch. 14); larger models
  are a user choice, not a default.
- **No hidden flows.** No telemetry, no analytics, no cloud account. Transparency is part of
  the misuse mitigation (Appendix C §4.2).

## Design commitments that follow

**Status:** ◐ Partial — each is detailed, with its own status, in the chapter named.

1. **Local first.** Everything runs on the user's devices; network use is an explicit,
   gated action (ch. 08, ch. 10).
2. **Provenance is always visible.** Owned and external content look different everywhere —
   on the canvas, in the AI's answers, in the AI's state (the *Sovereignty Halo*, ch. 12,
   Appendix B §3). ◐ Not met on mobile: the Android canvas marks provenance with a colour bar
   only, without the shape cue (ch. 12).
3. **Hard barriers over good behaviour.** Constraints that matter are enforced by code paths,
   not by prompts (ch. 08, Appendix B §6).
4. **Surface, don't suppress.** Detected attacks, uncertain judgments and dropped items are
   shown to the user, not handled silently (ch. 04, ch. 08).
5. **Accessibility is a design constraint, not a retrofit.** WCAG 2.1 AA; provenance is
   carried by shape, not colour alone (ch. 12). ◐ Met on the desktop; on mobile provenance is
   colour plus a text label for assistive technology, which does not meet WCAG 1.4.1 visually
   (ch. 12).

## The user's role

**Status:** ○ Decided 2026-11-14 (Céline). A stance, carried by the choices each chapter names.

**Her words:** « l'utilisateur a des responsabilités et un rôle à jouer : ne pas
vouloir tout faire à sa place ou tout faciliter, surtout dès le début. » The user has
responsibilities and a role to play. Sovereign does not try to do everything in the user's place,
nor to make everything easy, least of all at the start.
- **« The start » is the platform's experimental phase,** not a user's first days (Céline, the same
  day: « dans le contexte de Sovereign, c'était dans la phase d'expérimentation de la plateforme »).
  Needs beyond what is commonly accepted are not anticipated far ahead: such a thing is built when
  it proves necessary.

## General rules

**Status:** ◐ Partial — each rule is detailed, with its own status, in the chapter named. A place
not yet built says so.

**Decided 2026-11-14 (Céline): the general rules have one place, here, and they are public.** Each
rule is stated once, where it was first met; this section indexes them, so that a new case can be
checked against the rules that exist before a new one is written.
- **A rule enters with the cases it has decided.** A rule that has decided one case only says so.
- **Not here:** how the project is worked on (its rules of method), and the principles by which
  decisions are weighed.

| Rule | Stated in full | Where it applies |
|---|---|---|
| **The requirement is in the type,** not in a prompt or an order of messages | commitment 3, above | a proposal's resolved target (ch. 08, AT1); the wire version carried by each document request (ch. 05) |
| **Surface, don't suppress** | commitment 4, above | ch. 04, ch. 08 |
| **What cannot be read is never shown as empty** | ch. 03, *What cannot be read is never shown as empty* | the places ch. 03 names |
| **A refusal is said,** never answered as an empty result | ch. 05, *Peers on an older build* | the refused document requests |
| **Nothing is remembered where the request can carry it** | ch. 05, *Peers on an older build* | the wire version |
| **By identity, never by position** | ch. 08, AT8 | the shell's lane rename, Skills menu and document windows; the phone's « + » button |
| **Before any login, nothing of a persona is answered or written** | ch. 07, AR2 | both hosts |
| **A decoy goes through the same path as the real thing** | ch. 07, *Login timing padding* and AR4 | the login's timing; the pairing decoy; the guardian-enrollment decoy, on both hosts |
| **A sentence sends the user only where the thing works as built, and promises nothing that is not built** | here | what the assistant says of export and import (ch. 08, ch. 11); the notice after the media pass (ch. 03) |
| **A text read by the model and by the user is written for the user;** what is for the model alone comes after its first line | here | a chat tool's failure (ch. 08). One case so far |
| **A message that crosses to another device names each device by its role,** never as « this » or « your » | here | the refusal sent to a peer on another wire version (ch. 05). One case so far |
| **One thing is said one way:** a screen keeps its own words, and a host has one form for one kind of refusal | here | the phone's lanes; the refused undos (ch. 08); the shell's gestures that need a login, « The workspace is locked: log in to … » |

*The last four rules were formulated while deciding cases met in implementation, by the Fable
maille, by delegation of Céline (2026-11-14).*

## Direction: the contents of other software, never the software

**Status:** ○ Decided 2026-11-13 (Céline), a direction, not scheduled.

**The direction is the content-centric paradigm, on every host.** The contents of other software
come into Sovereign as cards: by import, by the channels, by skills, by the web. Her words:

> Je ne veux pas que les autres logiciels entrent dans Sovereign, cela n'aurait aucun sens. Mais
> leurs contenus, oui.

*Replaces:* the native shell becoming a Wayland compositor (Smithay), in which other programs would
render as cards on the canvas. That direction is retired, on Linux as elsewhere (Céline confirmed
it on 2026-11-13: « Oui, retiré partout »). Nothing of it had
been built beyond the native shell itself.

## Direction decided on 2026-11-10 (D1–D18)

**Status:** ○ Decided, mostly not built. These are Céline's decisions from a strategy session on
2026-11-10, written into the spec on 2026-11-13 once their contradictions with it were resolved,
one by one (CHANGELOG v2.0.73). Each line points to the chapter that carries it.

**Posture and concepts**

| # | Decision | Where |
|---|---|---|
| D1 | **Posture.** Solid construction for the long term, not fast delivery. | — |
| D2 | **Concepts.** Contacts and documents are the two fundamental concepts. Threads organise them, and messages are the exchanges with contacts, placed in the threads. | *The shift*, above |
| D3 | **The inbox** is organised by contacts and ordered by how often one interacts with each, with favourites to keep a rare contact in view. As built, it is sorted by unread count, then by name. | ch. 12, *Inbox* |
| D5 | **The axis of the vision** is a decentralised network against a proprietary cloud, not the individual against relationships. | — |
| D6 | **Collaboration** is a real goal at 2–5 years, if the first steps are validated. It happens only between verified contacts, never over an anonymous or public channel (Appendix C §1.3, revised 2026-11-13). | Appendix C |

**Devices, node and independence**

| # | Decision | Where |
|---|---|---|
| D4 | **Paired devices** are the same user's other devices, with no hierarchy of authority between them. As built, no device commands another. The recovery bundle and card, and each guardian's binding to the enrolling device, are still on one device (ch. 06). | ch. 05, 06 |
| D7 | **The personal node with no interface is an optional add-on:** a redundancy the user may add to strengthen security (Céline, 2026-11-13). **Sovereign GE** stays the general name. In the longer term, Céline expects « Sovereign ecosystem » to name the whole, with Crabe and Spotter. | — |
| D8 | **The node's geometry varies.** It is blind by default; a few limited watch functions may be authorised. | — |
| D12 | **Independence.** The core functions follow the user everywhere, in a private and safe environment, on any machine the user owns, without depending on one hardware maker or system vendor. Targets: Windows, Linux, Android; macOS perhaps later. On ch. 07's full-disk-encryption requirement: it stays for now; the direction is to shrink what depends on it (Céline, 2026-11-13). | ch. 07, *What stays in clear* |
| D13 | **No network dependency** in exchange for that independence. | — |
| D14 | **The same functions on every host** as soon as the hardware allows; the phone is a complete Sovereign, not a reader by nature. « No AI on mobile for now » (2026-10-10) is the current step, a choice of order (Céline, 2026-11-13). | ch. 14, *Mobile* |
| D15 | **Installation on machines one owns** in the short term. A temporary reader on someone else's machine is a good idea, not a priority. | — |
| D17 | **A journal kept by each device**, exchanged directly between devices. The node is one more blind replica, never a required passage. It is the target that will replace the pairwise state sync of ch. 05 (C-WIRE, as built). **Mail is not carried by the node:** as the target, each device fetches its own mail by IMAP (Céline, 2026-11-13: « je veux que chaque machine soit autonome »). | ch. 05, ch. 13 |
| D18 | **The « phone as a reader » step is dropped.** A reader mode with a cache stays a choice the user may make. | — |

**Hard rules**

| # | Decision | Where |
|---|---|---|
| D9 | **Hard rule 1.** Without the user, nothing in Sovereign acts above the level « observe », on a device or on the node: it prepares and proposes. A standing rule the user has set counts as the user, as the 30-day purge of the trash does; so does a sync, which is the user's own act on another device (Céline, 2026-11-13). Earned trust (ch. 08) applies only on a request the user is making. Ingest, which stores detected personal data by reference at a save or later in the background sweep, is how Sovereign keeps content, not an action above « observe »; what it replaced without her seeing waits for her review as suspected (Céline, 2026-11-17: « C'est le même processus »; the wording, the Fable maille's, chosen by her; ch. 09, C-SWEEP). | ch. 08, 09 |
| D10 | **Hard rule 2.** What one authorises for the night is what a seizure obtains. This sentence is shown to the user when they grant it. | ch. 08 |
| D11 | **A sealed journal**, the common base of sync, backup and mail: a proposal to write for the spec. | ch. 05 |
| D16 | **Hard rule 3.** No behaviour in an interface. As built, much behaviour lives in the hosts; moving it into shared crates is planned (contract C-SESS, ch. 07). | ch. 02, 07 |

## Decided on 2026-11-13, from the session's proposals (P1–P23)

**Status:** ○ Decided, not built. The strategy session of 2026-11-10 also made 23 proposals. Céline
examined them by functional group on 2026-11-13. What she adopted is below, each with the chapter
that carries it. P13 (email read-only first) was settled the same day for the email release:
« Lecture seule en v0.0.13 », with no write to the server at all (ch. 13). That release has since
become v0.0.14 (ch. 15, *Roadmap*). P17a and P23 are for later.

**Mail and messages** (ch. 13; for the email release's contract C-COMMS)

| # | Decision |
|---|---|
| P1 | **A message is an immutable event; a document is a work object.** A skill reads anything and writes only documents. An explicit gesture promotes a message or an attachment into a document. **What is immutable is the message as received** (Céline, 2026-11-13: « L'original reçu »): its raw original, kept encrypted and never changed. The stored body is its tokenised form, written once at ingest, or after her confirmation for what the sweep finds, as for documents (ch. 09). |
| P2 | **How often one interacts (D3) counts deliberate acts only:** messages sent, documents linked, favourites. It is measured over a sliding 30-day window, whose start is shown. Opening a message does not count, since that would be an engagement loop. The Sent folder is fetched from the first version. |
| P3 | **A contact has a nature:** person, organisation or automatic (newsletters, notifications). |
| P15 | **No link in a mail is visited automatically.** A mail's link is often unique to its recipient, or triggers an action. More widely, the judges (Crabe, Spotter) receive their capabilities from the core, and their rights depend on the kind of content. |

**The network and the models**

| # | Decision | Where |
|---|---|---|
| P14 | **One gate for all network egress, in the core.** Every outgoing request carries a persona, a purpose and a consent: mail, remote models, search, checks. The identity firewall lives there. It makes Appendix C §1.3 (« no communication network of its own ») verifiable. Mail is born behind it, from the email release on (v0.0.14). | ch. 02 |
| P11 | **The model is chosen per task and per provider.** It is local by default. A remote model serves only when it is chosen for a task, and what leaves is shown before it is sent. | ch. 08 |
| P16 | **Sending a text to a remote model is a transmission** (level 4, Transmit). Only an entirely local judge can be a watch function. | ch. 08 |
| P17b | **The keys of model providers are per device, or account-wide, as the user chooses** (Céline, 2026-11-13). Synced, they travel encrypted like the rest. | ch. 07 |
| P18 | **The shell's single reliability score is removed** until the contract of Crabe v3 is integrated. As built, the shell shows « Factual · 4.2 / 5 ». | ch. 10 |

**Keys, node and sync**

| # | Decision | Where |
|---|---|---|
| P4 | **Capabilities are received, never sought.** One session per persona is the only provider of paths, keys and stores. This is the contract C-SESS, decided earlier. | ch. 07 |
| P5 | **Delegations:** a key restricted to a scope, revocable, that expires. One mechanism covers a watch function, a collaborator, revoking a device and a temporary reader. **The first step is rotating the account key,** so that a lost device can be revoked. As built, the account key is one secret shared by every device, with no rotation. | ch. 07 |
| P6, P7 | **A node is blind, or authorised by a delegation.** A node hosted by a third party stays strictly blind. | ch. 05 |
| P10 | **Each type of data has its merge rule:** additive, or ordered by stamp. Stamps are kept for what needs them. | ch. 05 |
| P12 | **The project never operates infrastructure.** A node is the user's own machine, or a replaceable, blind third party. Appendix C §5 (« Software, Not Service ») stays true. | Appendix C §5 |
| P19 | **Node and reader are roles one device can combine.** A single device is a complete Sovereign, and the journal works entirely locally. | ch. 05 |

**People**

| # | Decision |
|---|---|
| P8 | **A collaborator is a contact whose identity is verified by its own means**, not by the guardian protocol. That protocol stays minimal (Appendix C §3.3). |
| P9 | **Collaboration comes in this order:** a verified contact, then a shared thread, then backup between contacts, then backup in encrypted fragments in the opt-in, reciprocal mesh (ch. 06, Feature 2). A stranger never holds a document in clear, or a key. Céline: « Pas de partage de documents en clair, mais par contre, sauvegarde encrypté de fragments oui ». |

**Platforms and interfaces**

| # | Decision |
|---|---|
| P20 | **The promise becomes « no platform can hold the user »**, and each host declares what its system can see (with D12). The reason: an application cannot be more private than the system that carries it. Where the promise is public (the README, the website), it is to be reworded. |
| P21 | **The contents of other software, never the software** (*Direction*, above). |
| P22 | **No reopening of Tauri against the shell on the desktop.** After the core is extracted, the shell is tried on Android. The trial measures touch input, the phone's keyboard, and TalkBack. A single UI stack waits for that result. |
