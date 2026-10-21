# 05 — Sync & P2P

How a user's own devices find each other, reach each other across networks, pair, and
converge on the same workspace — with no server that can read anything. Recovery through
guardians, which rides the same transport, is chapter 06.

*Consolidates:* specification v1.2/v1.3 §P2P Storage & Guardian Protocol (Three-Layer
Architecture, Device-to-Device P2P Sync, Sync Protocol), §Version Control (conflict
resolution), §Encryption & Key Management (in-transit); the M0 transport spike (findings log
and filled decision matrix, branch `spike/m0-transport-discovery`, 2026-07-07/08); archived
plans `p2p-completion-plan.md`, `p2pbackupuserreadyplan.md`, `backup-m1-security.md`,
`HANDOVER-m2-guardian-app.md`; release notes v0.0.7–v0.0.9; the code at `main` d02c353.

## Scope and layers

**Status:** ✅ Built — device sync and transport; the layers below are what ships.

Three layers keep the user's data available:

1. **Primary storage** — each device holds the full workspace, encrypted at rest under its own
   key hierarchy (chapter 07).
2. **Own-device replication** — paired devices each hold a full, independently encrypted copy,
   kept converged by the sync protocol below. In v0.1, data durability is synced devices, full
   stop.
3. **Guardians** — hold shares of a Recovery Key that restores *access* when the passphrase is
   lost (chapter 06). They never hold data.

*Replaces v1.x:* "Layer 2 — P2P encrypted fragments across the user's own devices." Own devices
replicate the whole workspace rather than fragments; erasure-coded fragments belong to the
Phase 2 crowd backup (chapter 06, Feature 2).

## Transport

**Status:** ◐ Partial — reach through relays built; mailbox and deposit capability dormant; seed-list signing not built.

### Swarm and transports

**Status:** ✅ Built — (code: crates/sovereign-p2p/src/node.rs, behaviour.rs, identity.rs)

- **Stack:** libp2p 0.56. Transports: QUIC (`quic-v1`) and TCP with Noise + yamux, DNS
  resolution, and the relay-v2 client transport (Noise + yamux over the circuit). The node
  listens on QUIC; TCP is available for dialing.
- **Identity:** each device's libp2p Ed25519 keypair is derived deterministically (HKDF) from
  its Device Key, so a device keeps one PeerId across restarts. The PeerId is also the
  device's identity in the sync protocol.
- **Behaviours:** mDNS (toggleable), identify (`/sovereign/id/1`, agent string `sovereign` with
  no build version), request-response over CBOR on `/sovereign/sync/1`, relay-v2 client,
  DCUtR, and the mailbox client (`/sovereign/mailbox/1`).
- Idle connections close after 60 s.

*Replaces v1.x:* "Transport: QUIC … TLS 1.3 + device-pair key." Channels are authenticated and
encrypted by libp2p's Noise handshake (QUIC's built-in handshake on QUIC), bound to the
expected PeerId; sync payloads are additionally sealed under per-pair keys (see *Sync
protocol*). There is no TLS layer of the project's own.

### Discovery

**Status:** ◐ Partial — mDNS and seed relays built; the rendezvous client is compiled in but not
driven.

- **LAN:** mDNS discovers peers on the local network. It can be switched off
  (`enable_mdns`, default on); with it off, pairing needs the offer's dial addresses.
  (code: crates/sovereign-p2p/src/config.rs)
- **Internet:** reachability comes from relay reservations on the seed relays (below), not from
  a discovery service.
- A libp2p rendezvous client behaviour is part of the swarm and a `rendezvous_server` setting is
  plumbed through the configuration, but nothing registers or discovers through it.

*Replaces v1.x:* "Discovery: mDNS (local network) + rendezvous server (internet)." The M0 spike
replaced the rendezvous server with seed relays that every node already needs (decision D3).
Whether to remove the vestigial rendezvous client is not decided.

### Relay mandatory, hole punching opportunistic

**Status:** ✅ Built — (code: crates/sovereign-p2p/src/node.rs; test: crates/sovereign-p2p/tests/e2e_relay_reservation.rs)

A device that is not on the same LAN as its peer is reached **through a relay**. Direct
connection by hole punching is an optimization, never a dependency.

- At startup the node dials each configured seed relay, waits until the relay has identified it,
  then listens on the relay's `/p2p-circuit` address, which requests a reservation. (Requesting
  the circuit before identify completes produced no reservation; the deferral is deliberate.)
- DCUtR attempts to upgrade a relayed connection to a direct one. Its outcome is only logged;
  nothing in the node judges it or acts on it. Per the M0 lesson, the DCUtR event is not a
  reliable verdict — the side that loses the simultaneous-open race reports an error beside a
  working direct link — so success, if it is ever judged, must be judged by an established
  direct connection.
- The node does not eagerly confirm addresses observed by identify; doing so starves DCUtR of
  candidates.
- Every relay-client event other than an accepted reservation is logged at warning level, so a
  refused reservation is distinguishable from a silent relay.

### Measured basis (M0 transport spike)

