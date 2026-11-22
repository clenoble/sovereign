# 07 — Keys and Data at Rest

How Sovereign derives, wraps and stores its keys, what is encrypted on disk and what is not,
and how personas (primary and duress) are kept apart at rest. This chapter describes the key
hierarchy as built; the transport and guardian-recovery keys are covered only where they touch
the at-rest chain.

*Consolidates:* specification v1.3 §P2P Storage → Encryption Scheme, §Security Model →
Encryption & Key Management, §Core Tenets (Data Sovereignty); `CLAUDE.md` "Data-at-rest threat
model"; the project's archived plans (private): the v0.0.8 release plan (CRYPTO-001, CRYPTO-002
dispositions) and the hand-over of the shell port (KEK as content-chain root);
`RELEASE_NOTES_v0.0.4.md`–`v0.0.9.md`; code in `crates/sovereign-crypto`,
`crates/sovereign-db/src/{encrypted.rs,layered.rs}`, `crates/sovereign-app/src/{setup.rs,duress.rs,login_throttle.rs,account_key_migration.rs}`,
`crates/sovereign-shell/src/crypto.rs`.

*A note on identifiers.* Audit identifier **CRYPTO-001** is used in the code and plans for three
related items: Argon2id passphrase stretching, per-persona key isolation, and the accepted absence
of a device-bound key and of key rotation (mitigated by OS full-disk encryption). This chapter
cites it in each of those senses.

---

## Cryptographic primitives

**Status:** ✅ Built — (code: `crates/sovereign-crypto/src/aead.rs`, `master_key.rs`, `index_key.rs`, `mac.rs`).

- **AEAD:** XChaCha20-Poly1305, 256-bit keys, 192-bit random nonces generated per encryption.
- **Passphrase stretching:** Argon2id (v1.3), 64 MiB memory, 3 passes, parallelism 1, 256-bit
  output, with a per-install random salt.
- **Key derivation:** HKDF-SHA256 with versioned, domain-separated info strings.
- **Blind index:** HMAC-SHA256 under a dedicated Index Key.
- **Local tamper evidence:** HMAC-SHA256 under a key derived from the Device Key.
- **Secret hygiene:** every key type implements `Zeroize` + `ZeroizeOnDrop` and redacts itself
  in `Debug`. Secrets held by the UI layers outside these types are not uniformly zeroized
  [◐ — tracked in the private backlog].
- **No logging surface:** `sovereign-crypto` has no `tracing` dependency, by design — code that
  cannot log cannot log key material. Callers decide what to report.

*Note:* the ethics appendix (`doc/legal/sovereign_os_ethics.md` §2.4) names AES-256; the built
cipher is XChaCha20-Poly1305. This is a documentation inconsistency, not a design conflict.

## Key hierarchy

**Status:** ✅ Built — (code: `crates/sovereign-crypto/src/{master_key,device_key,kek,account_key,key_db,index_key,document_key,auth}.rs`).

```
passphrase + install salt
  └─ Argon2id ──▶ Master Key                       (never stored)
       ├─ HKDF("sovereign-device-key:<device_id>") ──▶ Device Key   (never stored)
       │     ├─ wraps the persona's KEK                 → auth.store
       │     ├─ wraps the persona's AccountKey          → auth.store
       │     ├─ encrypts the persona's auth probe       → auth.store
       │     └─ keys the local commit-history MAC
       └─ HKDF("sovereign-account-key:v1") ──▶ AccountKey  (first device only;
                                                            imported on paired devices)

KEK — random 256-bit, one per persona — root of the at-rest CONTENT chain
  ├─ seals the six per-entity key databases ──▶ Document Keys (random, KEK-wrapped, epoch-tagged)
  ├─ seals the Index Key (blind index)
  └─ seals the canary store

AccountKey — the user's shared data key (same on all of the user's paired devices)
  ├─ PII vault values and preserved raw bodies (ch. 09)
  ├─ the per-persona model-trust (TOFU) store
  └─ HKDF ──▶ session-log key · P2P transport key · pair key · backup tag · backup signing key

Recovery Key — separate secret, Shamir-split to guardians; restores KEK + AccountKey
```

### Master Key

**Status:** ✅ Built — (code: `crates/sovereign-crypto/src/master_key.rs`).

The Master Key is stretched from the passphrase with Argon2id and the install salt. It is held in
memory only for the duration of authentication and is never written to disk. The auth store
records which KDF it was created under, so an existing store always reopens with the same
derivation: stores written before the KDF field existed (v0.0.6 and earlier) open with the legacy
single-pass HKDF. A transparent upgrade to Argon2id is implemented for the CLI key store. Stores
created under the legacy KDF keep it until re-created; no transparent upgrade exists for the GUI
auth store.

*Replaces v1.x:* "Master Key (256-bit, TPM-protected)" / "Master Key in secure enclave/TPM" — no
TPM, secure-enclave or other device-bound key exists. The absence was accepted (CRYPTO-001,
v0.0.8) on the grounds that Argon2id bounds offline guessing and OS full-disk encryption is a
deployment requirement (see *At-rest model*). A device-bound key remains a possible future
hardening; nothing is specified for it.

### Device Key

**Status:** ✅ Built — (code: `crates/sovereign-crypto/src/device_key.rs`, `mac.rs`).

`Device Key = HKDF(Master Key, "sovereign-device-key:" + device_id)`. It is bound to the passphrase
and the device identifier, and its at-rest jobs are deliberately narrow: wrapping the persona's
KEK and AccountKey in the auth store, encrypting the auth probe that identifies the persona, and
keying the local commit-history MAC. It does **not** seal any store that holds content keys.

Because it keys the commit-history MAC, the Device Key is not entirely outside the content's
history: commits written under one Device Key do not verify under another, so a passphrase
replaced by guardian recovery (or any future passphrase change) leaves the older version history
failing its MAC check. Keying the commit MAC from the KEK, with a migration window for existing
tags, is open work tracked in the private backlog. (code: `crates/sovereign-db/src/encrypted.rs`)

