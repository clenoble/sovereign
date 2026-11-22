# 05 — Sync & P2P

How a user's own devices find each other, reach each other across networks, pair, and
converge on the same workspace — with no server that can read anything. Recovery through
guardians, which rides the same transport, is chapter 06.

*Consolidates:* specification v1.2/v1.3 §P2P Storage & Guardian Protocol (Three-Layer
Architecture, Device-to-Device P2P Sync, Sync Protocol), §Version Control (conflict
resolution), §Encryption & Key Management (in-transit); the M0 transport spike (findings log
and filled decision matrix, branch `spike/m0-transport-discovery`, 2026-07-07/08); the
project's archived plans for P2P completion, backup and the guardian app (private); release notes
v0.0.7–v0.0.9; the code at `main` d02c353.

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
- **The DNS transport is optional** (v0.0.12 item 12). The node reads the system resolver
  config once, at start. Where there is none (Android: no `/etc/resolv.conf`, its resolver is
  `netd`), the swarm is built without the DNS layer and starts anyway; one startup line says so.
  LAN discovery, sync with paired devices on the local network and peers addressed by IP work as
  on the desktop. An address that needs a name (`/dns`, `/dns4`, `/dns6`, `/dnsaddr`) is never
  dialled, by `Dial` or by the seed relays, and no name goes to a third-party resolver; the reason
  is logged once per node. Settings → Devices shows: "This phone cannot look up names: it syncs
  on the local network and with addresses given by IP." Resolving names through the system's own
  resolver (`getaddrinfo`) is later, when seeds given by name are needed on the phone.
  (code: crates/sovereign-p2p/src/node.rs `new_with_dns_source`, `has_dns_transport`)

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
- **PO1 — Closing the offer's window ends the offer, on both hosts** (v0.0.13; the Fable maille, by
  delegation of Céline (2026-11-14), on 2026-11-19). **Built in v0.0.13** on the desktop (merged
  2026-11-19); the app already disarmed it when its panel closed. On the desktop, the pairing window closed by « Done », a click
  outside it, Escape or Enter disarms the offer on the node, so a code seen while it was open cannot
  be used after she closed it. A proof that arrives once the offer is disarmed is refused. After a
  pairing that succeeded, closing has nothing left to disarm. Nothing is said to her: closing the
  window is what ends it. The duress session's decoy offer arms nothing, and its closing disarms
  nothing.

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
- **PR1 — Pairing a device again replaces its record, on both sides.** **Built in v0.0.13** (item C3,
  merged 2026-11-18): the desktop and the app ask at the pairing, and « Replace it » removes the old
  record as a « Forget » does. After a device is paired again with a device it was paired with, each
  side's paired list holds one record for the other: the new one. The old record is not kept, and the
  device shows once in the device list, History, alerts and orphans.
  - **The joining side** holds this already: the source keeps its peer id, and the list is keyed by
    peer id.
  - **The source side** (the device that showed the offer) is where it can fail: a device paired
    again after a new setup comes back with a new peer id, derived from its new passphrase, and is
    added under that id. The old record stays beside it, as a device that never syncs again (the
    tests of v0.0.13 found it so); PR2 is what removes it.
  - **The old record's pair key goes with it:** the old peer id leaves the allow-list and the sealing
    map, as at unpairing.
  - **History's earlier rows from that device** read as a forgotten device's, « … » and its old id's
    last 8 characters (ch. 12, HN1): nothing links the old id to the new one. The rows written after
    the new pairing carry its name.
  - **PR2 — The source side asks.** The pairing carries the joining device's name, which two devices
    can share, and nothing else links the old id to the new one. So when a device joins under a name
    a paired device already has, the source side asks whether it is that device, set up again:
    « Replace it » or « Keep both ». « Replace it » removes the old record as above; « Keep both », or
    no answer, keeps both records. **Nothing is replaced without her answer.** With several paired
    devices of that name, the question lists them, and she picks one or keeps all (the Fable maille,
    by delegation of Céline (2026-11-14), on 2026-11-18).

Specified, not built: per-device cryptographic revocation through a fresh key exchange per pair.

### Device names

**Status:** ✅ Built on the desktop since v0.0.12 (items 14 and 19, merged `c7a78e4`, 2026-10-24).
The phone's device name is a later item. (code: crates/sovereign-shell/src/device_name.rs,
device_rename.rs, history_device.rs)

A device's name is a label for people. It is not its identity: the device in a stamp is its peer
id (*Stamps*, item 22), and renaming a device never changes what is stored or written next. Until
v0.0.11 the name came only from `[p2p] device_name` in the configuration file, "Sovereign Device"
by default, with no setting in the UI, so two devices were easily indistinguishable. Céline put
both items in on 2026-10-23; on 2026-10-24 she approved the choices PANDA2 put to her ("je suis
d'accord avec tes choix"). The choices below marked PANDA2's are the PANDA2 maille's.
- **One name per workspace.** This device's name is kept in a file of its own in the data
  directory, read at start, and it overrides the configuration's value; without it the
  configuration's value is used, then "Sovereign Device". It is the device's name, not an
  account's: both personas read and change the same one (PANDA2's choice). So renaming the device
  in a duress session renames it for the primary too; nothing in that reveals the duress persona.