**Status:** ◐ Partial — A applied; D partly applied: the seed-relay setting exists, with no
signed list and an empty default; B built, dormant; C not built (chapter 06).

The spike (2026-07-07/08) measured reachability on real networks before the guardian system was
built: two machines on a home network, one on mobile 5G behind carrier NAT, and a public VM.

| Sub-problem | Decision | Measured evidence |
|---|---|---|
| **A — reach known peers** | Relay-v2 + DCUtR, **sized for relay dominance** | Relayed path works over the real internet on both transports, every topology tested: connects 90–247 ms, circuits stable for minutes. Hole punch succeeds on the same LAN (~110–150 ms dial-to-direct) but **fails on both TCP and QUIC between mobile carrier NAT and home NAT**, re-validated under clean conditions with a public relay. With no reachable relay there is no path at all (clean ~11 s timeout). |
| **B — reach offline peers** | Store-and-forward **mailbox** (B1); mobile push wake (B2) not evaluated | Item deposited while the recipient was offline, delivered byte-exact on its next pull (~10 min residence across machines); drain-once holds; a third peer pulls nothing (pull is keyed to the authenticated connection). |
| **C — find fragment hosts** *(Phase 2)* | **Manifest-carried placement (C3)**; no content DHT | A fetcher knowing only the relay fetched from two manifest-listed hosts in 41 ms. |
| **D — entry points** | **D3**: signed, updatable seed list; start with one project node wearing three hats (relay, mailbox, bootstrap) | Every node cold-joined from a single multiaddr in every test. A small public VM (~CHF 5/month, unprivileged service, two open ports) sufficed. |

Lessons carried into the product:

- Budget relay bandwidth assuming punch failure for mobile peers.
- Default relay circuit limits are **120 s and 128 KiB per circuit** — enough for signaling,
  handshakes and shard exchange; bulk transfers need a direct path or raised limits.
- Unroutable punch candidates (loopback, virtual adapters, a relay co-hosted with a peer) waste
  the attempt budget; co-hosting a relay with a peer poisons that peer's candidate set.
- Reservation drop is not promptly observable at the relay: choreograph on timeouts, never on
  drop events.

### Seed list

**Status:** ◐ Partial — the setting is built and empty by default; signing and updating the list
are not built.

`seed_relays` is a list of relay multiaddrs, each ending in `/p2p/<relay-peer-id>`; malformed
entries are skipped, never fatal. Empty means LAN/mDNS only. (code:
crates/sovereign-core/src/config.rs, crates/sovereign-p2p/src/config.rs) Not settable in the
mobile app, which runs on the default configuration.

Specified, not built: the list is **signed and updatable** (D3), ships with project-run nodes,
and documents the path to community-run relays. A project seed node was deployed and
wire-validated in July 2026 [unverified: current operating status]; it is not in the default
configuration.

### What the relay sees

**Status:** ✅ Built — (code: crates/sovereign-relay/src/main.rs)

A relay learns routing metadata — which PeerIds connect, when, and how many bytes move — and
never keys or plaintext. Relayed connections are end-to-end Noise between the two peers;
mailbox blobs are sealed by the sender before deposit. Bootstrap and relay nodes never hold
keys.

### `sovereign-relay`