### KEK — root of the at-rest content chain

**Status:** ✅ Built — (code: `crates/sovereign-crypto/src/kek.rs`, `key_db.rs`, `index_key.rs`; `crates/sovereign-app/src/setup.rs`; `crates/sovereign-shell/src/crypto.rs`).

The Key-Encryption Key is random (256 bits), generated once per persona, and stored wrapped by the
Device Key in that persona's auth-store entry. **Everything that decrypts user content hangs off
the KEK, not the Device Key**: the Document Keys are KEK-wrapped, and the on-disk stores that hold
them — the key databases and the Index Key — are sealed under the KEK.

This is what makes guardian access recovery work. Recovery reconstructs the KEK and AccountKey
and installs them under a **new** passphrase (new salt, hence a new Device Key). Because nothing in
the content chain depends on the passphrase-bound Device Key, the recovered KEK reopens the whole
chain without the old passphrase. The one exception is the commit-history MAC, which is keyed to
the Device Key (see *Device Key*): after recovery the content decrypts, but older commits no longer
verify. A design that sealed the key databases under the Device Key
would silently break recovery even though the keys inside are KEK-wrapped — a regression of exactly
this kind was caught by the first live recovery run on 2026-07-16 and corrected to this design.

**Migration.** A key database or Index Key file still sealed under the legacy Device Key is read
once at login (the passphrase is present) and re-sealed under the KEK, transparently, with no user
action.

### AccountKey — the shared data key

*Decided 2026-11-13 (Céline; ch. 01, P5), not built:*
- **The account key is to be rotated,** so that a lost device can be revoked. As built, it is one
  secret shared by every device, with no rotation.
- **This is the first step of the delegations:** a key restricted to a scope, revocable, that
  expires. It serves revoking a device, a watch function, a collaborator and a temporary reader.
- **The keys of model providers** are per device, or account-wide, as the user chooses (P17b).

**Status:** ✅ Built — (code: `crates/sovereign-crypto/src/account_key.rs`, `vault.rs`, `auth.rs`; `crates/sovereign-app/src/account_key_migration.rs`).

*Absent from the v1.x hierarchy diagram.* The AccountKey is user-scoped rather than device-scoped:
on the first device it is derived from the Master Key without the device identifier; a paired
device **imports** it during pairing and wraps it under its own Device Key. All of a user's paired
devices therefore hold the same AccountKey, which is what lets them read each other's synced vault
entries, raw bodies and session logs, and derive the same P2P transport key without an interactive
key exchange.

It encrypts directly (no per-record key): PII vault values (`PiiRecord.value_encrypted`), preserved
raw bodies of documents and messages (`body_raw_encrypted`), and the per-persona model-trust store.
Through HKDF with distinct `:v1` info strings it yields the session-log key, the sync transport key,
pair keys, the backup tag and the backup signing key.

The duress persona derives its own AccountKey from its own passphrase; an imported AccountKey is
never given to the duress persona.

**Migration.** v0.0.4 encrypted vault values and raw bodies directly under the Device Key. v0.0.5
re-keys every such row under the AccountKey once, idempotently, gated by a marker file. The
migration runs in the mobile app (and the frozen Tauri desktop build); the native shell does not
run it.

### Key databases (per entity type, per persona)

**Status:** ✅ Built — (code: `crates/sovereign-crypto/src/key_db.rs`; `crates/sovereign-app/src/setup.rs`; `crates/sovereign-shell/src/crypto.rs`).

Each persona has six key databases, one per encrypted entity type — documents, messages, threads,
conversations, contacts, share records — each mapping an entity id to its list of KEK-wrapped keys
(current plus any older epochs). Each file is a single AEAD blob (`nonce ‖ ciphertext`) sealed under
the KEK, written atomically with owner-only permissions. Separate files also mean separate locks, so
the message and document encryption paths do not contend.

A key minted at write time is persisted **before** the encrypted row is written; if the key database
cannot be saved, the write fails rather than storing a row under a key that exists only in memory
(CRYPTO-002 in the v0.0.8 review numbering).

### Index Key (blind index)

**Status:** ✅ Built — (code: `crates/sovereign-crypto/src/index_key.rs`; `crates/sovereign-db/src/encrypted.rs`).

One 32-byte HMAC-SHA256 key per persona database, sealed under the KEK. Encrypted fields that must
remain searchable — document titles, thread names, message bodies — are tokenized (lower-cased,
split on non-alphanumerics, tokens under three characters dropped, deduplicated, capped) and each
token's keyed hash is stored beside the ciphertext. A search hashes the query tokens and matches on
the hashes. The same token always hashes to the same value under one key: this leaks token
frequency and co-occurrence to whoever reads the database file, an accepted trade-off (CRYPTO-004)
that is strictly better than plaintext and is covered by OS full-disk encryption.

### Document Keys and rotation

**Status:** ◐ Partial — random per-entity keys and the epoch field are built; rotation is not.

Every encrypted entity gets its own random 256-bit key, wrapped by the KEK and tagged with a
rotation `epoch`. Keys are random, not derived: compromising one reveals nothing about another. The
key database stores a list of wrapped keys per entity so that older epochs remain available for
decrypting historical versions. (code: `crates/sovereign-crypto/src/document_key.rs`, `key_db.rs`)

**Rotation — ○ Specified, not built.** v1.x specifies rotation every 90 days or 100 commits, with
old keys retained for history. No code rotates a Document Key, the KEK or the Index Key; in practice
every entity has a single epoch. The KEK layer still allows a future rotation without re-encrypting
every document. No automatic schedule is claimed (the earlier "90-day automatic rotation" claim did
not survive the v0.0.9 fact-check).