- **At setup.** The first-device wizard asks for it on the step that names the AI, in a second field
  "This device's name", and the join form in a fifth field (PANDA2's choice). It is prefilled with
  the computer's name, the operating system's host name (Céline, 2026-10-24: "Le nom de
  l'ordinateur"), and editable; under it: "Your other devices and your guardians see this name."
  A blank entry means the default. The name is trimmed and kept to at most 64 grapheme clusters,
  never cut inside one (PANDA2's choice). The join sends it in `PairHello`.
- **Renaming this device.** The Devices window's "This device" line has a Rename action: the
  editor's single-line field (Enter saves, Escape cancels, a blank entry is refused with a
  message). The new name is saved at once and is what this device sends from then on, at the
  next pairing and in a guardian offer, without a restart.
- **The name travels only at pairing**, and the protocol is unchanged: devices already paired keep
  the name they learned. The Devices window says so under this device's name: "Devices already
  paired with this one keep the name they learned until you pair them again."
- **Renaming a paired device, here only.** Each paired device's row has a Rename action with the
  same field and rules. The new label replaces that device's stored name in this device's paired
  list, so every place that names it uses it: History, alerts, orphans (since item 22, which made
  stamps carry the peer id the paired list is keyed by), the update notice and the device list. It
  is not sent anywhere. This removes the "two Sovereign Device" ambiguity on each device without a
  protocol change.
- **History names the device** (item 14). A version received from a peer is stored with the
  message "Synced from {id}" or "Received from {id}", the id being the peer's. The History window
  shows, and announces, the device's name from the paired list instead; "this device" for its own
  id; for a device no longer paired, "…" and the id's **last** 8 characters (every peer id starts
  with the same 8). The stored message is unchanged, so older versions are shown the same way;
  other messages are shown as they are. In v0.0.13 this extends to every row that names a device or
  a version, on the phone too (ch. 12, *History on both hosts*; ○ specified 2026-11-18). A row's
  message is one line, cut with "…" at the row's
  width, never drawn over the time line under it; the full message stays in the row's accessible
  text.
- **Accessibility.** The rename fields are text inputs named "This device's name" and "Name of
  {current name}"; the Rename actions are buttons named "Rename this device" and "Rename {name}";
  the setup field is named by its visible label.

Out of these items: trash rows also wrap over the next row (PANDA2's choice to leave them for a
later lot); the phone's device name.

## Sync protocol

*The target (D17 and D11, Céline, 2026-11-10; ch. 01, Direction):*
- a journal kept by each device and exchanged directly, with the node as one more blind replica, never a
  required passage;
- a sealed journal shared by sync, backup and mail, which is a proposal to write.

*Decided 2026-11-13 (Céline; ch. 01, P6, P7, P10, P12, P19):*
- **The node** is an optional add-on. It is blind, or authorised by a delegation (ch. 07). Hosted by a
  third party, it stays strictly blind. The project never operates one.
- **Each type of data has its merge rule,** additive or ordered by stamp.
- **One device is a complete Sovereign,** and the journal works entirely locally.

*What is below is the protocol as built.*

**Status:** ◐ Partial — all tables carried; documents travel by the C-WIRE contract below since
v0.0.11, so title-only edits travel, and thread moves too since v0.0.12 (*Moving a document*);
conversation titles and contact-address removals propagate since v0.0.11 (C-WIRE *Rows*);
collaborative merge is roadmap.

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
and on demand. The Tauri app (the phone, and the frozen Tauri desktop when built with P2P) also
polls all paired peers: every 5 minutes, every 15 minutes after three rounds that started no
sync, hourly after five, and back to 5 minutes once a round starts one. (code:
crates/sovereign-app/src/lib.rs) The shell does not poll: it syncs on discovery, after pairing and
on demand.
With `wifi_only` (default on Android, off on desktop), automatic sync is suppressed on cellular
data; an offline device triggers no automatic sync.

### The node's supervision — contract (C-SUP)

**Status:** ✅ Built for v0.0.13 (lot LQ), on the shell and the phone app; SU1-SU6 are held by tests
on both hosts, except two points of SU2 that are read in the host and not tested: which notice shows
when a login also has the setup's to show, and the app's screens asking the stop's state before the
backend. Decided by Céline on 2026-11-13 (« Lot avant rc1 + surveillance »): the P2P node's
end, by a panic or not, is shown, with a restart. Not built: the shell has no logout, switch or
lock yet, so its asked end is the application's exit only (SU5 is held at the supervisor); the
lock of C-SESS will be another end.

**Guarantees (C-SUP, each one a test):**
- **SU1 — The node's end is observed,** whether its task returns, returns an error or panics. It is
  never silent.
  - **A start that fails is said as a stop** (Céline, 2026-11-16): a session whose keys are open, whose
    node was tried and did not start, shows what SU2 shows for a stop, with the same words and the same
    gesture. The reason goes to the log, and to the user only if a restart fails in its turn. The shell's
    session opened without a login has no keys and tries no start: it is not this case. On the app, a key
    missing at the start is an error of the start, and is said as one.
  - **The stop reaches the screen after the login has answered:** the interface's reset at login
    (C-AREA AR4) never clears a stop of the session it opens.
- **SU2 — The host says that sync stopped,** in its status line and once as a notice, on each host that
  runs a node: the shell, and the app built with P2P. The words (the Fable maille, by delegation of
  Céline, 2026-11-15), the same on both hosts:
  - the status line: « Sync stopped », first, then what the line would say without it: « Sync stopped;
    {the rest} ». An alert still open is still shown (the Fable maille, by delegation of Céline,
    2026-11-16);
  - the notice, once per stop: « Sync stopped unexpectedly. Nothing syncs until you restart it. »;
  - the gesture: « Restart sync », present wherever the notice is shown;
  - the button that closes the notice on the app: « Dismiss » (the Fable maille, by delegation of
    Céline, 2026-11-16);
  - a restart that fails: « Sync could not be restarted: {reason} ».
  - Why it stopped goes to the log, not to the user.
  - **While the status line says « Sync stopped », a gesture that needs the node shows the stop's
    notice again,** with its gesture, on both hosts: never « Log in to start » nor « enable sync … first ».
    « Once per stop » means once by itself: an answer to the user's gesture is not the notice coming back
    by itself (the Fable maille, by delegation of Céline, 2026-11-15).
  - **When a login also has the setup's notice to show** (on the shell, which shows one notice at a time),
    the setup's notice is the one shown: it lists what was not saved, said nowhere else. The stop stays in
    the status line, and its notice comes at the first gesture that needs the node (the Fable maille, by
    delegation of Céline, 2026-11-16).
- **SU3 — A restart is a gesture of the user's,** and builds a fresh node with the open session's keys.
  Nothing restarts the node by itself in v0.0.13 (the Fable maille, by delegation of Céline,
  2026-11-15): a retry would hide a stop that repeats. If stops prove frequent in use, a bounded
  automatic restart that says each retry comes next.
- **SU4 — Supervision never starts a node where none may run:** never in a duress session (C-AREA,
  AR4), and a switch or a lock ends the supervision with the node.
  - **A stop ends with its session** (the Fable maille, by delegation of Céline, 2026-11-15; C-AREA AR4):
    after a logout, a switch or a lock, the next session starts as if sync had never stopped, and shows
    nothing of the previous stop.
- **SU5 — An end the session asked for is not a stop:** a logout, a switch, a lock or the application's
  exit end the node and say nothing. Only an end the host did not ask for is said.
- **SU6 — Each host calls the supervision,** and the tests hold the call, not only the supervising
  function: a « built » here is read against a caller.

### Manifest

**Status:** ✅ Built — (code: crates/sovereign-p2p/src/protocol/manifest.rs, sync_service.rs, sync_engine.rs)

Peers exchange a **manifest** listing every syncable record, table by table:

- **Documents:** id, **normalised hash** and **stamp**, with the previous stamp and a bounded
  list of earlier ones (C-WIRE, *What travels for a document*); beside them, the **purge
  markers** (id and deletion stamp) and the manifest's **wire version**. The entry builds before
  v0.0.11 sent (head commit, commit count, content hash, last modification, deletion marker) is
  no longer sent.
- **Rows:** threads, entities, PII records, share records, contacts, messages, conversations,
  milestones, relationships and suggested links — each with id, the table's last-writer-wins
  timestamp, content hash and deletion marker; each stamped thread also with its stamp (*Rows*,
  below). Share records, milestones and relationships are append-only.
- The manifest is AEAD-sealed under the account-wide **transport key** (derived from the
  AccountKey), so record ids, timestamps and sizes are invisible to the network and to relays.
  A peer that cannot produce a manifest under that key is dropped.
- A per-peer high-water mark on the manifest's generation time rejects replayed manifests.
- The diff compares content hashes first; only differing records move.

*Replaces v1.x:* manifests "encrypted with a shared device-pair key." Built: the account
transport key seals the manifest; pair keys seal the records it points to.

Specified, not built: padding the manifest to hide record count and size distribution.

### Documents

**Status:** ✅ Built — since v0.0.11 a document is compared, ordered and applied by the C-WIRE
contract below (normalised hash, stamps, `apply_wire`), so a title-only edit travels; a thread
move travels since v0.0.12 (*Moving a document*). A peer on a build older than v0.0.11 exchanges
no documents with this one (*Peers on an older build*). (code: crates/sovereign-p2p/src/wire.rs,
sync_service.rs, sync_engine.rs)

- A document travels as its **current state** — title, content, thread membership and deletion
  marker in one sealed snapshot — not as a replayed commit chain. Commit history stays local to
  each device.
- Direction is decided by **stamps**, not by the wall clock (C-WIRE, *Direction and concurrent
  edits*). Until v0.0.11 it was content last-writer-wins on modification time.
- Every snapshot is Ed25519-signed by the sending device and names it as author; an unsigned
  snapshot, a bad signature, or an author other than the verified sender is rejected.
  Timestamps beyond a bounded future skew are rejected.
- A document missing locally is created **under its origin id**, so it never duplicates on the
  next round.
- The manifest compares documents by their **normalised hash**, which covers the title and the
  thread among the rest (C-WIRE, *Comparing two states*), so a title-only edit travels, and a move
  to another thread travels as a stamped write (*Moving a document*, v0.0.12). Until v0.0.11 the
  hash covered content and the deletion marker only, so neither travelled; a peer still on such a
  build is sent no document at all.

*Replaces v1.x:* "Request missing commits only … apply commits using git-like merge." Encrypted
commit chains cannot cross the per-device key boundary.

### Rows: signed Lamport last-writer-wins

**Status:** ✅ Built — signed Lamport LWW for every row table but threads, which are ordered by
their own stamp (below); since v0.0.11 a conversation's title change and a contact's address
removals are applied too (C-WIRE *Rows*, guarantee 15). (code:
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
- Applying a winning row goes through per-table setters. A conversation's title is applied when
  it changes, and a contact's addresses are applied as a set, removals included, so both copies'
  hashes converge (since v0.0.11; until then the title was not applied and addresses were only
  ever added).
- **Thread rows are ordered by the thread's stamp**, not by the Lamport counter (decided by
  Céline, 2026-10-18: the later action wins; built and released in v0.0.11). Every thread write
  — create, rename, delete, restore, on every path: the shell, the assistant, the phone, skills,
  import, backup restore, the decoy's seeding — goes through the document service's one
  thread-write path, which stamps it from the same device clock as documents (*Stamps instead of
  the wall clock*). The stamp travels in the row, a field added with a default. For the same
  thread the greater stamp wins, whatever the two actions are, and decides the direction of the
  exchange, so two devices converge. A row without a stamp (written by an older build) is
  compared as if stamped at its modification time (the later of its modification and deletion
  times), counter 0, its device, within the same 5-minute bound; so an older device's later
  rename can still win. A **deleted** thread with a stamp is compared on its id, its deleted state
  and its stamp only (its name and description are not hashed), so a thread shrunk to its id on
  one device and a named tombstone on the other are not re-sent every round. What a rename or a
  deletion that wins does here, and the shrink after 30 days: ch. 03, *Thread deletion and the
  fallback lane*.

### Deletions

**Status:** ◐ Partial — tombstones for soft deletes built; a hard delete leaves none. (code:
crates/sovereign-p2p/src/sync_service.rs)

Soft-deleted records stay in the manifest as **tombstones**, with their timestamp advanced to
the deletion time; the deleted state is part of the content identity. A peer still holding the
record live therefore receives the deletion instead of resurrecting it — including deleted PII.
A deletion applied from a peer is a soft delete (restorable), and what it replaced is kept like
any other peer overwrite (*Non-destructive peer overwrite*); no review flag is raised (retired
2026-10-15). A live winning state restores a locally deleted record.

A hard delete leaves no tombstone and is undone by the next sync: a peer still holding the record
sends it back.

### Thread membership

**Status:** ✅ Built — a new document lands in its thread; a later move to another thread syncs
since v0.0.12, as a stamped write (*Moving a document*). (code: crates/sovereign-p2p/src/wire.rs,
sync_service.rs)

A synced document carries its thread, so it lands in its real lane when it is created on the
receiver. A peer on a build older than v0.0.11 is sent no document and none of its documents is
applied (*Peers on an older build*), so no document arrives without its thread. A document whose
thread does not exist here is drawn in the fallback lane, "Unfiled" (ch. 03).

### Ownership of synced documents

**Status:** ⚠ Conflict — decision pending (Céline).

A document synced from one's own paired device is treated as the user's own content (same
account) and is created **owned**. How ownership is decided for content arriving from outside is
chapter 04.

> ⚠ **Conflict — decision pending (Céline).** Every synced document is created owned, including
> documents that are external on the sending device; chapter 04 requires provenance to survive
> sync.

### Non-destructive peer overwrite and peer review

*Decided 2026-10-15 (Céline; C-WIRE below, *Applying a remote state*): no systematic review of
changes from her own devices; the analysis runs on every received change and raises an alert only
on a detection; what an overwrite replaced stays recoverable (History, previous values).*

**Status:** ◐ Partial — built in C-DOC adoption step 4 and released in v0.0.11: preservation, the
analysis on every received document change in the shell and the app, the alerts and the lists of
previous values in both UIs. Both UIs offer to restore only threads, contacts and PII records, of
the seven tables the sync layer can restore. (code: crates/sovereign-p2p/src/sync_service.rs,
wire.rs; crates/sovereign-ai/src/peer_audit.rs; crates/sovereign-app/src/tauri_commands/sync_alerts.rs;
crates/sovereign-shell/src/panels.rs)

A paired device is trusted to sync, not to silently replace. Every overwrite from a peer
preserves what it replaced:

- **Documents:** the local state is kept as a version before the peer's state applies, through
  the document service (C-WIRE *Applying a remote state*); no review flag is raised.
- **Rows:** before an overwrite, the prior row is captured, sealed under the account transport
  key, and stored as a `RowRecovery`, listed under **Previous values** (`v` in the shell; under the
  alerts in the app) with the device and the time, and the buttons "Restore" and "Remove from
  list"; nothing there is pending and no badge appears. Restore is wired in the sync layer for
  threads, entities, PII records, contacts, messages, conversations and suggested links; both UIs
  offer it only for threads, contacts and PII records. Append-only tables are never overwritten.
  **A thread a peer deleted** is brought back live, with its name, by a stamped write after the
  deletion it undoes (`DocumentService::thread_restore`), so the restore travels like any thread
  write; its documents
  in the trash stay there until restored (v0.0.12 item 6, A4; until v0.0.11 the restore went
  through `thread_update`, which refuses a deleted thread).
- **A restore that fails is said.** **Built in v0.0.13** (item C2, merged 2026-11-18): both hosts call
  the sync layer's one restore and show its notice where Restore was offered. Until
  v0.0.13 a failed restore from Previous values, a thread a peer deleted among them, was only written
  to the log: the host hands the restore to the sync node and hears nothing back.
  - **The host that asked shows the outcome,** in the place that offered Restore: Previous values
    in the shell, the list under the alerts on the phone. A failure is a notice « Not restored:
    {what} — {why} », where {what} names the row (the thread's name, the contact's name, the PII
    record's label) and {why} says what failed in plain words: sync is not running, the previous
    value could not be read, or the write was refused. The notice keeps to these plain sentences; a
    store's error is never shown, and any reason without a sentence of its own is « the write was
    refused », with its detail in the log.
  - **A restore that answers late is not « Not restored ».** The host waits a bounded time for the
    sync node's answer; past it, the notice is « No answer from sync yet for {what}. It may still be
    restored. », never « Not restored », since the node may still carry it out. No node, or a closed
    channel, is « sync is not running », at once (the Fable maille, by delegation of Céline
    (2026-11-14), on 2026-11-18). As built, the desktop waits on its window's thread, so the wait is
    kept short.
  - **A failed restore changes nothing:** the entry stays in Previous values, and the current value
    stays as it was.
  - It covers every row the hosts offer to restore (threads, contacts, PII records), since they go
    through the one restore of the sync layer.
- **Analysis:** every received document change is analysed (`peer_audit.rs`): the model-free
  checks (a scan of the incoming content for injected instructions, a wholesale-rewrite check
  against the prior) at once, during sync, in the shell and in the app; the model's verdict
  afterwards, in the background, where a model is loaded (in the shell, once the orchestrator
  exists, built on the first chat message). It never gates the sync. Of the changes received, only
  one with a detection raises an alert. (The Alerts list also shows documents that could not be
  read and documents whose thread was deleted, ch. 03.)
- **Alerts:** in the shell, the status bar says "A change from {device} to “{title}” looks
  suspicious — press A to see it" (or "N changes from your devices look suspicious — press A to
  see them"), and `a` opens the *Alerts* window, with **Open the document**, **Open the previous
  version** and **Dismiss** for each; in the app, the same notice as a banner over the canvas,
  opening the same list. Dismissing changes nothing in the document. Going back is the ordinary
  "Use this version" in History.

*Replaces v1.x:* the `pending_edits` conflict queue and its approval/rejection flow. *Retired
2026-10-15:* the per-change review flag, the review lists (`r` in the shell, the mobile list) and
the `RowsFlaggedForReview` badge.

### Concurrent-edit conflicts

**Status:** ◐ Partial — keeping both built and released in v0.0.11 (C-WIRE, *Direction and
concurrent edits*, decided by Céline 2026-10-15); the notice is shown in the app only: the shell
drops the event. (code: crates/sovereign-p2p/src/wire.rs, node.rs;
frontend/src/lib/components/SettingsPanel.svelte)

Two edits of one document made on two devices without either seeing the other's are **both
kept, without asking**: the later stamp is current on both devices, the other state is a version
in History on both. The sync round that finds it emits a `SyncConflict` event ("edited on both
devices: the later edit is current, the other is in History"). The app (phone and the frozen Tauri
desktop) lists such documents in Settings → Devices under "Edited on both devices", with **Open
History** and **Dismiss**; "Use this version" in History puts the other state back, as an
ordinary edit that syncs. "Keep mine" is gone (*"Use this version" replaces "Keep mine"*). The
shell shows no notice for it.

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

**Status:** ◐ Partial — decided 2026-10-15 with Céline's answers to all five questions (devices
on an older build, purges, backup restore, "Keep mine", review of her own devices' changes); tests
written first, built in C-DOC adoption step 4 and released in v0.0.11. Amended since: purges are
local (Céline, 2026-10-23; built in v0.0.12, item 3), the stamp bound inside the clock (v0.0.12,
lot 1), the device in a stamp is the peer id (v0.0.12, item 22), a move is a stamped write
(v0.0.12, item 15). Not as written: the shell shows no concurrent-edit notice (guarantee 7), and
both UIs restore 3 of the 7 tables sync can overwrite (guarantee 15). Technical choices marked
**[choice]**. (code: crates/sovereign-p2p/src/wire.rs, sync_service.rs;
crates/sovereign-docs/src/wire.rs; tests `crates/sovereign-p2p/tests/wire_*`,
`crates/sovereign-docs/tests/g24_*`–`g31_*`)

**Why.** Documents are written by one door, the document service (ch. 03, C-DOC), which keeps
v2 content, versions, trust, trust decisions, provenance, the trash and purge markers. Until
v0.0.11, sync wrote around it (C-DOC adoption, step 4): it compared and sent the **stored bytes**,
decided direction by **wall-clock** modification time, created every arriving document **owned**
with provenance *unrecorded*, and sent neither trust, the user's trust decisions, provenance
(copies included) nor purge markers. So an undetermined import became owned on her other device, a
trust decision made on the phone never reached the desktop, a purge was undone by the next sync,
and a document stored in v2 on one device and in a legacy form on another never compared equal.
This contract fixes what travels, how two states are compared, and how a remote state is applied
— through the document service and nothing else (as built, `DocumentService::apply_wire`, and
`apply_wire_version` for a concurrent state kept as a version).

**How the wire evolves (K8, decided 2026-10-10).** By **added fields with defaults**, no protocol
bump. Every field below that is new is optional on the wire; a peer that does not send it is
treated as described in *Peers on an older build*. Every new field travels **inside** the sealed,
signed envelope (the document snapshot, the AEAD-sealed manifest); nothing new is sent in clear.

### What travels for a document

The snapshot (sealed under the pair key, signed by the sender) carries:

| Field | Before v0.0.11 | Contract (built since v0.0.11) |
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
hash** and the **stamp** (with the previous stamp and a bounded list of earlier ones, so a device
several edits behind sees a fast-forward). The byte-hash entries of builds before v0.0.11 are not
sent when the document service exists: a peer on such a build is sent no document (*Peers on an
older build*). It gains a list of **purge
markers** — the document id and the stamp of its deletion, nothing else: a purged document travels
as a deletion, which puts or keeps a peer's copy in its trash, and no peer copy is purged (purges
are local, Céline, 2026-10-23; *Deletions and purges*) — and a **wire version** number so a peer on
an older build can be recognised. Each marker also carries the `fingerprint` field a v0.0.11 build
requires, sent empty, so that build still reads the manifest.

### Stamps instead of the wall clock **[choice]**

Every write to a document, and every trust decision, gets a **hybrid logical clock** stamp:
wall-clock time, a counter, and the device — ordered by time, then counter, then device. A device
never issues a stamp lower than one it has seen, so a later write always carries a greater stamp
even when the clocks disagree; a received stamp more than **5 minutes** ahead of the receiver's
clock is refused, as manifests are (until v0.0.11 a document was accepted up to 24 hours ahead;
rows other than threads keep a 24-hour bound on their wall-clock time, beside their Lamport stamps).
The rows already use Lamport stamps (*Rows* above); documents and trust decisions join them. This
closes the C-DOC weakness that a device whose clock runs ahead wins every disagreement. The
displayed times stay wall-clock.

**The bound is inside the clock** (v0.0.12, lot 1, after the cloud review of v0.0.11, finding 1). A
stamp more than 5 minutes ahead of this device's **wall clock** never moves its clock, whatever
carries it: a document's state, a trust decision, a purge marker, a thread row (refused whole
beyond the bound, as since v0.0.11), a backup, or a stamp already stored (read again at start). A
document whose state or decision stamp is beyond the bound is refused whole; a purge marker beyond
it is refused and applies at a later sync; a backup is restored as stored, but its stamps beyond the bound do not move the clock. The rule "never lower
than a stamp issued or seen here" holds for stamps within the bound; a stamp beyond it that is
already stored keeps winning concurrent conflicts on its document until real time passes it.
Each device bounds against its own wall clock, so two devices whose clocks are d apart may see a
delay of at most d, never a stop. A counter at its maximum moves to the next millisecond: no wrap,
no panic. *Status:* ✅ built in v0.0.12 lot 1 (`lot1_c_stamp_bound.rs`, `lot1_clock_tests.rs`).

