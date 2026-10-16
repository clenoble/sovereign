# 06 — Recovery

How a user gets back into their workspace after forgetting the passphrase (Feature 1, guardian
access recovery), and how their data would survive the loss of every device (Feature 2, crowd
data backup). The transport both ride on is chapter 05; the key hierarchy is chapter 07.

*Consolidates:* specification v1.2/v1.3 §Guardian Social Recovery, §Recovery Flow (Feature 1),
§Guardian Management, §Optional Cloud Backup, §Security Model (guardian items); archived plans
`F1-guardian-access-recovery-implementation.md`, `backup-m1-security.md`,
`backup-ui-command-contract.md`, `HANDOVER-m2-guardian-app.md`,
`DECISIONS-2026-07-16-frontend-owner.md`, `p2pbackupuserreadyplan.md`, `p2p-completion-plan.md`;
the M0 transport spike (branch `spike/m0-transport-discovery`); Appendix C
(`doc/legal/sovereign_os_ethics.md`) §2.2, §3.3; release notes v0.0.7–v0.0.9; the code at
`main` d02c353.

## Two features, never conflated

**Status:** ✅ Built — the separation holds in code: Feature 1 shipped, Feature 2 not built.

| | Feature 1 — Access recovery | Feature 2 — Crowd data backup |
|---|---|---|
| Trigger | The passphrase is lost or forgotten. | Every device is lost. |
| Data | Present, on the user's synced devices. | Brought back from an opt-in mesh of other users. |
| Who helps | Guardians restore **access**. | Mesh hosts return **ciphertext**. |
| Status | Shipped v0.0.9. | Phase 2. In v0.1, data durability is synced devices. |

A build plan once conflated the two — guardians were described as holding a data key — and the
divergence cost a rework. Guardians hold a share of the **Recovery Key**: never documents, never
a data key.

## Feature 1 — Guardian access recovery

**Status:** ◐ Partial — recovery key, roster, enrollment protocol, release gate, sealed state and
verify-before-commit built; on the shipped surfaces the shell does not record completed
enrollments and the mobile recovery wizard cannot start; heartbeat, rotation, cross-device
cancellation and the Android guardian app not built.

### The Recovery Key

**Status:** ✅ Built — (code: crates/sovereign-crypto/src/recovery_key.rs)

- The Recovery Key is a **dedicated random 256-bit key**, distinct from the passphrase-derived
  Master Key.
- It seals the **account secrets** — the random KEK and the AccountKey that actually decrypt the
  user's content — into a **Recovery Bundle** (`KEK ‖ AccountKey`, AEAD). The bundle lives on
  the owner's device outside the passphrase wrapping: inert without the Recovery Key.
- Reconstructing the Recovery Key opens the bundle and restores decryption capability **without
  the passphrase**. This works because the at-rest content chain is rooted at the KEK, not at the
  passphrase-bound Device Key (chapter 07).
- The Recovery Key itself is never retained: it is split at setup and survives only as shares.

### Shamir 3-of-5

**Status:** ✅ Built — (code: crates/sovereign-crypto/src/guardian/shamir.rs, recovery_roster.rs)

The Recovery Key is split into **5 shares, any 3 of which reconstruct it** (Shamir secret sharing
via the `blahaj` crate). The threshold and total are fixed (3-of-5); a configurable M-of-N is not
specified.

### The roster, and arming at five

**Status:** ◐ Decided 2026-10-10 (Céline): **recovery may start once 3 of the 5 guardians are
enrolled** (the threshold). Five stays the size of the roster and the recommendation, since it
tolerates unavailable guardians. Today the roster and both UIs say recovery turns on at 5/5; the
UI and the roster's arming rule are to be aligned with this decision.

- Setting up recovery generates the Recovery Key, seals the bundle, pre-splits five shares and
  lays out five roster slots. The roster — including shares not yet handed out — is stored
  AEAD-encrypted under the KEK (`recovery_roster.enc`, login only); the bundle is stored beside it
  (`recovery.bundle`, readable before login). (code: crates/sovereign-crypto/src/recovery_roster.rs,
  recovery_store.rs)
- As each guardian enrolls, that slot's share is handed over and the owner's copy cleared.
- **Recovery is armed only when all 5 guardians are enrolled** (decided 2026-07-10). The roster
  reports `enrolled / 5`; the shell and the mobile app say recovery turns on at 5/5; enrollment
  stops once armed. (code: crates/sovereign-app/src/tauri_commands/guardian.rs,
  crates/sovereign-shell/src/p2p.rs)