## At-rest model

**Status:** ✅ Built — (code: `crates/sovereign-db/src/encrypted.rs`, `layered.rs`; `crates/sovereign-app/src/setup.rs`; `crates/sovereign-shell/src/crypto.rs`).

The at-rest model is **field-level encryption plus OS full-disk encryption**, not whole-database
encryption.

### Field-level encryption — `EncryptedGraphDB`

**Status:** ✅ Built — with known exceptions listed in the private backlog.

`EncryptedGraphDB` is a decorator over the raw graph store: it encrypts selected fields on write and
decrypts them on read, using the per-entity key databases and the Index Key. Encrypted today:
document titles and bodies (and their commit snapshots), message bodies, subjects and HTML bodies, thread names
and descriptions, conversation titles, contact names, notes and addresses, share-record URLs, and —
under the AccountKey — PII vault values and preserved raw bodies. Writes never place the plaintext
value in the raw store first: sensitive fields are blanked before the raw insert and filled with
ciphertext under the minted id, so an interrupted write leaves an empty field, never plaintext.
Commit-history rows carry a Device-Key MAC so that local tampering is detected on restore.

Installation fails closed: if the encrypted store cannot be built at login, the session is not
opened rather than writing plaintext.

### Login swap — `LayeredGraphDB`

**Status:** ✅ Built — mobile only; the native shell does not use it.

Mobile boots on a raw store behind a `LayeredGraphDB`, an atomic-swap indirection held by every
consumer, swapped at login: the inner store is replaced by an `EncryptedGraphDB` over the persona's
raw database, transparently to callers. The native shell opens no database until login. Rows
without nonces are read as legacy plaintext; the decrypt paths guard on the nonce columns.
(code: `crates/sovereign-db/src/layered.rs`; `crates/sovereign-app/src/setup.rs`;
`crates/sovereign-shell/src/crypto.rs`)

### What stays in clear, and why full-disk encryption is required

**Status:** ✅ Built (as a stated requirement) — the requirement is documented, not enforced by the software.

Metadata and structure stay plaintext on disk, because encrypting them opaquely would make the store
unqueryable: record ids, timestamps, graph edges and relationship types, positions, flags, and the
blind-index hashes. Also in clear today: PII record labels, entity names, domains and notes, and
share-record metadata other than the URL (ch. 09). Embedded SurrealDB with RocksDB has no native at-rest encryption, and fields not
listed above are stored in clear. **OS full-disk encryption (BitLocker, FileVault, LUKS) is
therefore a deployment requirement**, with field encryption as defence in depth on top of it
(disposition of ATREST-001; CRYPTO-001). Extending field encryption to further user-authored fields
is open work.

**Direction (Céline, 2026-11-13, on D12 of 2026-11-10).**
- **What D12 means here:** Sovereign runs on any machine the user owns, without depending on one
  hardware maker or one system vendor.
- **The full-disk-encryption requirement stays for now.**
- **The direction is to shrink what depends on it:** encrypt more of what stays in clear (ATREST-011,
  ch. 04).
- **Each host declares what its system can see.**

*Replaces v1.x:* "No plaintext on disk" — field-level encryption leaves metadata in clear by design;
the claim is replaced by the full-disk-encryption requirement above.

### Private file writes

**Status:** ◐ Partial — the private-write helper is built and used for the auth store, key databases, Index Key and salt; one mobile onboarding file is not yet written through it. (code: `crates/sovereign-crypto/src/fs_private.rs`)

All key-material files (auth store, key databases, Index Key, salt, wrapped keys) are written
atomically — temporary sibling, fsync, rename — and owner-only (`0600`) on Unix; on Windows the
per-user profile ACL scopes access (CRYPTO-005). Mobile onboarding still writes
`keystroke.store` with a plain `std::fs::write`.
(code: `crates/sovereign-app/src/tauri_commands/auth.rs`)

### The phone's system log

**Status:** ✅ Built in v0.0.13 (Céline, 2026-11-20; the way decided by the Fable maille, by delegation of Céline,
the same day). In the app's phone entry (`sovereign_app::run`) and the page's first script
(`frontend/static/phone-silence.js`).

The app writes nothing to the phone's system log (Android's logcat), which USB debugging, a bug report or a
forensic tool can read: the phone's entry installs no log subscriber and sets a panic hook that writes nothing,
and on a phone the page's first script silences its console and its uncaught errors. This is said of what the
app writes: the system, the web view, the device's drivers and the framework the app is built on still log under
the app's process. On v0.0.13's candidate the framework wrote one line at each start, naming a plugin's command and
no path. Held by source
readings of the phone's entry, by the page's own tests, and, on each release candidate's APK, by a measure of the
log through a whole setup and logins.

## Authentication and personas

**Status:** ✅ Built — (code: `crates/sovereign-crypto/src/auth.rs`; `crates/sovereign-app/src/{setup.rs,duress.rs}`; `crates/sovereign-shell/src/crypto.rs`).

### Auth store

**Status:** ✅ Built.

`auth.store` holds the salt, the device identifier, the KDF record, and **exactly two persona
entries in randomized order**, each labelled with a random opaque value. An entry contains an
encrypted fixed-length probe, the persona's wrapped KEK, and its wrapped AccountKey. Authentication
derives the Master and Device Keys once and tries each entry; the probe that decrypts identifies the
persona. The two entries are indistinguishable without a passphrase, and the probes are padded to a
fixed length so their sizes do not reveal which is which. Stores created before v0.0.9 keep
unpadded probes until re-created.