**The device in a stamp is the device's peer id** (v0.0.12 item 22; found by PANDA2's items 14 + 19
test author; Céline, 2026-10-24: "Dans la v0.0.12"). It is the libp2p peer id of the persona,
derived from its device key, the same id the P2P node announces: unique per workspace and persona,
unchanged by renaming the device. Before, it was the configured display name ("Sovereign Device" by
default on every desktop), so two devices could write equal stamps, and an equal stamp is ignored as
known. Stamps already stored keep their name; old and new coexist (ordering by time and counter
first; a write naming an old stamp as its previous one fast-forwards). Where a device is named from a
stamp (History, orphans, alerts), the name comes from the paired list, "this device" for the own id,
and an old name is shown as it is. If the identity cannot be derived, the desktop opens no document
service rather than stamp with a shared word. *Status:* ✅ built (`item22_device_identity.rs` and the
hosts' `item22_identity_tests.rs`).

### Comparing two states

Two states of a document are equal when their **normalised forms** are equal: the title, the
thread, the trash state, the trust, the trust decision, the provenance, and the content **as v2**
— a legacy form converted with the deterministic migration seeded by the document id (ch. 03, G3),
so the same legacy document is the same on every device whether or not it has been migrated. The
normalised hash is computed by the document service (`sync` never parses content, per the readers
gate). A title-only edit or a thread move changes it, so both travel (a title-only edit since
v0.0.11, a move since v0.0.12; before, neither did).

### Direction and concurrent edits

- **Fast-forward.** A remote state whose *previous stamp* is the local state's stamp replaces it;
  a local state whose previous stamp is the remote's is sent. Otherwise the edits were concurrent.
- **Concurrent edits keep both, without asking** — the rule Céline decided for save conflicts on
  2026-10-12, applied across devices: the state with the greater stamp is current on **both**
  devices; the other state is kept as a version on **both** (the losing device versions its own
  state before applying, as it already did before v0.0.11; the winning device receives the losing
  state and stores it as a version without making it current); both devices show a notice. Nothing is asked unless the
  user acts on the notice.
- With three or more devices, a "concurrent" verdict can be a false alarm (a relayed state whose
  previous stamp is unknown here); the cost is an extra version and a notice, never a loss
  **[choice]** — full version vectors are not worth it for one user's devices.

### Applying a remote state

Every remote document state goes through the document service (C-DOC G19; as built,
`DocumentService::apply_wire`, with the wire metadata of G24–G31, and `apply_wire_version` for a
concurrent state kept as a version): the local
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

**What the service needed beyond `apply_remote`** — each built in step 4 (C-DOC G24–G31, ch. 03)
and released in v0.0.11, sync calling `apply_wire` and not `apply_remote`:
1. the original **creation time** (a document arriving was dated by the sync) — ✅ carried;
2. the **stamps** (the service decided with the wall-clock `modified_at`, and trust decisions
   carried a wall-clock `decided_at`) — ✅ stamps for writes and decisions;
3. storing a remote state **as a version without making it current** (the winning side of a
   concurrent edit) — ✅;
4. receiving a **purge marker** from a peer, applied as a deletion (*Deletions and purges*) — ✅,
   as amended by item 3 (v0.0.12);
5. the **author device** distinct from the relaying sender — ✅;
6. an outcome that says the edit was **concurrent** (for the notice) and whether **ingest changed
   the text** (so the rewritten state travels back) — ✅;
7. the **normalised hash** as a service function, and a way to send the stored state (with trust,
   decision, provenance, creation time, stamps) without the sync layer reading `content` — ✅;
8. a **backup restore** entry (below) — ✅;
9. **row tables** stay out of the service: conversation titles and contact-address sets are
   applied by the row apply (below) — ✅.

### Deletions and purges

- **Every deletion a user makes is a trash** (C-DOC G13) and travels as a tombstone; applying it
  moves the document to the trash on the other device, restorably; a live state with a greater
  stamp brings it back (G19). There is no hard delete left to escape this.
- **A purge is local** (decided by Céline, 2026-10-23, after the cloud review of v0.0.11;
  replaces the rule of 2026-10-15, under which a purge marker travelled and purged a peer's
  identical copy). Emptying the trash, purging one item, or 30 days in the trash removes the
  document **from this device only**. A peer's purge never removes anything here, and this
  device's purge never removes anything there.
- **What travels after a purge is the deletion, never the purge.** The device that purged keeps a
  **local purge marker** — the id and the stamp of the deletion, nothing else (no fingerprint, no
  content) — and advertises it as a deletion: a peer that holds the document **live** with an
  earlier stamp moves it to **its trash**, restorably, and that state takes the deletion's stamp,
  so both devices order it alike; a peer that holds it live with a stamp not earlier than the
  deletion keeps it live; a peer that holds it **in its trash** keeps it there as it is, its trash
  date and its stamp unchanged. In each case the peer's copy stays until its own term (30 days on
  that device) or until it is purged there by hand. A peer that holds **no copy** records the
  deletion (the id and its stamp), so an older copy arriving later from a third device is
  ignored; of two deletions recorded for one id, the later stamp is kept. A received deletion
  never purges anything (v0.0.12 item 3).
