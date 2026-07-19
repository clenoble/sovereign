# Feature 1 — Guardian Access Recovery: implementation record

*Build log, 2026-07-10/11. **Canonical design is the spec**
(`doc/spec/sovereign_os_specification.md` §Guardian Social Recovery); this is a
dated record of what was built and the choices made along the way, per Céline's
request to record decisions. Not a spec, not a plan — history.*

Branch: `feat/backup-m1-security`. All commits below are local until the NAS
(`origin`) is reachable to push (it was off-LAN at the end of the session).

## What Feature 1 is (recap)

Guardian **Access** Recovery: the user forgot their passphrase; their data is
present on their synced devices; 3-of-5 guardians restore **access**. Guardians
hold Shamir shares of a dedicated random **Recovery Key** that wraps the account
secrets (KEK + AccountKey). Reconstruct 3 shares → open the bundle → re-wrap the
secrets under a **new** passphrase. Data (already synced) then decrypts. This is
distinct from Feature 2 (crowd data backup / mesh), which is deferred.

## Built (all committed, all tested green — 168 crypto + 138 p2p unit tests)

| Inc | Commit | What |
|---|---|---|
| 1  | `5aa30a9` | `sovereign-crypto::recovery_key` — RecoveryKey + RecoveryBundle (seal/open account secrets, 3-of-5 split). 5 tests. |
| 2a | `1e58bcb` | `sovereign-crypto::recovery_roster` — owner RecoverySetup: generate→seal→split-5→**arm-only-at-5**. 5 tests. |
| 2b | `9d379a8` | `sovereign-app` — KEK retention in AppState; encrypted roster persistence; `begin_guardian_enrollment`/`list_guardians`; arm the offer with a Recovery-Key share. |
| 4a | `1c5783a` | `sovereign-crypto::auth` — `AuthStore::create_with_secrets`: recovery *install* (re-wrap recovered KEK+AccountKey under a new passphrase). 1 test. |
| 3  | `37ce2aa` | `sovereign-guardian` — G3 `run` serving mode (relay reservation + BackupHost serving of held shards, 72h/approve-deny). |
| 4b-core | `c3960ef` | `sovereign-p2p` — `request_recovery_share` client + `AccessRecovery` driver (gather shares → reconstruct → open bundle). 4 tests. |
| 4b-app | (local) | `sovereign-app` — RecoveryCard (pre-login) + pre-login commands: `access_recovery_available/start/poll/status/finalize/cancel`. |
| 5  | (local) | `sovereign-app` — invalidate-by-passphrase: a successful local login clears an in-progress recovery. |

## Decisions made (recorded for review — change any you disagree with)

1. **Arm-at-5** (your call, confirmed): recovery is offered only with the full
   5-guardian roster. `RecoverySetup::is_armed()` = 5 enrolled.
2. **Identity proof = out-of-band human** (your call): nothing digital stored;
   each owner↔guardian pair agrees a shared memory/object/question. In the spec.
3. **Guardian serving via BackupHost-reuse** (your call): the guardian `run`
   mode loads its custody shards into a `BackupHost` and serves via the existing
   audited `RequestShard` path (72h + approve/deny + A2 rate-limits), rather
   than refactoring shared `node.rs`. Consequence: the guardian gains a direct
   `sovereign-db` dep (`test-utils` → `MockGraphDB`, the lightest empty DB for a
   node that never syncs — documented "fattening").
4. **`RequestShard` reused as-is** (autonomous): its `shard_data` is opaque at
   the protocol level, so it carries a Recovery-Key share unchanged; the F1
   client reads it as a raw share instead of a data-path payload. No protocol
   change.
5. **`finalize` overwrites the forgotten `auth.store`** (autonomous): this is
   the recovery outcome, gated by holding ≥3 guardian shares (each needing
   approval + 72h). The recovered KEK/AccountKey are the same secrets the old
   store held, so synced content stays decryptable — only the passphrase
   wrapping changes. New salt + device-id + random duress decoy.
6. **Shamir shares aren't self-authenticating** (design fact, tested): a
   wrong/garbage well-formed share is accepted at ingest, but the bundle's AEAD
   rejects it at `open()` — recovery fails loudly, never returns wrong secrets.
7. **`--auto-approve` is DEV/E2E only**: the shipping mobile guardian app
   approves after the human verifies the requester out of band.
8. **Invalidate-by-passphrase is LOCAL-only for now** (autonomous, scope
   recorded): login cancels a recovery on the owner's own device. The
   cross-device half (owner login → guardians auto-deny an attacker's recovery)
   needs an owner→guardian liveness channel and is deferred; until then the
   cross-device case relies on the built all-5-notify + 72h + guardian deny.

## NOT built yet (honest gaps)

- **Frontend UI** — owner roster UI, guardian app UI (D2/D3), and the
  access-recovery wizard are PANDA2's domain. The backend commands exist and
  are named for the UI; the Svelte/shell wiring is not done here.
- **Live multi-process e2e** — the pieces are unit-tested, but an owner-enrolls
  → guardian-serves → recover run across processes (like the M0/backup runs)
  has NOT been executed. That's the natural next validation.
- **Guardian heartbeat (G3 liveness)** — the serving loop answers RequestShard
  but not `GuardianPing` yet (custody has `prove_possession`; the loop doesn't
  wire it). Health-monitoring, not on the recovery critical path.
- **G4 roster rotation** — replace a lost guardian → re-split + epoch bump. Not
  built (separate workstream from the F1 recovery core).
- **Cross-device invalidate** — see decision 8.

## How to test the vertical slice (when you're back)

Rough multi-process shape (all local, or laptop↔PANDA2):
1. Owner (full app, p2p on): onboard, `begin_guardian_enrollment` ×5, enroll 5
   `sovereign-guardian` instances (each `enroll` then `run --relay <seed> --auto-approve`).
   Check `list_guardians` shows 5/5 armed; `recovery_card.json` + `recovery.bundle` written.
2. "Forget the passphrase": on the same device, from the login gate,
   `start_access_recovery` → `access_recovery_poll` until 3 shares
   (SOVEREIGN_RELEASE_DELAY_SECS=short) → `access_recovery_finalize` with a NEW
   passphrase → logged in, synced content decrypts.
Guardians reachable via relay circuits (M0 machinery, proven).