**Created once, never replaced by account creation** (decided by Céline 2026-10-19, for v0.0.11
rc2; built: `AuthStore::save_new`, and the phone's onboarding claim, code:
`crates/sovereign-crypto/src/auth.rs`, `crates/sovereign-app/src/onboarding_claim.rs`). **Every** first write of `auth.store` refuses an existing file (an exclusive
create, not a replace): account creation fresh or paired, on the phone and on the desktop, and the
backup recovery that rebuilds an account (ch. 06). Account creation and backup recovery are
**single-flight** on the phone, one claim excluding the other: a request
that arrives while one is in progress is refused ("Setup is already in progress") and one that
arrives after one has completed is refused ("Already onboarded — log in instead"); a refused request
changes no file. The claim is taken **before** the key derivation, so the check and the write
cannot be separated by it. *Why:* on rc1 a second tap on "Get started", during the ~10 s of key
derivation on a phone, passed the existence check, replaced `auth.store` and `salt` with a new KEK
while the first session had already sealed the key databases under the old one; the account could
never be unlocked again. v0.0.10 has the same window (the IPC-006 check is check-then-act).

**Never a torn store** (○ specified 2026-11-18, to build in v0.0.13, the torn `auth.store` of the
v0.0.12 review; written by PANDA2). A torn `auth.store` is a lockout for good: its presence sends to
login, and its load fails at every attempt.
- **AS1 — The first write is whole or absent.** A crash or a power loss at any point of the first
  write leaves no `auth.store`, or the whole new one, never a part of it, on every platform, Android
  included. It stays exclusive as above. Until v0.0.13, where hard links are refused (measured on
  Céline's phone, in the app's data directory), the first write fell back to writing the file in
  place, which a crash could leave torn.
- **AS2 — A later write replaces it whole or not at all.** Any write of an existing `auth.store`
  leaves, after a crash or a power loss at any point, the previous file or the new one, never a torn
  file, Android included. As built, one shipped path rewrites it: guardian access recovery installs
  the recovered store over the forgotten one (ch. 06), through the store's own replacing write, a
  temporary file and a rename.
- **AS3 — No stray file decides.** A temporary file a crash leaves beside it is never read as the
  store and never sends to login.

**What decides between login and setup is the store** (v0.0.12, lot 1, after the cloud review of
v0.0.11, findings 4 and 9). With encryption, the app shows the login screen when `auth.store`
exists and the setup wizard when it does not, on the phone as on the desktop; the
`onboarding_done` marker no longer decides. Once `auth.store` is written, a setup that is killed
or fails at a later step leaves an account that opens with its password; a failure after the
store is reported as an **incomplete** setup, not an error, and the wizard goes to the login
screen. At the first login that follows, the marker is written and a notice says once: "Setup
was interrupted before it finished. Your account works; your safety phrase, typing profile and
sample documents may not have been saved." (Céline, 2026-10-23: "un avis une fois"). In paired
setup the record of the source device is written before `auth.store`, so an account that opens
is paired; a failure to write it fails the setup before anything else is written. A pairing
retried after `auth.store` failed writes a new record that replaces the first: the paired list is
keyed by the device's peer id. **The duress passphrase must differ from the primary one**, refused
by the store itself on every path that creates one (fresh and paired setup on both platforms,
crowd backup and guardian recovery), with the desktop's words: "Duress password must differ from
your password." (Céline, 2026-10-23: fix it now, with no detection in the app; the advice to
testers was first meant for the v0.0.12 notes, and Céline decided on 2026-11-04 that the public
notes say nothing of the duress password or the canary phrase, so it is not there). **An account created on the
phone before v0.0.12 with one password for both personas** keeps working: with that password it
always opens the same persona — the primary or the decoy, whichever of the two entries the store
tries first, an order drawn at random when the store was created — and never the other. Nothing in
the app detects or reports it. Céline decided on 2026-10-23 that testers get a recommendation; its
wording, written by the laptop maille the same day, is to set the phone up again, pairing it again
from the desktop. The backend also checks what was typed twice (password, duress password,
safety phrase) before the claim and before any key derivation. *Status:* ● built in v0.0.12 lot 1
(phone); the desktop's interrupted setup is items 18 and 20.