- **The 30 days run on each device's own clock**, from the moment the document entered *that
  device's* trash (by a local delete or by applying a peer's deletion), never from a date a peer
  sent.
- **A purged id comes back only live, by a later action.** A device that purged a document ignores
  a peer's copy of it that is in the peer's trash, and a live state whose stamp is earlier than or
  equal to its deletion's. A **live state with a later stamp** — the document restored or edited on
  another device after the deletion — **brings it back here, live** (Céline, 2026-10-23), through
  the service, as a new arrival (its own first version, the sender's trust and provenance); the
  local marker is then dropped. This is the one exception to C-DOC G13 ("a purged document is never brought back by
  sync"), which is amended to say so.
- **Peers on v0.0.11** (Céline, 2026-10-24): documents keep syncing with them. A v0.0.11 purge
  marker is read as a deletion (its fingerprint is neither compared nor stored); this build's
  markers keep the `fingerprint` field a v0.0.11 build requires, sent empty, so that build still
  reads the manifest and ignores them.
- **The 30 days and an upgrade:** at its first purge tick a store records when it started counting
  ("trash since"); a document whose trash date is earlier is purged 30 days after that, not
  before, and the trash view shows those days **[choice]**. **The days shown are the days used:**
  the trash view's days left are counted from the date the purge uses — the local trash date,
  floored by "trash since" — in whole days; the date shown stays the stored trash date, so only a
  document floored at an upgrade shows an old date with more days left. A document restored from a
  backup into the trash enters this device's trash at the restore. **Fingerprints stored by
  v0.0.11 are erased** at every unlock, on every unlock path (a store whose format conversion
  completed under v0.0.11 included), each persona at its own unlock; the purge tick and the
  reading of the markers for the manifest erase any left, so no stored fingerprint is ever sent.
- **Sync does not refetch what it will ignore:** the document plan neither fetches nor pushes a
  copy that the other side's marker would ignore **[choice]**. *(Remainder, backlog: a copy a peer
  holds in its trash with a later stamp is still fetched each round, then ignored; the manifest
  does not say whether a copy is in the trash.)*
- *Status:* ✅ built in v0.0.12 item 3 (tests `item3_local_purges.rs`, `item3_no_fingerprint.rs`,
  `item3_trash_since.rs`, `item3_purge_outcomes.rs`, `g27_purge_markers.rs`, and `sovereign-p2p/tests/item3_local_purges.rs`,
  `wire_purges.rs`). Review of v0.0.11, findings 2, 3, 13 and 14, closed.

### Moving a document

**A move is a stamped write, and it travels** (v0.0.12, lanes lot, item 15; Céline's rc3 test A6
showed that a move never converged). Every move goes through the document service: the desktop's
drag, the assistant's move, the restore into a thread (one operation: out of the trash and into the
thread), and the phone's "Move to Thread". The thread changes, title, content and versions do not;
moved on two devices, the later move wins on both. A move into a deleted or unknown thread, of a
trashed or purged document, or into the thread the document is already in, is refused with its
reason. **The pin stays on each device** (Céline, 2026-10-24): it is not stamped, not sent, and
never part of the compared state. A card's free position (written only by the frozen Tauri
desktop) is not synced either. *Status:* ✅ built (`lanes_m1_move.rs`, `lanes_m2_one_move_path.rs`,
`wire_lanes_moves.rs`, `wire_lanes_pin.rs`).

