# 07 — Keys and Data at Rest

How Sovereign derives, wraps and stores its keys, what is encrypted on disk and what is not,
and how personas (primary and duress) are kept apart at rest. This chapter describes the key
hierarchy as built; the transport and guardian-recovery keys are covered only where they touch
the at-rest chain.

*Consolidates:* specification v1.3 §P2P Storage → Encryption Scheme, §Security Model →
Encryption & Key Management, §Core Tenets (Data Sovereignty); `CLAUDE.md` "Data-at-rest threat
model"; `doc/plans/archive/plans/v0.0.8-release-plan.md` (CRYPTO-001, CRYPTO-002 dispositions);
`doc/plans/archive/plans/HANDOVER-shell-port-and-rename.md` (KEK as content-chain root);
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
  [◐ — see the native-shell parity items in `doc/plans/BACKLOG.md`].
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

*Replaces v1.x:* "No plaintext on disk" — field-level encryption leaves metadata in clear by design;
the claim is replaced by the full-disk-encryption requirement above.

### Private file writes

**Status:** ◐ Partial — the private-write helper is built and used for the auth store, key databases and Index Key; two mobile onboarding files are not yet written through it. (code: `crates/sovereign-crypto/src/fs_private.rs`)

All key-material files (auth store, key databases, Index Key, salt, wrapped keys) are written
atomically — temporary sibling, fsync, rename — and owner-only (`0600`) on Unix; on Windows the
per-user profile ACL scopes access (CRYPTO-005). Mobile onboarding still writes `salt` and
`keystroke.store` with a plain `std::fs::write`.
(code: `crates/sovereign-app/src/tauri_commands/auth.rs`)

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

### Primary and duress personas

**Status:** ✅ Built.

A user may set an optional **duress passphrase**. It opens a separate decoy workspace under
coercion. When none is set, the second entry is created with a random, unreachable passphrase, so
the auth store always has two entries and the presence of a duress persona cannot be read from it.

The duress persona is isolated at rest: its own raw database, its own six key databases and Index
Key, its own KEK and AccountKey, and its own model-trust store. A coerced login decrypts a physically
separate database under separate keys and cannot reach the primary persona's rows (CRYPTO-001).

### Decoy seeding

**Status:** ◐ Partial — built in the shared login path; completion tracked in the private backlog.

The duress workspace is seeded with plausible, innocuous content, spread over a randomized past so
the workspace does not look created in one instant. Content variation
across installs is open work. (code: `crates/sovereign-app/src/duress.rs`)

### Login timing padding

**Status:** ◐ Partial — built in the shared login path; completion tracked in the private backlog.

The persona-dependent part of login (the duress persona opens, and may seed, a separate database)
is padded to a constant floor so that login latency does not reveal which persona unlocked
(SIDECHANNEL-001). (code: `crates/sovereign-app/src/tauri_commands/auth.rs`)

### Login throttle

**Status:** ◐ Partial — built in the shared login path; completion tracked in the private backlog.

A persisted attempt tracker enforces a lockout after a configurable number of failures within a
window (defaults: 10 attempts, 300 s). Its purpose is to stop scripted online guessing through the
application's command surface; it does not defend against an attacker holding the disk, who can
attack the auth store directly — that case is bounded by Argon2id (CRYPTO-002).
(code: `crates/sovereign-app/src/login_throttle.rs`)

### Canary phrase

**Status:** ◐ Partial — enrollment and sealed storage are built; showing the phrase after login is not.

During onboarding the user may set a personal canary phrase, sealed under the KEK. Its purpose is to
be shown back after login, so that a substituted or tampered environment — which cannot know the
phrase — is noticed. Display after login is not wired. A runtime detector primitive exists in the
crypto crate without a caller. (code: `crates/sovereign-crypto/src/canary.rs`)

### Keystroke dynamics

**Status:** ○ Specified, not built — deferred.

A typing-rhythm check on the passphrase. The crypto primitive and an enrollment step exist; login
does not compare against the reference, and the enrollment step is not enabled by default.
(code: `crates/sovereign-crypto/src/keystroke.rs`)

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