**The desktop's interrupted setup** (v0.0.12 items 18 and 20, B5b: the laptop's proposal of
2026-10-24, taken whole by PANDA2; built, merged `7e2f6c9`). The desktop had no marker. Its wizard
now records that a setup **started** before `auth.store` is written, and writes a
**setup-complete** marker at the end, once the profile, safety phrase, starter lanes or sample
documents are in place. Once the store is written, a later failure drops the wizard for the login
form, with the error. At a login where the setup started and did not complete, a notice says once,
after the login: "Setup was interrupted before it finished. Your account works; your profile,
safety phrase and sample documents may not have been saved." (the desktop's list of what its
wizard saves after the store); then the marker is written, at the login of either persona. **An
account made before v0.0.12** (a store and neither record) logs in with no notice and gets its
marker silently, so existing desktop accounts see no false alarm (PANDA2's choice, 2026-10-24).
Join and recovery leave no notice. The UI side is ch. 12, *Onboarding and the auth gate*.

### Primary and duress personas

**Status:** ◐ Partial — the two personas and their separate stores are built as described below,
but the isolation is not complete yet: some state used after login is kept outside the persona's
own files, so a duress session can still show that it is not the primary persona — which persona
is which is what must not be readable (*ATREST-011*); that a duress option exists is not a secret
(found 2026-10-15
and by the 2026-10-16 review; tracked in the private backlog, to be closed by C-SESS and C-HOST:
one persona file layout, per-persona orchestrator state).

A user may set an optional **duress passphrase**. It opens a separate decoy workspace under
coercion. When none is set, the second entry is created with a random, unreachable passphrase, so
the auth store always has two entries and the presence of a duress persona cannot be read from it.

The duress persona is isolated at rest: its own raw database, its own six key databases and Index
Key, its own KEK and AccountKey, and its own model-trust store. A coerced login decrypts a physically
separate database under separate keys and cannot reach the primary persona's rows (CRYPTO-001).

### After login — every persona's state stays its own

**Status:** ◐ Partial — built 2026-10-17 and released in v0.0.11: per-persona orchestrator state
and session-log beacon, and guardian recovery reachable from the primary persona only (`296c8b7`);
the duress enrollment's waiting screen and the persona's own identity profile in the shell
(`9f64f9a`). Not covered: the phone host's profile (below). The one-time set-aside at migration
was built in v0.0.11 and replaced in v0.0.12 by the quarantine at every login (item 10).

What must not be readable is *which* persona a session is (ATREST-011). A live session must not
show it either. So, after login:

- **Every file the application writes for the session lives in that persona's own files.** No code
  running after login writes to, or reads its own state from, a location the personas share. In
  particular the **orchestrator's state** — the conversation log with its anchor and beacon, and
  the trust state — is per persona: the host gives the orchestrator the active persona's directory
  (to be absorbed by C-HOST's host wiring).
- **Guardian access recovery belongs to the primary persona.** The recovery roster, its bundle and
  the enrollment queue are the primary's; a duress session never reads or writes them.
- **In a duress session, guardian enrollment appears to work, waits for the guardian, and never
  completes** (Céline, 2026-10-17: *"en attente du gardien, qui ne devrait jamais enrôler depuis
  une session sous contrainte"*; replaces v2.0.27's stop with a message, which contradicted a
  running-sync screen). The session shows the same enrollment screen as an ordinary one — a code
  and a QR of the same form, "waiting for the guardian" — but **no offer is armed**: it carries no
  share, no enrollment key is derived from one, nothing listens for a guardian, and nothing is
  recorded on this device. A guardian's device that scans it gets no answer, as with a guardian
  who never connects. Closing the screen leaves the session as an ordinary enrollment left
  unfinished would.
- **No user-visible difference.** The same steps in a duress session and on a fresh install with no
  duress passphrase show the same strings; nothing tells the session that another persona exists.
- **Migration** (Céline, Q2): the orchestrator directory used before this change, shared by both
  personas, becomes the **primary** persona's; the duress persona starts with an empty one. A log
  that does not verify under the persona's key is moved unread into a quarantine and a fresh log
  starts, with the alteration notice: **at every login, not once** (v0.0.12 item 10, ch. 08 *Session
  log*). Until v0.0.11, a one-time rule set such a log aside with a gentler notice, recording "done"
  in a plain marker file that nothing authenticated (review finding 17): deleting it made a later
  alteration pass for the migration. The marker now decides nothing: it is no longer written, and
  one left behind is ignored, never deleted. Existing `set-aside-*` directories stay as they are,
  and no read path opens a `set-aside-*` or `quarantine-*` directory: nothing in them reaches the
  model, the conversation history or any other read.
- **The shell's user profile is split** (Céline, 2026-10-17: now, unless it pulls in too much of
  C-SESS). **Device preferences** read before login — the theme and the bubble style — stay in
  one file shared by the personas: they identify no one. **The persona's identity** — display
  name, nickname, designation — lives in the persona's own files and is read and written only
  after login; before login nothing shows it. At migration the existing profile's identity becomes
  the primary's; the duress persona starts with defaults. (The phone host's profile is the same
  class of issue and is not covered here.)

### Decoy seeding

**Status:** ◐ Partial — built in the Tauri app's login (the phone, and the frozen Tauri desktop).
**The desktop shell, the only desktop UI shipped, does not seed:** `seed_duress_db` lives in
`sovereign-app` and the shell never calls it. Checked at v0.0.12 rc1 (`20425d1`, 2026-11-01): a
duress account created in Céline's test opened empty. Completion is tracked in the private backlog.

The duress workspace is seeded with plausible, innocuous content, spread over a randomized past so
the workspace does not look created in one instant. Content variation
across installs is open work. Its threads are stamped, as a real workspace's are, with the duress
persona's own peer id — derived from that persona's device key — never with the configured device
name nor the primary's id (v0.0.12 item 22). (code: `crates/sovereign-app/src/duress.rs`)

### Login timing padding

**Status:** ◐ Partial — built in the Tauri app's login only (`tauri_commands/auth.rs`). The desktop
shell has no padding: nothing in `sovereign-shell` refers to it (checked at rc1, `20425d1`,
2026-11-01). Completion is tracked in the private backlog.

The persona-dependent part of login (the duress persona opens, and may seed, a separate database)
is padded to a constant floor so that login latency does not reveal which persona unlocked
(SIDECHANNEL-001). (code: `crates/sovereign-app/src/tauri_commands/auth.rs`)

### Login throttle

**Status:** ◐ Partial — built in the Tauri app's login only (`tauri_commands/auth.rs`, with its
known load/save race). The desktop shell has no throttle or lockout: nothing in `sovereign-shell`
refers to them (checked at rc1, `20425d1`, 2026-11-01). Completion is tracked in the private backlog.

A persisted attempt tracker enforces a lockout after a configurable number of failures within a
window (defaults: 10 attempts, 300 s). Its purpose is to stop scripted online guessing through the
application's command surface; it does not defend against an attacker holding the disk, who can
attack the auth store directly — that case is bounded by Argon2id (CRYPTO-002).
(code: `crates/sovereign-app/src/login_throttle.rs`)

Since v0.0.13 it counts a wrong password only (LE4, below).

### Login errors say what failed — contract (LE)

**Status:** ● LE1 to LE6 built in v0.0.13 (2026-11-19), read on main: the core returns a wrong password's
error only when no entry's probe opened; the desktop's login, the app's login and onboarding, and the command
line turn it into « Invalid password », and every other failure into one of the two other sentences, through one
shared type, with the data folder's paths in plain words; the app's attempt tracker and login screen count a
wrong password only; the app's login error is an alert. Céline, 2026-11-19, through the Fable maille:
« pas forcément résoudre l'échec intermittent et peut-être le bug sous-jacent, mais a minima avoir un
message d'erreur exact quand il y a un problème. » Contract and tests by WSL, code by PANDA2, second run
by the laptop.