- Enrollment confirmations reach the encrypted roster through a non-secret pending queue
  (`guardian_enrolled.pending`), because the event path holds no KEK. A record that cannot be
  parsed is kept and counted, never silently dropped — a guardian who believes they are enrolled
  must not vanish from the roster. The mobile app writes this queue; the shell never records
  enrollments, so its roster stays at 0/5.

> ⚠ **Conflict — decision pending (Céline).** The specification offers recovery only with the full
> 5-guardian roster. Recovery is intended to start only from an armed roster; enforcement is being
> completed (private backlog). Open: whether recovery must refuse to start before 5/5, or whether
> the specification should allow it once the threshold of 3 guardians is enrolled.

### In-person enrollment

**Status:** ◐ Partial — handshake built and tested; the shell does not record completed
enrollments, and neither owner UI shows the offer as text while the guardian app accepts only
pasted text. (code: crates/sovereign-p2p/src/guardian_enroll.rs, node.rs;
crates/sovereign-guardian/src/enroll.rs; test: crates/sovereign-p2p/tests/e2e_guardian_enroll.rs)

Enrollment happens **in person**, one guardian at a time:

- The owner shows a **plaintext enrollment offer** as a QR code: its own kind and version (it can
  never be confused with a pairing offer), a random offer id, the owner's PeerId and dial hints,
  the owner's label, and an expiry. No secret is in it.
- The owner says a **spoken code** aloud (10 symbols, 50 bits, the pairing-code generator). Both
  sides stretch it with **Argon2id**, salted by the offer id, into a handshake key *K*.
- Handshake over `/sovereign/sync/1`, MAC domains separate from pairing:
  1. `GuardianHello` → `GuardianChallenge { nonce }`
  2. `GuardianProof` (MAC under *K*) → `GuardianGranted` — the share plus custody metadata
     (owner tag and label, shard id, epoch, threshold, total), **AEAD-sealed under *K***
  3. `GuardianComplete` with a **custody receipt** (MAC under *K* binding the shard id and the
     guardian's identity) → `GuardianDone`
- The guardian persists the share **before** sending the receipt, and the owner marks the
  guardian enrolled **only on the receipt** — so the roster can never claim a share the guardian
  did not store.
- Offer TTL 600 s, three wrong proofs destroy it, one session at a time with idle eviction.
- The guardian is **not** a paired device: it receives no AccountKey, no salt, no sync access.

### Identity proof — out of band, human, per pair

**Status:** ✅ Built — nothing is stored, by design; the enrollment screen carries the instruction.

At enrollment, each owner↔guardian pair agrees how the guardian will recognize the owner at
recovery time: a shared memory, a physical object, a private question — ideally something not
discoverable digitally, so it survives the compromise of a device or account. The guardian judges
it **person to person** before approving. Sovereign stores nothing about it; each pair's proof is
independent. (Decided 2026-07-10.) This replaces a pre-shared recovery passphrase: no second
digital secret to forget or steal. The recognition-proof copy is required on the enrollment
screen in the shell and in the mobile app. (code: crates/sovereign-shell/src/app.rs; frontend/src/lib/components/RecoveryPanel.svelte)

### Guardian custody

**Status:** ◐ Partial — sealed custody built; platform-keystore hardening for mobile not built.

(code: crates/sovereign-guardian/src/custody.rs, state.rs, lib.rs)

- One guardian install can guard for several people. Each duty records owner tag and label, the
  owner's PeerId, shard id, epoch, threshold/total and enrollment time; the share bytes are stored
  separately, **sealed under a per-install random custody key** (XChaCha20-Poly1305). The custody
  key and the guardian's identity root are written owner-only.
- Enrolling again for the same owner replaces the old duty.
- On one disk this is defense in depth, not a hard boundary; a single guardian's share reveals
  nothing by itself. Specified, not built: platform keystore protection for the Android build.

### Recovery card and bundle

**Status:** ◐ Partial — written on the device where enrollment runs; not distributed to the
owner's other devices.

The **recovery card** (`recovery_card.json`) is a non-secret pointer the recovering device reads
before login: owner tag, epoch, threshold, enrolled guardian PeerIds and seed relays. With the
bundle, it is what makes "Forgot your password? Recover with guardians" available on the login
screen. (code: crates/sovereign-crypto/src/recovery_store.rs)

Specified, not built: the bundle and card living on all of the owner's synced devices. Sync
carries database records (chapter 05), and neither file is one; today recovery is possible on
the device where guardians were enrolled.

The card's relays come from the seed-relay setting (chapter 05: empty by default, unsettable on
mobile); with no relay, recovery refuses to start.