**Status:** ✅ Built — (code: crates/sovereign-relay/src/*)

A lean deployable binary (libp2p and `sovereign-core` wire types only — no database, crypto or
AI): relay-v2 server with default circuit limits, identify (`/sovereign/relay/1`), ping, and
the persistent mailbox. It listens on TCP and QUIC on one port (default 4001), keeps a stable
identity from a seed file so its seed-list entry survives restarts, and sweeps expired mailbox
items on a timer (default hourly).

### Store-and-forward mailbox

**Status:** ◐ Partial — built in the relay and the node; not yet used by the shell, the mobile
app or the guardian app.

Guardian heartbeats, recovery requests and approvals span days and cannot assume both ends are
online. The mailbox carries them. (code: crates/sovereign-core/src/mailbox.rs,
crates/sovereign-relay/src/mailbox_store.rs)

- **Put** deposits a sealed blob for a recipient PeerId; the sender is authenticated from the
  libp2p connection and cannot be spoofed. **Pull** drains every item addressed to the caller,
  identified by its authenticated connection; no peer can pull another's mail.
- Identical re-puts are de-duplicated, so a re-sent heartbeat heals itself rather than piling up.
- Persistent on disk; per-item TTL (14 days by default — the 72 h recovery window fits inside
  with margin); per-recipient item and byte caps; per-blob ceiling (a mailbox item is a control
  message, not a payload); per-sender rate and share caps; global byte and queue ceilings.
  Over-quota deposits are refused rather than evicting other senders' mail.
- The node exposes `MailboxPut` / `MailboxPull` commands and their events; nothing in the
  products issues them yet.

### Deposit capability

**Status:** ◐ Partial — mechanism built at both ends; dormant until the mailbox is activated.

A recipient-issued **deposit token** authorizes one named sender to deposit into one mailbox until
an expiry. It is signed with the recipient's libp2p key, so a public relay verifies it without
knowing anyone's social graph: the key's PeerId must equal the mailbox address, the signature
must verify, the token must be unexpired, and the authenticated depositor must be the named
sender. The relay enforces it only when started with `--require-deposit-token` (off by
default). Issuance is intended to happen during guardian enrollment, both ways (owner → guardian,
guardian → owner); that wiring is not built. (code: crates/sovereign-p2p/src/deposit_token.rs,
crates/sovereign-relay/src/deposit_token.rs, crates/sovereign-core/src/mailbox.rs)

### Hard barriers and rate limits

**Status:** ✅ Built — (code: crates/sovereign-p2p/src/node.rs)

- **Paired-only sync.** Manifest, commit, row and storage requests from a peer not on the
  paired allow-list are refused before the database is touched, and the node never starts sync
  with an unpaired peer. mDNS discovering a device grants it nothing.
- Pairing and guardian-enrollment handshakes are accepted from unpaired peers by design; each
  is gated by its own offer (below).
- Inbound sync requests from a paired peer pass a per-peer token bucket.
- Requests carrying lists (commit ids, commits, row ids, rows) are capped per request and
  refused before allocation when oversized.

## Pairing

**Status:** ✅ Built — interactive handshake since v0.0.7; per-device cryptographic revocation not built (below).

### Pairing offer

**Status:** ✅ Built — (code: crates/sovereign-p2p/src/pairing_offer.rs, pairing_client.rs; test: crates/sovereign-p2p/tests/e2e_pairing.rs)

The existing device shows a **plaintext offer** as a QR code: a random offer id, its PeerId,
dial addresses, its device name, issue time and expiry. The offer contains **no secret** — no
AccountKey, no salt — so photographing it yields nothing to attack offline. Dial hints exclude
loopback and unspecified addresses; the same predicate serves all four offer paths (pairing and
guardian enrollment, mobile app and shell).

When the QR cannot be scanned, the offer text is **copied and pasted** across devices.

*Replaces v1.x / v0.0.5:* the QR used to carry the AccountKey encrypted under a short PIN, which
left an offline brute-force artifact.

### Pairing code and handshake

**Status:** ✅ Built — (code: crates/sovereign-p2p/src/pairing_offer.rs, node.rs; crates/sovereign-crypto/src/pair_payload.rs)

- The existing device displays a **10-symbol pairing code** (Crockford-style alphabet, 50 bits,
  shown as `XXXXX-XXXXX`, case-insensitive). It never appears in any artifact; the user types it
  on the new device.
- Both sides stretch the code with Argon2id (64 MiB, t=2), salted by the offer id, into a
  handshake key *K*.
- The handshake runs over `/sovereign/sync/1`, dialed to the offer's PeerId so the transport
  itself authenticates the existing device:
  1. `PairHello` → `PairChallenge { nonce }`
  2. `PairProof` (MAC under *K* over offer, nonce and dialer) → `PairGranted` — the salt and
     AccountKey, AEAD-sealed under *K*
  3. The new device derives its **final** identity from the received salt, then
     `PairComplete` (MAC binding that final PeerId to the session) → `PairDone`
- On `PairDone` the existing device adds the new PeerId to its allow-list and pair-key map at
  once, so the first sync needs no app round-trip. The new device finds the existing one through
  the offer's addresses and mDNS together, so a stale address heals instead of timing out, and
  syncs immediately after pairing.

The code is proven **live**: there is no offline verifier for it.

### Offer lifetime and limits

**Status:** ✅ Built — (code: crates/sovereign-p2p/src/pairing_offer.rs, node.rs)

- **TTL 600 s.** Copy-paste plus typing a code across two devices takes longer than the old
  two minutes; the attempt cap, not the TTL, bounds guessing.
- **Three wrong proofs** destroy the offer; offers are single-use.
- One handshake session per offer; a session idle for **30 s** can be evicted, so a peer that
  opens a handshake and vanishes cannot lock out the real device.

### Paired-device store and pair keys

**Status:** ◐ Partial — per-pair keys and the encrypted store built; per-device cryptographic
revocation not built.

- `paired_devices.json` records each paired peer and its **pair key**, AEAD-encrypted at rest
  under a key derived from the device identity key. (code: crates/sovereign-p2p/src/pairing.rs)
- A pair key is derived from the AccountKey and the two sorted PeerIds, so both ends agree
  without an exchange. Traffic between one pair cannot be opened with another pair's key, and
  unpairing removes the peer from the allow-list and its key from the sealing map together.
  (code: crates/sovereign-crypto/src/account_key.rs)
- A peer with no pair key fails closed: nothing can be sealed for it or accepted from it.
- After access recovery (chapter 06) the device gets a new identity; its paired-device list is
  reset and devices must be paired again.

Specified, not built: per-device cryptographic revocation through a fresh key exchange per pair.

## Sync protocol

**Status:** ◐ Partial — all tables carried; title-only edits, document thread moves,
conversation titles and contact-address removals do not propagate; collaborative merge is
roadmap.

**Decided 2026-10-10 (Céline): the wire evolves by added fields, not by a protocol bump.** New
fields (document title and thread in the manifest hash, ownership in the snapshot, the missing
apply semantics) are added with defaults, so an older peer still talks to a newer one. Known
cost: until both devices are upgraded, the changed hashes never match and the affected items
are re-sent every round.

### Plaintext on the sync boundary

**Status:** ✅ Built — (code: crates/sovereign-p2p/src/sync_service.rs)

Each device owns its at-rest key hierarchy (Device Key → KEK → per-document keys, chapter 07);
ciphertext sealed on one device cannot be opened on another. Sync therefore reads the
**decrypted** state through the encrypting database layer, seals it under the pair key for the
wire, and the receiver **re-encrypts it under its own keys**. The Key Database never syncs.

*Replaces v1.x:* "Key Database … synced across devices via the P2P protocol" and "receives
wrapped Document Keys through authenticated P2P channels." No document key ever leaves a device;
the AccountKey (imported at pairing) is the only shared secret.

### When sync runs

**Status:** ✅ Built — (code: crates/sovereign-app/src/sync_startup.rs, crates/sovereign-shell/src/p2p.rs)

Sync starts when mDNS discovers a peer (the node ignores unpaired ones), right after pairing,
and on demand. The periodic 5-minute poll of all paired peers runs in the mobile app only; the
shell syncs on discovery, after pairing and on demand.
With `wifi_only` (default on Android, off on desktop), automatic sync is suppressed on cellular
data; an offline device triggers no automatic sync.

### Manifest

**Status:** ✅ Built — (code: crates/sovereign-p2p/src/protocol/manifest.rs, sync_service.rs, sync_engine.rs)

Peers exchange a **manifest** listing every syncable record, table by table:

- **Documents:** id, head commit, commit count, content hash, last modification, deletion
  marker.
- **Rows:** threads, entities, PII records, share records, contacts, messages, conversations,
  milestones, relationships and suggested links — each with id, the table's last-writer-wins
  timestamp, content hash and deletion marker. Share records, milestones and relationships are
  append-only.
- The manifest is AEAD-sealed under the account-wide **transport key** (derived from the
  AccountKey), so record ids, timestamps and sizes are invisible to the network and to relays.
  A peer that cannot produce a manifest under that key is dropped.
- A per-peer high-water mark on the manifest's generation time rejects replayed manifests.
- The diff compares content hashes first; only differing records move.

*Replaces v1.x:* manifests "encrypted with a shared device-pair key." Built: the account
transport key seals the manifest; pair keys seal the records it points to.

Specified, not built: padding the manifest to hide record count and size distribution.

### Documents

**Status:** ◐ Partial — content LWW built; the document hash covers content and deletion only,
so title-only edits do not travel, and thread moves of existing documents are not applied.
(code: crates/sovereign-p2p/src/sync_service.rs, sync_engine.rs)

- A document travels as its **current state** — title, content, thread membership and deletion
  marker in one sealed snapshot — not as a replayed commit chain. Commit history stays local to
  each device.
- Direction is **content last-writer-wins on modification time.** Equal times with differing
  content are broken deterministically by content hash, so both devices converge on the same
  winner, and the case is surfaced as a conflict (in the mobile app; see *Concurrent-edit
  conflicts*).
- Every snapshot is Ed25519-signed by the sending device and names it as author; an unsigned
  snapshot, a bad signature, or an author other than the verified sender is rejected.
  Timestamps beyond a bounded future skew are rejected.
- A document missing locally is created **under its origin id**, so it never duplicates on the
  next round.
- The manifest's document hash covers content and the deletion marker only. A title-only edit
  therefore leaves the hashes equal and does not travel; and for a document that already exists
  on the receiver, the snapshot's thread is not applied, so a move to another thread does not
  propagate.

*Replaces v1.x:* "Request missing commits only … apply commits using git-like merge." Encrypted
commit chains cannot cross the per-device key boundary.

### Rows: signed Lamport last-writer-wins

**Status:** ◐ Partial — signed Lamport LWW built for every row table; per-table apply is
partial: conversation-title changes and contact-address removals are not applied. (code:
crates/sovereign-p2p/src/sync_service.rs, version_store.rs, protocol/sync.rs)

- Every row envelope carries a **version stamp** `(counter, device)` from a per-device Lamport
  clock and an Ed25519 signature by the sending device over the canonical envelope (bound to its
  table, so it cannot be replayed into another).
- Conflicts resolve by the stamp, never by the forgeable wall-clock timestamp. Local edits are
  stamped lazily when a row's content hash changes; applying a remote row merges the clock.
- A row is rejected when unsigned or badly signed, when its author is not the verified sender,
  when its counter advances implausibly far beyond what this device has seen, or when its
  timestamp exceeds the future-skew bound. Replayed and stale rows lose the comparison.
- Stamps and per-row versions persist in a non-secret file (counters and hashes only).
- Applying a winning row goes through per-table setters, and not every field has one: a
  conversation's title change is not applied, and contact addresses are only ever added, so a
  removal does not propagate (the two copies' hashes then never converge).

### Deletions

**Status:** ◐ Partial — tombstones for soft deletes built; a hard delete leaves none. (code:
crates/sovereign-p2p/src/sync_service.rs)

Soft-deleted records stay in the manifest as **tombstones**, with their timestamp advanced to
the deletion time; the deleted state is part of the content identity. A peer still holding the
record live therefore receives the deletion instead of resurrecting it — including deleted PII.
A deletion applied from a peer is a soft delete (restorable) and is flagged for review like any
other peer change; a live winning state restores a locally deleted record.

A hard delete leaves no tombstone and is undone by the next sync: a peer still holding the record
sends it back.

### Thread membership

**Status:** ◐ Partial — a new document lands in its thread; a later move to another thread does
not sync. (code: crates/sovereign-p2p/src/sync_service.rs)

A synced document carries its thread, so it lands in its real lane when it is created on the
receiver. A snapshot from an older peer without the field lands in the default thread, shown
as "Unfiled".

### Ownership of synced documents

**Status:** ⚠ Conflict — decision pending (Céline).

A document synced from one's own paired device is treated as the user's own content (same
account) and is created **owned**. How ownership is decided for content arriving from outside is
chapter 04.

> ⚠ **Conflict — decision pending (Céline).** Every synced document is created owned, including
> documents that are external on the sending device; chapter 04 requires provenance to survive
> sync.

### Non-destructive peer overwrite and peer review

*Decided 2026-10-15 (Céline; C-WIRE below): the systematic review (flag on every change, the
review lists, the badge) is retired for changes from her own devices; the audit stays, runs on
every received change and raises an alert only on a detection; the overwrite preservation stays
(History, previous values). This section describes the build until step 4 lands.*

**Status:** ◐ Partial — overwrite preservation built; the shell never runs the audit; both UIs
offer restore for 3 of the 7 restorable tables. (code: crates/sovereign-p2p/src/sync_service.rs;
crates/sovereign-ai/src/peer_audit.rs; crates/sovereign-app/src/tauri_commands/peer_review.rs;
crates/sovereign-shell/src/panels.rs)

A paired device is trusted to sync, not to silently replace. Every overwrite from a peer
preserves what it replaced and is surfaced:

- **Documents:** the current local version is committed first; the peer's content is applied;
  the document is flagged `peer_review_pending` with the peer, time and prior commit.
- **Rows:** before an overwrite, the prior row is captured, sealed under the account transport
  key, and stored as a `RowRecovery`. Restore is wired in the sync layer for threads, entities,
  PII records, contacts, messages, conversations and suggested links; both UIs offer it only for
  threads, contacts and PII records. Append-only tables are never overwritten.
- **Audit:** each pending change is scored — a model-free heuristic (prompt-injection scan of the
  incoming content, wholesale-rewrite check against the prior) always, plus an LLM verdict when a
  model is loaded. The audit annotates; it never gates the sync. Only the frozen Tauri
  desktop ran it: the shell never runs it, and the mobile app has no model to run it with (ch. 08).
- **Review surface:** the shell (`r`) and the mobile app list pending changes; the user keeps the
  synced version or restores the prior one. A `RowsFlaggedForReview` event badges the panel when
  it happens.

*Replaces v1.x:* the `pending_edits` conflict queue and its approval/rejection flow.

### Concurrent-edit conflicts

**Status:** ◐ Partial — surfacing and "keep mine" built; "keep theirs" and "keep both" not
built.

A `SyncConflict` event surfaces equal-timestamp divergence. **Keep mine** (touches the document
so it wins the next round, then syncs) and Dismiss are offered in the mobile app; the shell drops
the event. "Keep theirs" and "keep both" need a fetch-document verb that does not exist.

### Git-like merge and `pending_edits`

**Status:** ✗ Retired — replaced by last-writer-wins with non-destructive overwrite and peer
review (above).

Branch-and-merge conflict markers, auto-merge of disjoint sections, and creator/admin approval of
merges are not part of device sync.

### Collaborative merge (CRDT)

**Status:** ○ Specified, not built — roadmap.

Multi-user, concurrent editing of one document by several people with merge rather than
last-writer-wins. Device sync between one user's devices does not need it.

## Sync wire — contract (C-WIRE)

**Status:** ○ Draft 2026-10-15, amended the same day with Céline's answers to all five questions
(devices on an older build, purges, backup restore, "Keep mine", review of her own devices'
changes). Technical choices marked **[choice]**. No test and no code yet.

**Why.** Documents are now written by one door, the document service (ch. 03, C-DOC), which keeps
v2 content, versions, trust, trust decisions, provenance, the trash and purge markers. Sync still
writes around it (C-DOC adoption, step 4): it compares and sends the **stored bytes**, decides
direction by **wall-clock** modification time, creates every arriving document **owned** with
provenance *unrecorded*, and sends neither trust, the user's trust decisions, provenance (copies
included) nor purge markers. So an undetermined import becomes owned on her other device, a
trust decision made on the phone never reaches the desktop, a purge is undone by the next sync,
and a document stored in v2 on one device and in a legacy form on another never compares equal.
This contract fixes what travels, how two states are compared, and how a remote state is applied
— through `DocumentService::apply_remote` and nothing else.

**How the wire evolves (K8, decided 2026-10-10).** By **added fields with defaults**, no protocol
bump. Every field below that is new is optional on the wire; a peer that does not send it is
treated as described in *Peers on an older build*. Every new field travels **inside** the sealed,
signed envelope (the document snapshot, the AEAD-sealed manifest); nothing new is sent in clear.

### What travels for a document

The snapshot (sealed under the pair key, signed by the sender) carries:

| Field | Today | Contract |
|---|---|---|
| title, thread, trash marker | sent | sent |
| content | the stored bytes | the stored v2 content; a newer-format document verbatim |
| trust | — | **new**: owned / external / undetermined |
| trust decision | — | **new**: the user's last explicit decision — trust, device, and its **stamp** (below) |
| provenance | — | **new**: written, assistant, web + URL, import + path, paired device, unrecorded, or *copy of <id>* carrying the source's provenance (nested, as stored — ch. 03 G20) |
| created at | — | **new**: the document's original creation time, so it keeps its place on the timeline |
| stamp, previous stamp | — (wall-clock `modified_at`) | **new**: the stamp of the write that produced this state, and of the state it replaced (below) |
| author device | — (the sender) | **new**: the device on which the write was made, when the sender only relays it **[choice]** |
| modification time | sent | sent, for display and for peers on an older build only |

The manifest (sealed under the account transport key) gains, per document, the **normalised
hash** and the **stamp**; it keeps today's byte hash for older peers. It gains a list of **purge
markers** — document id, the stamp of the purge, and the **fingerprint** of the purged version
(*Deletions and purges*), nothing else — and a **wire version** number so a peer on an older build
can be recognised.

### Stamps instead of the wall clock **[choice]**

Every write to a document, and every trust decision, gets a **hybrid logical clock** stamp:
wall-clock time, a counter, and the device — ordered by time, then counter, then device. A device
never issues a stamp lower than one it has seen, so a later write always carries a greater stamp
even when the clocks disagree; a received stamp more than **5 minutes** ahead of the receiver's
clock is refused, as manifests already are (today a document is accepted up to 24 hours ahead).
The rows already use Lamport stamps (*Rows* above); documents and trust decisions join them. This
closes the C-DOC weakness that a device whose clock runs ahead wins every disagreement. The
displayed times stay wall-clock.

### Comparing two states

Two states of a document are equal when their **normalised forms** are equal: the title, the
thread, the trash state, the trust, the trust decision, the provenance, and the content **as v2**
— a legacy form converted with the deterministic migration seeded by the document id (ch. 03, G3),
so the same legacy document is the same on every device whether or not it has been migrated. The
normalised hash is computed by the document service (`sync` never parses content, per the readers
gate). A title-only edit or a thread move changes it, so both now travel (they do not today).

### Direction and concurrent edits

- **Fast-forward.** A remote state whose *previous stamp* is the local state's stamp replaces it;
  a local state whose previous stamp is the remote's is sent. Otherwise the edits were concurrent.
- **Concurrent edits keep both, without asking** — the rule Céline decided for save conflicts on
  2026-10-12, applied across devices: the state with the greater stamp is current on **both**
  devices; the other state is kept as a version on **both** (the losing device versions its own
  state before applying, as today; the winning device receives the losing state and stores it as
  a version without making it current); both devices show a notice. Nothing is asked unless the
  user acts on the notice.
- With three or more devices, a "concurrent" verdict can be a false alarm (a relayed state whose
  previous stamp is unknown here); the cost is an extra version and a notice, never a loss
  **[choice]** — full version vectors are not worth it for one user's devices.

### Applying a remote state

Every remote document state goes through `DocumentService::apply_remote` (C-DOC G19): the local
state is versioned first, ingest runs (G10), trust moves only with a later decision (G12), a purged
id is handled as in *Deletions and purges*. Sync keeps no write of its own to documents: no store
update, commit, create, soft delete or restore from `sovereign-p2p`.

**No systematic review; an alert only when something looks wrong** (decided by Céline,
2026-10-15). A change arriving from one of her paired devices applies without asking; the state it
replaced is in History, as for every write. Every received document change still goes through the
**incoming-change analysis** (`peer_audit.rs`): the deterministic scan of the new content for
injected instructions, the check for a wholesale rewrite of the previous text, and — where a model
is loaded — the model's verdict. It is **silent when nothing is suspicious** and **raises an alert
only on a detection**, pointing at the document and at its previous version in History. Her
reasoning: security belongs first at device pairing and at data sync — the pairing ceremony, pair
keys, sealed and signed envelopes, authorship checks — but a paired device can still be
compromised (stolen, or carrying a trapped page saved on it); she never re-reads her own gestures,
only the rare alerts. Consequences:
- **retired:** the per-change review flag as a queue (`peer_review_pending` set on every change),
  the shell's review list (`r`), the mobile review list, and the `RowsFlaggedForReview` badge;
- **kept:** `peer_audit.rs` — now run on every received document change rather than on a review
  queue. **The deterministic checks run at once, during sync, on every device** (mobile
  included, it has no model). **The model's verdict runs afterwards, in the background, for every
  received document — first arrivals included — and never makes sync wait** (decided by Céline,
  2026-10-15: on a machine with a model loaded, a first sync of hundreds of documents would
  otherwise queue hundreds of model calls ahead of sync and of the user's chat). A background
  verdict that finds something raises the same alert;
- **the alert surface** **[choice]**: in the **shell**, a notice in the status bar ("A change from
  <device> to <document> looks suspicious") that opens an *Alerts* window listing each detection —
  the document, the device, what was found — with *Open the document* and *Open the previous
  version*; in the **mobile app**, the same notice as a banner over the canvas, opening the same
  list in a sheet. An alert stays listed until she dismisses it; dismissing changes nothing in the
  document. Restoring the previous version is the ordinary version restore ("Use this version");
- what the review protected stays recoverable: a document's prior state is a version (History); a
  row's prior value is still kept by sync (`RowRecovery`, sealed) and restorable from a list of
  *previous values*, without a pending badge **[choice]** — rows are not analysed (the scan reads
  document text);
- C-DOC G19 ("the document is flagged for review") changes accordingly: the flag becomes an alert
  raised only on a detection.

**What `apply_remote` lacks today**, to add in the contract before the tests (not added here):
1. the original **creation time** (a document arriving today is dated by the sync);
2. the **stamps** — the service decides with the wall-clock `modified_at`; trust decisions carry a
   wall-clock `decided_at`;
3. storing a remote state **as a version without making it current** (the winning side of a
   concurrent edit);
4. receiving a **purge marker** from a peer, with its fingerprint (*Deletions and purges*);
5. the **author device** distinct from the relaying sender;
6. an outcome that says the edit was **concurrent** (for the notice) and whether **ingest changed
   the text** (so the rewritten state travels back);
7. the **normalised hash** and the **purge fingerprint** as service functions, and a way to send the
   stored state (with trust, decision, provenance, creation time, stamps) without the sync layer
   reading `content`;
8. a **backup restore** entry (below);
9. **row tables** are out of the service: conversation titles and contact-address sets stay with
   the row apply (below).

### Deletions and purges

- **Every deletion a user makes is a trash** (C-DOC G13) and travels as a tombstone; applying it
  moves the document to the trash on the other device, restorably; a live state with a greater
  stamp brings it back (G19). There is no hard delete left to escape this.
- **A purge removes the other device's copy only if it is the same version** (decided by Céline,
  2026-10-15). The purge travels as a marker carrying the **fingerprint** of the version that was
  purged. A device that holds the document — live or in its trash — purges it too **only if its
  copy has the same fingerprint**. If its copy differs (edited there since, or otherwise
  different), it is **kept** and synchronised as usual: it comes back to the device that purged
  it, edits included. To be rid of it, the user deletes it again on either device; that purge
  then removes it from both, since the copies are now the same.
- **The fingerprint** **[choice]** is a hash of the purged version's **title and content** (the
  content as normalised v2, ch. 03 G3), and nothing else: not the thread, trash state, trust or
  times. A purge erases what was written; a copy that differs only by its lane, its trash state or
  a trust decision holds nothing that would be lost, so it is purged. The marker carries the hash,
  never the content.
- **A purged id comes back only as a differing copy.** A device that purged a document ignores a
  peer's state for it with the purged fingerprint, and **recreates** it from a state with a
  different fingerprint — through the service, as a new arrival (its own first version, the
  sender's trust and provenance, in the trash if the copy was in the peer's trash); the purge
  marker is then dropped there. This is the one exception to C-DOC G13 ("a purged document is
  never brought back by sync"), which is amended to say so.
- A device that never had the document records the marker and refuses an identical copy from then
  on.

### Peers on an older build

**Documents are not exchanged with a device on an older build until it is updated** (decided by
Céline, 2026-10-15), and both screens say so ("Update <device> to sync documents"). Threads,
contacts, messages and the other rows keep syncing. The reason: an older build cannot read v2
content — its tolerant read shows a v2 document as the raw JSON text, and an edit there would save
that text back as the document.

A peer is on an older build when its manifest carries no wire version. The new build sends it no
document state and applies none from it; documents written on the old device stay there until it
is updated, then sync normally. The v2 format is already written by this build, so "keep writing
the old form until every device is updated" is not an option.

### Backup restore

**A backup is restored only onto an empty workspace** (decided by Céline, 2026-10-15): a new
device, or an account just recovered with the guardians. The guardian backup is the sealed copy of
the whole workspace your guardians hold, for when every device is lost. A device that still holds
documents never restores a backup: it catches up with your other devices by sync. A restore on a
non-empty workspace is refused, with the reason ("this device already holds documents; it gets the
others by sync").

On an empty workspace, the restore goes through the service, document by document: each lands with
one version ("Restored from backup"), its trust, decision, provenance and times as backed up. There
is nothing local to conflict with, so everything in the backup comes back. **The purge rule there:**
an empty device knows no purge, so a document purged after the backup was taken comes back with
it; the backup carries the purge markers it knew of **[choice]**, which keep a document purged
before the backup from returning by sync later. When the restored device next syncs with another
device that still holds a later purge marker, the rule of *Deletions and purges* applies: a copy
identical to the purged version is purged again; a differing one is kept. If no other device
survives, what the backup held is what comes back.

**For the rare wish to see a backup next to existing data** (Céline's fallback): restore it into a
second, empty profile, export what is wanted from there and import it into the first. That depends
on the **general export**, decided 2026-08-04 (ch. 04, *Export*) and **not built**; until it is,
this fallback does not exist.

### "Use this version" replaces "Keep mine"

**Decided by Céline, 2026-10-15.** The mobile "Keep mine" (a write that changed only the
document's time so it would win the next round) is replaced, on mobile and in the shell, by **"Use
this version"**, offered on the concurrent-edit notice and in History: a version restore (C-DOC
G9), a normal, later write that syncs like any edit. No "touch" write exists in the contract (an
identical save does nothing, G11).

### Rows

The row apply (*Rows* above) gains, by added fields: a **conversation's title** applied when it
changes; a **contact's addresses** applied as a set, removals included (both hashes then converge);
one shared list of **restorable tables** used by sync and by both *previous values* lists, so every
table sync can overwrite is one both UIs can restore. Trust of entities and contacts is out of this
contract.

### Guarantees — each to become a test

1. **Everything that defines a document travels**: title, thread, content (v2; a newer format
   verbatim), trash state, trust, the last trust decision with its stamp, provenance (copies
   included), creation time, stamps; all inside the sealed, signed envelope.
2. **A missing field never overwrites a known one**: a peer that does not send trust, decision,
   provenance, creation time or stamps leaves the local values as they are, and a document new
   here gets the safe defaults (undetermined, *paired device*).
3. **Equal means equal after normalisation**: two devices holding the same document — one
   migrated to v2, one still legacy — compare equal and exchange nothing; a title-only edit, a
   thread move, a trust decision or a trash change makes them unequal.
4. **Order is by stamp, not by clock**: a later write wins whatever the two devices' clocks say;
   a stamp more than 5 minutes ahead of the receiver is refused.
5. **Convergence**: after both directions of a round, both devices hold equal states; when the
   receiver's ingest rewrites the text, the rewritten state travels back once and a further round
   with no edits exchanges nothing (no loop).
6. **Nothing is lost**: every local state a remote state replaces is a version first, on every
   path (overwrite, tombstone, revival, recreation after a purge).
7. **Concurrent edits keep both without asking**: both devices end with the same current state
   (the greater stamp) and the other state in History, and both show a notice.
8. **Trust follows decisions only** (C-DOC G12): a later decision applies in either direction; no
   other remote difference changes trust; a document new here never arrives owned without a
   sender trust or decision that says so (closes "synced documents always land owned").
9. **Tombstones**: every user deletion reaches every paired device as a trash, restorable; a later
   live state revives.
10. **A purge removes only the same version** (Céline, 2026-10-15): a device whose copy has the
    purged fingerprint purges it, wherever it sits; a device whose copy differs keeps it, and that
    copy comes back to the purging device as a new arrival; a second purge of now-identical copies
    removes them everywhere; the marker carries no title or content, only the id, stamp and
    fingerprint.
11. **Only the service writes documents**: no store write to documents from `sovereign-p2p` (the
    write gate's step-4 entries removed and its patterns extended to soft delete and restore).
12. **Older peers exchange no documents** (Céline, 2026-10-15): a peer without a wire version is
    sent no document state and none of its document states is applied; both screens say which
    device to update; rows sync with it as before.
13. **Alerts, not reviews** (Céline, 2026-10-15): a document change received from a paired device
    is analysed every time; an innocent change applies silently (no flag, no alert) with the
    replaced state in History; a suspicious one (injected instructions, a wholesale rewrite, or a
    model verdict against it) is applied too and raises exactly one alert naming the document,
    the device and the previous version. Rows raise no alert; their previous values stay
    restorable.
14. **"Use this version"** is a version restore that travels like any edit; nothing writes a
    "touch".
15. **Rows**: conversation titles and contact-address removals converge; every table sync can
    overwrite is restorable in both UIs.
16. **Nothing new in clear**: every added field is inside a sealed envelope; the signatures cover
    them.
17. **A backup restores only onto an empty workspace** (Céline, 2026-10-15): on a workspace that
    holds any document, the restore is refused, writes nothing and says why; on an empty one,
    every backed-up document comes back through the service with one "Restored from backup"
    version, its trust, decision, provenance and times, and the purge markers in the backup are
    recorded.

All of Céline's questions on this contract are answered (2026-10-15).

## Known limits

**Status:** ◐ Partial — named limits of the built transport.

- Sync over mobile data is unreliable (v0.0.8 known issue). The M0 measurements explain it:
  without a reachable relay, mobile peers behind carrier NAT have no path, and the default seed
  list is empty.
- The mobile app cannot be given a relay: the seed-relay setting is not exposed there, so off
  the LAN it has no path to the user's other devices.
- Unroutable punch candidates are not filtered before hole punching (offer dial hints are).
- Bulk transfers through a relay are bounded by the relay's circuit limits.