### Peers on an older build

*Status:* ✅ built for v0.0.13 (lot L1b, merged 2026-11-15): wire version 2, carried by each document
request (`crates/sovereign-p2p/tests/wire_version_2.rs`).

**Documents are not exchanged with a device on an older build until it is updated** (decided by
Céline, 2026-10-15), and both screens say so ("{device} runs an older version of Sovereign.
Update it to sync documents with it; other data still syncs."; in the shell's status bar, "{device}
runs an older version: update it to sync documents"). Threads,
contacts, messages and the other rows keep syncing. The reason: an older build cannot read v2
content — its tolerant read shows a v2 document as the raw JSON text, and an edit there would save
that text back as the document.

**A peer is on an older build when its manifest carries a lower wire version, or none.** *(A
v0.0.11 peer is not an older build for documents: same wire version, same document format; only its
purges differ, and they are read as deletions, see* Deletions and purges*.)*
- **The wire version goes up with every change of the document format** (Céline, 2026-11-14:
  « Oui, l'appliquer », on her rule of 2026-10-15 applied between v0.0.12 and v0.0.13). v0.0.13
  raises it from 1 to 2, since media blocks name the store's medium instead of a path (ch. 03).
- **The refusals are the new build's.** A build that only tests for a missing version, as v0.0.12
  does, does not compare numbers. So toward a peer on a lower wire version, the new build:
  - sends it no document state;
  - serves none of its requests for documents;
  - applies none of its document states.
- **Its purge markers are still applied.** A marker carries an id and a stamp, and no document
  content, so the change of format does not touch it. A purge is the user's intent.
- **The rule holds from both sides.** A build that meets a higher wire version holds the same three
  refusals. Its screen says that this device runs the older version and is the one to update
  (Céline, 2026-11-14: « Le garder »): a notice titled « This device needs an update », « This
  device runs an older version of Sovereign than {name}. Update this device to sync documents with
  it; other data still syncs. », and in the status bar « This device runs an older version: update
  it to sync documents ». It clears the same way as the sentence for the other device.
  v0.0.12 lacked this half, and it can only be put into a build before that build is published.
- **The answering side judges by the request it answers, whichever side started the round.**
  - The two requests that carry documents, fetching and pushing them, carry the requester's wire
    version. A request built by v0.0.12 has no such field, and reads as having none.
  - **A request with no version, or a lower one,** is refused for documents: nothing is served,
    nothing is applied. Rows, shards and backup fragments go on in their own requests.
  - **A request with a higher version** is refused the same way, and this device knows it is the
    older one.
  - **The refusal is said to the requester,** not answered as an empty list, so that a refused round
    is told apart from « nothing to sync ». Toward a request with no version or a lower one, it is
    the error response that v0.0.12 already knows: v0.0.12 logs it and goes on with its rows.
  - **Nothing is remembered per peer** on the answering side, so nothing can go stale: a device put
    back on an older build is refused at its first request. The asking side keeps its plan from the
    manifest it asked for.
  - **Any later request that carries a document joins this rule.** A test lists them, and a new one
    fails until it is named.
  - Found by the laptop maille (a peer that started every round was never refused); the way chosen
    was proposed by the Fable maille, over an announcement remembered per peer.
- **Documents written on the old device stay there until it is updated,** then sync normally. The v2 format is already written by this build, so "keep writing
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
it; the backup carries the purge markers it knew of **[choice]**, as id and deletion stamp, no
fingerprint (a backup taken by v0.0.11 has its markers restored without their fingerprints).
Restored, they are recorded as deletions and keep a document purged before the backup from
returning by sync later, unless a peer sends a live state with a later stamp (*Deletions and
purges*, as amended 2026-10-23: purges are local; v0.0.12 item 3). If no other device
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

The row apply (*Rows* above) applies a **conversation's title** when it changes, and a
**contact's addresses** as a set, removals included (both hashes then converge), since v0.0.11.
The contract also requires one shared list of **restorable tables** used by sync and by both
*previous values* lists, so every table sync can overwrite is one both UIs can restore. As built,
the sync layer restores seven tables and both UIs offer three of them, each from its own list
(threads, contacts, PII records; guarantee 15, status above). (code:
crates/sovereign-p2p/src/sync_service.rs; crates/sovereign-shell/src/panels.rs;
crates/sovereign-app/src/tauri_commands/sync_alerts.rs) Trust of entities and contacts is out of
this contract.

### Guarantees — each one is tested

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
10. **A purge is local** (Céline, 2026-10-23): a purge removes the document from this device only;
    on a peer, the marker moves a live copy stamped before the deletion to the trash, or keeps it
    there, and never purges it; a live state with a later stamp than the deletion brings the
    document back here, live; the marker carries no title, no content and no fingerprint, only the
    id and the deletion's stamp.
11. **Only the service writes documents**: no store write to documents from `sovereign-p2p` (the
    write gate's step-4 entries removed and its patterns extended to soft delete and restore).
12. **Older peers exchange no documents** (Céline, 2026-10-15 and 2026-11-14). A peer with a lower
    wire version, or none, is sent no document state, its requests for documents are not served,
    and none of its document states is applied. Its purge markers are still applied. This holds in
    either direction, whichever side starts, and from both sides: a build that meets a higher wire
    version refuses in the same way, and says that it is the one to update. Both screens say which
    device to update, and rows sync with it as before.
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