### Recovery flow

**Status:** ◐ Partial — built in the shell; the mobile wizard does not supply the new passphrase
at start and has no resume step. (code: crates/sovereign-p2p/src/access_recovery.rs,
backup_client.rs; crates/sovereign-app/src/tauri_commands/access_recovery.rs;
crates/sovereign-shell/src/recovery.rs)

Runs **before login**, resumable across restarts:

1. The user chooses a **new passphrase first**, before any guardian is contacted.
2. The device builds one relay-circuit address per guardian on the card and asks each for its
   share (`RequestShard`, a fixed request id per guardian so repeated polls belong to the same
   window).
3. The user reaches each guardian out of band; each guardian judges the identity proof and
   approves or denies in the guardian app.
4. A guardian's share is released only once **approved and after 72 hours** (below). The device
   polls (45 s cadence in the UIs, plus "check now") until **3 shares** are in.
5. The shares reconstruct the Recovery Key, which opens the bundle: KEK + AccountKey.
6. **Verify before commit** (below), then a new `auth.store` wraps the recovered secrets under
   the new passphrase, with a fresh salt, a fresh device id and an unreachable random duress
   decoy. The session installs; synced content decrypts.

The shell and the mobile app drive the same core: their finalize paths call one shared function,
so there is one copy of the ordering that protects the account.

*Replaces v1.x:* "Shards transmitted to the recovery device over an authenticated channel" —
built as `RequestShard` over a Noise channel through the relay; shares are not separately
encrypted to the recovering device.

### Release gate: approval + 72 hours, on the guardian's device

**Status:** ◐ Partial — approval and the 72-hour gate built on the guardian's device; the
release-build requirement below is being completed (private backlog). (code:
crates/sovereign-p2p/src/backup_host.rs; crates/sovereign-guardian/src/serve.rs)

- The first request for a share starts its window. The share leaves only when **both** hold: the
  guardian's user has **explicitly approved**, and **72 hours** have passed since that first
  request. Denying clears the request and the approval.
- The gate runs **on the guardian's device**: the recovering device cannot shorten it.
- **Requirement:** development overrides (a shortened delay for tests, an auto-approve flag in
  the guardian CLI) are excluded from release builds. ◐ Being completed (private backlog).
- Recovery read verbs pass a per-peer rate limit.

### Notifying every guardian

**Status:** ◐ Partial — every guardian on the card receives the request when reachable; other
channels not built.

- Built: the recovering device requests a share from **every** guardian on the card, not only
  three, and the guardian app lists pending requests — so every reachable guardian learns of the
  attempt, and through them the real owner.
- Specified, not built: delivery to a guardian who is offline at poll time (the mailbox, chapter
  05, is built for this and not wired); notification by email, SMS or push; reporting failed
  attempts to all guardians; an auditable request log on both devices.

### In-progress state sealed under the new passphrase

**Status:** ✅ Built — (code: crates/sovereign-crypto/src/recovery_seal.rs; crates/sovereign-p2p/src/access_recovery.rs)

During the approval window, collected shares never touch disk in clear. They are AEAD-sealed
under a key derived from the **new** passphrase (Argon2id, account parameters) and a stored,
non-secret per-recovery salt; the seal key is never persisted. A sealed probe distinguishes a
wrong passphrase on resume before any share has arrived. The progress file is otherwise
readable (phase, which guardians released) so status shows without the passphrase.

### Verify before commit

**Status:** ✅ Built — (code: crates/sovereign-crypto/src/recovery_store.rs)