*Why:* until v0.0.13 both hosts turned every failure of authentication into « Invalid password »: a
key derivation that failed, a key that would not unwrap after the password had matched, and, on the
desktop, a persona database that would not open, whose error was dropped. A damaged store and a
mistyped password read the same, so a user with the right password was told it was wrong, and nobody
could tell the cases apart afterwards. In the app, each such failure also counted toward the lockout.

The words below are what the user reads; the three are the same on the desktop and in the app.
- **A wrong password:** « Invalid password » (unchanged).
- **The password could not be checked** (the key derivation failed, before any probe was tried):
  « Your password could not be checked: {cause}. This is not a wrong password. »
- **The password is right, and the account did not open** (a probe opened, then the keys' unwrapping
  or the session's installation failed): « Your password is right, but your account could not be
  opened: {cause}. »

`{cause}` is the underlying error's own text, with every path and file name of the data directory
replaced by plain words (LE3).

- **LE1 — The core tells a wrong password apart.** `AuthStore::authenticate` returns an error of its
  own for a wrong password: the key derivation succeeded and no entry's probe opened. It returns it in
  that case only. A derivation that fails, and an unwrap that fails after a probe opened (the KEK, the
  AccountKey), each return another error, which carries its cause. Until v0.0.13 a failed unwrap and a
  wrong password returned the same error.
- **LE2 — « Invalid password » means a wrong password.** The desktop's login, the app's login and the
  command line say « Invalid password » (the command line: « authentication failed: wrong password »)
  for LE1's error only.
  - A derivation that fails reads the second sentence.
  - Any failure after a probe opened reads the third: the keys' unwrapping, and the session's
    installation (opening the persona's database, its key databases).
  - At the end of a first setup the desktop's « Your account was created, but it could not be opened:
    {error} » keeps its words, and `{error}` is the cause alone, by LE3. LE1's error, should it come
    there, reads « Invalid password », which is exact: no probe opened with the password just chosen.
- **LE3 — The cause is kept, and names no file.** The message carries the cause's own text, never a
  fixed word in its place. Opening a persona's database returns its error instead of dropping it, so a
  database that cannot open says why. In what is shown, each path or file name of the data directory
  is replaced by plain words (« the database », « a key database »), and the rest of the text is kept as
  it is. *Why:* a path can name a persona, and LE5 holds for the cause too. The lot adds no log line that names a persona's file where none did: the full cause,
  with its paths, is written nowhere new.
- **LE4 — The lockout counts wrong passwords only** (the app). The persisted attempt tracker records a
  failure for LE1's error only, and the login screen counts only « Invalid password ». An error that
  arrives with no text never reads « Invalid password ».
  - **No guess escapes the throttle:** a wrong password always yields LE1's error, so every wrong guess
    is still counted. Only failures that are not a wrong password, which a guess cannot cause, stop
    counting.
- **LE5 — The words tell no persona apart.** The three sentences are the same whichever entry's probe
  opened, primary or duress, and LE1 holds for both. The same failure made for each persona (a database
  that is locked, one that is damaged) reads the same on screen, character for character, cause
  included. The login timing padding is unchanged.
- **LE6 — A login failure is announced.** Each of the three sentences, when the login screen shows it,
  is in an alert (`role="alert"` in the app), so a screen reader says it without the person moving to
  it. The desktop's login form gives its error the alert role already.

### The lock at the device's sleep

**Status:** ○ Decided 2026-11-13 (Céline), to be built with C-SESS. Neither host locks today:
- **the phone** keeps its session open when the screen goes off and when it comes back. Its screen
  protection hides content from screenshots and from the recent-apps view; it is not a lock;
- **the shell** has no logout and no lock.

**Decided** (Céline, 2026-11-13: « A la mise en veille de l'appareil seulement pour commencer, le
verrou efface la clé. A programmer avec C-SESS »):
- **The trigger: the device's sleep, and only that, to begin with.** No lock on inactivity or on
  leaving the app, for now.
- **The lock clears the key.** The session's keys leave memory, so a session that comes back from
  sleep can be read only after a new login. An overlay that hides the screen while the keys stay
  in memory does not meet this.
- **It is built with C-SESS,** the session service that holds the keys and their lifetime.

**What the contract will have to say** (to be written with C-SESS):
- **On each host, what the device's sleep is:**
  - on the phone, the screen going off;
  - on the desktop, the system's sleep and hibernation.
- **When the lock happens, said exactly.** On the desktop, the keys must be cleared before memory is
  written to the hibernation file.
  - **The signal is named per platform, and tested on the machines that carry it.** Many recent
    laptops sleep in a low-power state where the screen goes off without a classic suspend signal.
  - Whether the console's display going off also counts as sleep on the desktop is Céline's to
    confirm when the contract is written.
- **The lock ends everything a switch ends** (C-AREA, AR4): the node, the keys, the timers and
  background tasks, the pending proposals, and the interface's state. One test serves both. A login
  after the lock may open the other persona.
- **What is not yet saved when the lock comes.** What can be saved is saved before the keys are
  cleared, and anything interrupted leaves the store whole.
- **The windows that stay open today** (chat, mail, personal data, devices) must be part of the
  contract.
- **Whether key memory is pinned** (kept out of swap), with the cost said. That would remove the
  second limit below for the keys Sovereign holds.

**What this does not protect against:**
- **A device that is taken while awake.** Its session stays open. A computer set never to sleep is
  never locked by this rule.
- **Keys written to the swap file before the sleep.** No key memory is pinned today, so the system
  may page a key out to disk at any time.

### Canary phrase

**Status:** ◐ Partial — enrollment and sealed storage are built; showing the phrase after login is not.

During onboarding the user may set a personal canary phrase, sealed under the KEK. Its purpose is to
be shown back after login, so that a substituted or tampered environment — which cannot know the
phrase — is noticed. Display after login is not wired. A runtime detector primitive exists in the
crypto crate without a caller. (code: `crates/sovereign-crypto/src/canary.rs`)

**It is a secret** (Céline, 2026-10-28), so it is entered like a password: masked on screen, its
length only for a screen reader, on the desktop and in the app (ch. 12, *The canary phrase is
entered like a password*, K1–K4). The pairing code is not a secret: it is valid for ten minutes and
meant to be shared (Céline, same day).

### Keystroke dynamics

**Status:** ○ Specified, not built — deferred.

A typing-rhythm check on the passphrase. The crypto primitive and an enrollment step exist; login
does not compare against the reference, and the enrollment step is not enabled by default.
(code: `crates/sovereign-crypto/src/keystroke.rs`)

Typing timings are optional input to login: a missing, partial or malformed sample (a password
filled in by the system's autofill produces key events without a key) is ignored and **never
makes a login fail** (2026-10-17; Android autofill was refused in v0.0.10).

### One persona, one area — contract (C-AREA)

**Status:** ✅ Built for v0.0.13 on the shell and the phone app (lot L1c and its follow-up); each
guarantee is held by the walks' tests on both hosts. Not built: what an « As built » line below
says. The class of defect it closes — a file the two personas share, read or written by a
session — came back in three releases running.

**Decided:**
- **2026-11-04 (Céline),** the scope of v0.0.13: the walk below, written first by an author other
  than whoever fixes the two defects it covers.
- **2026-11-13,** the walk runs again at the end of each release, extended to the stores that
  release created.

**Guarantees (C-AREA, each one a test):**
- **AR1 — The walk.** A duress session drives every feature the host offers, as built.
  - **For example:** create, edit, move, delete and restore a document; threads; search; chat and
    the assistant's actions; the embedded browser; pairing and sync; guardian enrollment; settings;
    trash and purge; logout. The list is illustrative; the test names each feature it drives.
  - **Pairing and sync** run with a second node in the same process, as the P2P end-to-end tests
    already do.
  - **A feature the walk cannot drive is a failure, not a skip.**
  - **What it asserts:** no file outside the duress persona's own area is created, changed or
    deleted. The test snapshots the whole Sovereign directory, with each file's existence and
    content, before and after the walk.
  - **It runs on both hosts:** the shell and the phone app's backend.
- **AR2 — Shared files are named, with their reason.** A file both personas use is a stated
  exception in the test, with the reason it must be shared.
  - **Each exception says whether a duress session may change it, or only read it,** and the test
    asserts which. `auth.store` may change: it holds both personas by design (*Auth store*), and a
    duress login is recorded like any login. Any other shared file that changes fails AR1.
  - **The verdicts, file by file** (2026-11-13, from the walk's first runs; « into the area » means
    the file moves into the persona's area):

    | File | Verdict | Reason |
    |---|---|---|
    | `auth.store` | exception, **may change** | holds both personas by design; a duress login is recorded like any login |
    | `crypto/salt` | exception, **read only** | the key-derivation salt, read before login to open either persona; the same bytes whoever logged in last |
    | `crypto/device_id` | exception, **read only**, never created by a duress session | a random install id. On the phone it is created at the P2P node's start, and a duress session starts no node (AR4) |
    | the device preferences read before login (theme, bubble style), in their own file, `device_preferences.json` | exception, **may change** | the login screen needs them; they say nothing of which persona is open. Today they sit in `user_profile.json`, which moves into the area |
    | the login's rate-limit state (`login_attempts.json` on the phone) | exception, **may change** | counted before any persona is known |
    | `onboarding_started`, `onboarding_done`, `account_key.migrated` | exception, **written once, by whichever persona logs in first** | they say that a setup or a migration finished on this device, not by whom; after that they are read only |
    | `canary.store`, `keystroke.store` (phone) | exception, **read only** | read at login, before a persona is known |
    | `crypto/paired_devices.json` | **into the area** | pairings are a persona's (AR3) |
    | `crypto/sync_versions.json` | **into the area** | a persona's sync state |
    | `user_profile.json` (both hosts) | **into the area** | it holds the persona's name; the walk saw the primary's shown in a duress session |
    | `device_name.txt` | **into the area** | it is what the persona's paired devices are shown; it differs per persona |
    | `comms.toml` | **into the area** | it holds the mail address (C-COMMS: each store's persona) |
    | the recovery store, the same four files on both hosts: `crypto/recovery_card.json`, `crypto/recovery_roster.enc`, `crypto/recovery.bundle` and `crypto/guardian_enrolled.pending` | **the primary's only** | guardian recovery belongs to the primary (*After login*, above); a duress session neither reads nor writes it, and its decoy answers as a persona with no roster |
    | `recovery/` (both hosts) | **the primary's only** | an access recovery in progress on this device: what it recovers is the primary's account. It is started from the login screen by someone without the passphrase, so before any login. A primary login clears it (the owner has the passphrase); a duress login leaves it alone, and a duress session neither reads nor writes it |

  - **Any file not in this table** fails AR1 or AR6, until it is given a verdict.
  - **What the duress persona's own copies start with** (Céline, 2026-11-13): « la persona de
    contrainte a des noms d'appareil et nom de profil random générés automatiquement. L'utilisateur
    peut les changer s'il le souhaite ensuite en se loguant sur cette session. »
    - At setup, the duress persona gets a device name and a profile name generated at random.
      **An install set up before v0.0.13** never passes through setup again: there, the same draw
      happens at the first duress login that finds no names of its own.
    - They are never copied from the primary's, and the primary's later changes never reach them.
    - **On a device that has an account, before any login, no command answers or writes a persona's
      identity,** on either host. A process with no session has no identity directory. Setup, on a
      device with no account yet, is the one case that writes an identity before any login.
      - **The commands that create an account or answer its designation** are refused once the
        device has an account, inside a duress session too. A check that reads no file and names no
        persona still answers: the new password's policy, which recovery uses from the login screen.
    - **The test:** after setup, the duress session's device name and profile name differ from the
      primary's, and a primary change leaves them as they were.
    - **What this does not protect against** (Céline, 2026-11-13: « Laisser ainsi »): a generated name
      next to a name the user typed may look generated. That risk is accepted, and the user may
      change the duress names in that session.
    - The same question for `comms.toml` comes with C-COMMS.
- **AR6 — Reads count too: a differential walk.** The walk runs in a duress session twice: once with
  the primary's files and the unnamed shared files present, and once with them absent. Everything
  the session shows and writes is the same in both runs.
- **AR7 — Nothing is written outside the Sovereign directory.** The test checks the other places a
  host could write: the temporary directory, logs and the embedded browser's profile.
  - **The browser keeps one profile per persona,** in that persona's area (Céline, 2026-11-13: « Un
    profil par persona »).
  - As built, the shell keeps it in the open persona's area, and builds no browser without a
    session. The phone app has no embedded browser. The desktop app, frozen, still uses the
    platform's default profile.
- **AR8 — No new shared path.** A check in the gate counts, per crate, the calls to every function
  that returns a path under the Sovereign directory without taking a persona. It fails if a count
  grows.
  - **The check finds the functions itself:** from the roots (those that read `SOVEREIGN_DATA_DIR`
    or `.sovereign`), it follows to a fixed point every function that returns a path and takes no
    persona.
  - **A new function of that kind fails the check** until it is named, and so does a named one that
    disappears.
  - **Its limit:** it reads names, not paths.
  - **The counts are recorded by the check** when it is first written. The spec does not carry them.
  - C-SESS, the structural cure, comes later.
- **AR3 — The paired-devices list belongs to its persona.** A duress session never changes the
  primary's list. It has no pairing of its own either (AR4).
  - A list that fails to decrypt is **never replaced** by an empty one. It is kept as it is, and
    the failure is reported (ch. 03, *What cannot be read is never shown as empty*).
- **AR4 — A duress login never runs on the primary's keys, and runs no P2P node** (Céline,
  2026-11-13: « Aucun moteur »).
  - **In a duress session, Sovereign does not sync:** no node is started, nothing is announced on
    the network, and no device can be paired. Guardian enrollment keeps its decoy waiting screen.
  - **Pairing shows the same decoy** (Céline, 2026-11-13: « Le même leurre que les gardiens »). The
    usual screen, with a code and a QR of the real form, waits forever. Nothing is armed, and
    nothing is announced.
  - **At a switch to the duress persona,** the primary's node is stopped and its KEK cleared. This
    holds on the phone after a setup that ends « incomplete », and at every switch.
  - **At a switch back to the primary,** the primary's node starts under its own device key.
  - **A switch ends everything the previous session started:** its node, its keys, its timers and
    background tasks (the hourly purge, the personal-data sweep), and its pending proposals. Nothing
    started under one persona acts under the other.
    - **The test:** a primary session, then a switch to duress in the same process. The test forces
      the purge's tick and the sweep's pass. The primary's store and files do not change, and no
      proposal of the primary's can be confirmed.
    - **An approval counts in its own persona only.** The orchestrator's state, earned trust
      included, is the persona's (ATREST-011): an approval given in one persona never moves the
      other's count towards auto-approval.
  - **The interface starts from nothing at a switch.** No window, list, chat or notice of the
    previous session remains in it.
    - As built, the interface never goes back from an open session to the login screen. The one
      login that follows other screens in the same interface is the phone's setup that ends
      « incomplete ». A lock, or C-SESS, would add more.
    - **The test,** in the interface's own suite: after a login, every store is in its initial state.
  - **What this does not protect against:** while a duress session is open, the device is absent
    from the network. A paired device of the primary sees it go offline.
  - **The screens of a duress session say that nothing runs** (Céline, 2026-11-14: « Laisser ainsi »):
    the phone reports sync as not running, the Devices window shows no address, and « sync now »
    answers that no device is paired. Someone who knows Sovereign may notice the difference from a
    primary session. That risk is accepted.
- **AR5 — The walk grows with each release.** At the end of each release, it runs again, extended
  to every store the release created. In v0.0.13, that is the personal-data sweep's review queue
  (ch. 09).

### ATREST-011 — personas distinguishable by file name

**Status:** ○ Decided 2026-10-10 (Céline), to be built.

**Decision (option b).** The existence of a duress persona is not a secret — every user sees the
option at onboarding. What must not be readable from the disk is *which* persona is which, and
whether the option was activated. So: **one directory per persona under a neutral name**, and
**at least two, possibly three, persona directories always exist**, whether or not the user
enabled a duress passphrase at onboarding. Consequence to build in: a persona slot the user
never activated must be indistinguishable on disk from one in use (same files, plausible sizes
and dates), otherwise the unused slot gives the answer away. The single indistinguishable
container remains a possible later step. The orchestrator's state (session log, trust, profile)
moves into the persona directory too (ch. 08).

Per-persona files are distinguished by name, so the *partition* is readable by whoever holds the
disk while only the *content* is encrypted. The principle that names the fix: **transparency is owed
to whoever holds the key, not to whoever holds the disk.** The distinction belongs in what the
passphrase opens — indistinguishable key slots in one container — rather than in file names that
differ before any attempt is made. Also recorded in ch. 04 (*Export*), where it constrains the
general export.

## Not settled

**Status:** Open.

- **Device-bound key / TPM.** Accepted as absent (CRYPTO-001); no design is specified.
- **Key rotation.** The data model supports it; no schedule, trigger or procedure is specified
  beyond the v1.x default quoted above.