The new `auth.store` **must not overwrite the old one until at-rest decryption is proven.**
Finalize first opens every content-key store of the primary persona under the recovered KEK; if
any fails, it aborts and leaves the prior `auth.store` byte-for-byte intact. A failed recovery
never bricks an otherwise recoverable account, and the UI says so ("your account was left
unchanged"). Added after the first live run (2026-07-16) overwrote the store before an at-rest
failure.

### Shares are not self-authenticating

**Status:** ✅ Built — (code: crates/sovereign-p2p/src/access_recovery.rs; crates/sovereign-crypto/src/recovery_key.rs)

A malformed share is ignored at ingest. A well-formed but wrong share is accepted at ingest and
caught when the bundle's AEAD refuses to open: recovery **fails loudly** and never returns wrong
secrets. Limit: one invalid share makes finalize fail, with no retry on another subset of the
shares collected.

### Guardian app

**Status:** ◐ Partial — desktop app and CLI built. **Decided 2026-10-10 (Céline): the guardian
app is mobile-first** — guardians carry phones; the Android app is to be built (a port of the
existing Tauri app), the desktop app stays.

- **`sovereign-guardian`** is the engine and a CLI (`init`, `enroll`, `run`, `status`). It
  depends on `sovereign-p2p`, `sovereign-crypto` and `sovereign-core`, and on `sovereign-db`
  (test utilities, an empty in-memory database) because serving mode runs a node whose sync
  service needs one — the guardian never syncs. Serving mode reserves a relay circuit so a
  recovering friend can reach it, and serves held shares through the same audited release gate
  as above. (code: crates/sovereign-guardian/src/*, Cargo.toml)
- **`sovereign-guardian-app`** is the friend-side app: desktop Tauri today. It enrolls (offer
  pasted as text + spoken code + label), lists duties, lists pending recovery requests and
  approves or denies them. No content, no sync, no AI. It is not among the v0.0.9 release
  downloads; it builds from source. (code: crates/sovereign-guardian-app/src/*)
- A guardian does not need Sovereign GE.
- Specified, not built: the Android guardian app; approval behind device biometrics or PIN; a
  delete-duty action in the app (the engine supports it).

> ⚠ **Conflict — decision pending (Céline).** This chapter recorded that a mobile-first guardian
> app was chosen in July 2026 (friends carry phones); later plans order the work desktop-first.
> What is built is the desktop guardian app. Which platform comes first is not decided here.

### Heartbeat

**Status:** ◐ Partial — the proof-of-possession primitive exists; no protocol message and no
driver.

The owner proves periodically that each guardian still holds its share, without the share
crossing the wire: the owner sends a nonce, the guardian answers with a MAC keyed by a key derived
from the share and its epoch. A proof for one epoch never satisfies another. Built: the primitive
and the guardian-side answer. (code: crates/sovereign-crypto/src/guardian/pop.rs,
crates/sovereign-guardian/src/custody.rs) Not built: the ping/pong messages, owner-side
scheduling through the mailbox, and roster health (verified / unreachable / lost) in the UI.
The owner keeps no verification material once a share is handed over, so a heartbeat also needs
per-slot verification material in the roster.

### Owner cancellation of a takeover

**Status:** ◐ Partial — the owner's login is intended to cancel a recovery on the same device;
cross-device cancellation is specified, not built.

A successful login with the real passphrase on a device is intended to clear any recovery in
progress **on that device**; completion is tracked in the private backlog. The cross-device
half — the owner's login making guardians deny a recovery started elsewhere — needs an
owner-to-guardian liveness channel and is not built; until then the cross-device case rests on
notifying every guardian, the 72-hour window and the guardians' deny.

> ⚠ **Conflict — decision pending (Céline).** The specification states, unscoped, that the
> legitimate owner invalidates a concurrent takeover by authenticating with their real passphrase
> during the window. What is intended today is scoped to the device where the owner
> logs in.

### Rotation and guardian management

**Status:** ○ Specified, not built.

- **Replace a guardian:** revoke and replace a lost or departing guardian by re-splitting and
  redistributing at a new epoch. The threshold is never reduced; if fewer than five guardians are
  available, the user enrolls replacements before old shares are revoked.
- **Dropout handling:** notifications of increasing urgency after a guardian has been unreachable
  for a week (depends on the heartbeat).
- **Proactive rotation:** yearly; on suspicious activity (high threshold); on demand.
- **After a recovery:** a new Recovery Key and new shares are issued and redistributed.

The epoch already exists in shares, custody, proofs and the recovery card; the re-split flow does
not.

### Tags: one tag, where two were specified

**Status:** ⚠ Conflict — decision pending (Céline).

> ⚠ **Conflict — decision pending (Céline).** The specification requires two independent tags,
> known together only to the owner: `T_g` (guardians are enrolled under it) and `T_f` (mesh hosts
> index fragments under it), so that guardians who collude to rebuild the Recovery Key still
> cannot locate the owner's mesh ciphertext. The code derives **one** tag from the AccountKey
> (`derive_backup_tag`) and uses it both for guardian enrollment and as the fragment `owner_tag`.
> (code: crates/sovereign-crypto/src/account_key.rs; crates/sovereign-app/src/tauri_commands/guardian.rs, backup.rs; crates/sovereign-shell/src/p2p.rs)
> Without live mesh hosting this has no effect today; it matters once Feature 2 is built. The
> specification also leaves open whether `T_f` should be derivable from the account secret, and
> how it is recovered after total device loss.

### Security properties

**Status:** ✅ Built — properties of the built design; the dependencies on unbuilt parts are named.

Against malicious guardians:

- One or two colluding guardians: no impact — fewer than three shares reveal nothing.
- Three colluding guardians can reconstruct the **Recovery Key**, and through the bundle, access.
  Mitigations: choose guardians from different trust domains; each must still pass the window,
  and every reachable guardian sees every request. Because the Recovery Key is dedicated and
  rotatable, a collusion yields a capability that rotation can revoke, not the account key
  itself — rotation is not yet built (above).
- The out-of-band human proof blocks **remote** impersonation: a SIM swap, phished credentials or
  a compromised device cannot fake a shared memory with each guardian.

Against device theft: a stolen guardian device yields at most one share, which alone reveals
nothing; the share is sealed at rest on it (see *Guardian custody* for the limits of that seal).

Against coercion: no single guardian can unlock anything; the 72-hour window is the
anti-coercion delay. An optional duress path for recovery is specified, not built.

The plain-language trust statement shown to users: *your secrets stay yours as long as fewer than
3 of your 5 guardians collude.* (It understates the design deliberately; see above.)

*Corrects v1.x:* "3 malicious: could reconstruct Master Key." Guardians hold shares of the
Recovery Key, never the Master Key; the Master Key is passphrase-derived and is not split.

### Appendix C constraint: guardian protocol scope

**Status:** ⚠ Conflict — decision pending (Céline).

Appendix C §3.3 binds the guardian protocol to one function — storing and returning a key share
— and §2.2 relies on that to rule out repurposing (dead man's switches, covert coordination).

> ⚠ **Conflict — decision pending (Céline).** Appendix C §3.3 rule 5 says the protocol "does not
> support time-delayed shard release"; the built release gate holds every share for 72 hours.
> Rule 1 says there is "no messaging channel" beyond shard request/response; the specified
> heartbeat adds a ping/pong, and the mailbox meant to carry guardian traffic accepts any sealed
> blob up to its size limit. Rule 5 also requires biometric-authenticated approval, which is not
> built. Reading offered, not decided: the 72-hour **delay** is a hold that still requires active
> approval, not a time-**triggered** release, so rule 5's dead-man's-switch intent holds and the
> conflict there is one of wording. The mailbox and heartbeat are a question of substance: whether
> a general sealed-blob channel used by guardians fits rule 1. Rule 4 (every request logged on
> both devices) is not built either. Rule 6 makes the guardian app store, approve and **delete**
> a shard; the delete-duty action is not in the guardian app (the engine supports it).

## Legacy data-key backup path

**Status:** ⚠ Conflict — decision pending (Céline). Registered in the Tauri host, including the
mobile build (the Android APK); no frontend caller; removal pending.

> ⚠ **Conflict — decision pending (Céline).** The v0.0.7 data-backup path is still registered in
> the Tauri host, including the mobile build, with no frontend caller: `backup_now`,
> `backup_status`, `approve_shard_release`, `deny_shard_release`, and the matching pre-login
> `start_recovery` / `recovery_status` / `recovery_poll` / `cancel_recovery` /
> `recovery_finalize`. It makes the user's **paired devices** the guardians of a per-snapshot
> **backup key**, with a majority threshold (2-of-2 with two devices, 2-of-3 with three, 3-of-5
> with five). This conflicts with two rules of this chapter: guardians hold the Recovery Key,
> **never a data key**; and the threshold is **never reduced**.
> (code: crates/sovereign-app/src/tauri_commands/backup.rs, recovery.rs, lib.rs; crates/sovereign-p2p/src/recovery.rs)

The snapshot and fragment machinery underneath it is what Feature 2 will reuse (below).

## Feature 2 — Crowd data backup

**Status:** ○ Specified, not built — Phase 2. Parts of its machinery exist (below).

When every device is lost, the user's encrypted workspace comes back from an **opt-in,
reciprocal mesh** of Sovereign users: each participant hosts opaque fragments of others' backups
in exchange for theirs being hosted.

Recovery needs two things stored apart, never combined in one party's hands:

- the **key** — the account secrets, reachable through the guardians (Feature 1);
- the **ciphertext** — erasure-coded fragments on the user's own devices and, in Phase 2, mesh
  hosts that hold opaque fragments and no key material.

No host can decrypt; no guardian holds ciphertext. Total compromise would require at least three
colluding guardians **and** the ability to locate and gather a threshold of the owner's
fragments — which is what the separate fragment tag (above) and the read limits (below) protect.

### Snapshot, sealing and erasure coding

**Status:** ◐ Partial — built for the legacy path; not wired to Feature 1 or a mesh.

(code: crates/sovereign-p2p/src/backup.rs, backup_client.rs)

- A snapshot is a **logical, decrypted export** of every synced table (raw database files are
  device-bound), sealed under a fresh random key, then **Reed-Solomon coded** (default 3 data +
  2 parity: any 3 of 5 fragments rebuild it). Each fragment's digest is checked; a corrupted
  fragment counts as missing.
- Restore writes through the id-preserving creates, so a restored device re-syncs with
  survivors without duplicating anything.

### Signed, rollback-resistant manifest (A1)

**Status:** ◐ Partial — built and tested on the legacy path.

The backup manifest (snapshot id, epoch, owner tag, digests, fragment layout) is signed with an
Ed25519 key derived from the AccountKey, so every device of the account signs alike and a
recovering device re-derives the verifying key from passphrase and salt alone, independently of
any host. Before restoring, recovery aborts unless: every guardian-held manifest names the same
signer; the signature verifies (and matches the re-derived key); the epoch is not older than any
epoch a guardian attests; the owner tag matches. (code: crates/sovereign-crypto/src/account_key.rs;
crates/sovereign-p2p/src/backup.rs, backup_client.rs)

### Recovery-read limits (A2, A3)

**Status:** ◐ Partial — built in the host store and node; no mesh uses them yet.

- Listing hosted backups over the network requires an **exact owner tag** — no wildcard
  enumeration.
- Recovery read verbs (list, fetch fragment, request share) pass a per-peer rate limit, and
  fragment bytes served per owner tag per day are capped.
- Storing a fragment is accepted only from **paired** peers today; list and fetch are open,
  because a recovering device is unpaired by definition and fragments are opaque.
- List-carrying requests are capped per request.

(code: crates/sovereign-p2p/src/backup_host.rs, node.rs)

### Placement in the manifest (C3)

**Status:** ○ Specified, not built — decided by the M0 spike.

The signed manifest lists the **PeerIds of the fragment hosts**. A recovering device that has the
manifest already knows whom to contact; discovery reduces to reaching known peers, which the
relays already do. No content-discovery DHT. The spike fetched from two manifest-listed hosts
through a relay in 41 ms. The built manifest does not yet carry placement.

### Mesh hosting

**Status:** ○ Specified, not built.

- Opt-in hosting, surfaced as a user choice (`backup_host_enabled`, off by default, exists as a
  setting), with display of what one hosts and where one's fragments live.
- Accepting fragments from **non-paired** users replaces today's paired-only storage gate; it
  needs per-node and per-owner quotas, a give-to-get rule, and defenses against hosts that accept
  then drop fragments (periodic proof-of-possession audits — the heartbeat primitive — and
  re-replication when availability falls).
- Open questions: give-to-get economics; who runs the entry points (chapter 05). The mesh gets its
  own security design and review before it is built.

### Optional cloud backup

**Status:** ○ Specified, not built.

For users who want more: self-hosted storage, zero-knowledge providers, or cold storage, always
opt-in and never default. Same encryption as the backup; guardians remain the only social
recovery; the cloud is a convenience, never a security dependency.
