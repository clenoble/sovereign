# Silent failure modes

*A running catalogue of ways Sovereign fails **without saying so** — no
exception, no error line, nothing to grep. Started 2026-07-09 during the
Backup & Recovery e2e run preparation, after four of these turned up in a
single day.*

## Why this document exists

A loud failure costs minutes. A silent one costs a live run, or ships.

Each entry below was found by *checking a thing that appeared to work*.
None announced itself. Three of the four were caught only because someone
went looking for a positive confirmation rather than the absence of an
error — the difference between "no errors in the log" and "the line that
proves success is present in the log."

**The operating rule this implies:** never accept absence-of-error as
evidence of success. Name, in advance, the log line / file / return value
that will *prove* the thing worked, then go look for it. If a success
signal does not exist, that itself is the first bug to fix.

---

## 1. Tauri never binds `_`-prefixed command arguments

**Symptom:** the frontend calls `invoke('start_recovery', { ownerTag, … })`;
the command runs; the argument is simply absent. No error on either side.

**Cause:** the Rust parameter was `_owner_tag: String`. Tauri's arg binding
does not match an underscore-prefixed parameter name.

```rust
// silently broken — ownerTag never arrives
pub async fn start_recovery(_owner_tag: String, …)

// correct
pub async fn start_recovery(owner_tag: String, …)
```

**Why it is easy to plant:** the `_` prefix is the reflex fix for an
`unused_variables` warning while a command is stubbed out. Any command
written stub-first can carry this into production.

**Guard:** in `#[tauri::command]` fns, never leave a `_`-prefixed parameter
that the frontend passes. If the body genuinely doesn't use it yet, keep
the real name and silence the warning inside the body: `let _ = &param;`.

**Would have presented as:** "clicking Start does nothing", mid-run, with
nothing in any log. Found by claude-laptop, 2026-07-09
(`feat/backup-m1-security` @ `55042a1`).

---

## 2. A partial `[p2p]` TOML table disables P2P and only warns

**Symptom:** `config.toml` contains `[p2p] enabled = true`. P2P is off.

**Cause:** several `P2pConfig` fields have **no serde default** —
`listen_port`, `device_name`, `wifi_only`, `backup_host_enabled`,
`backup_quota_mb`. A table missing any of them fails to deserialize.
`AppConfig::load_or_default` then falls back to hardcoded defaults, in which
`enabled = false` and `backup_host_enabled = false`. The only trace is a
`tracing::warn!("Failed to load config from …")`.

So the config you wrote to *turn P2P on* is the reason P2P is *off*, and the
app starts normally.

**Guard:** write every field of `[p2p]`, not just the ones you're changing.
Better: give the fields `#[serde(default)]` so a partial table degrades
field-by-field instead of collapsing the whole config.

**Adjacent trap (INSTALLER-003, deliberate):** the config search path is
**only** `<project_root>/config/default.toml` — never a bare CWD-relative
one, so an attacker who controls the working directory cannot plant a
config. Correct hardening, but it means a `config/default.toml` sitting in
your shell's CWD is *silently ignored*. Pass `--config <path>` explicitly.

Found on PANDA2, 2026-07-09, while preparing the guardian profile for the
e2e recovery run.

---

## 3. Relay reservation failures are invisible

**Symptom:** the node logs `reserving relay circuit slot …`, connects to the
relay, identifies it (`agent=sovereign-relay`), and then nothing. No error.
Sixty seconds later the connection idles out. The node holds no reservation
and is unreachable through the circuit — but nothing ever said so.

**Cause:** `sovereign-p2p/src/node.rs` matches **only** the success event:

```rust
SovereignBehaviourEvent::RelayClient(event) => {
    if let libp2p::relay::client::Event::ReservationReqAccepted { … } = &event {
        info!("relay reservation accepted at {relay_peer_id} — reachable via circuit");
    }
    tracing::debug!("relay client event: {:?}", event);   // everything else
}
```

`ReservationReqFailed` (and every other relay-client event) falls through to
`debug!`. At default log levels a **refused** reservation and a **never
answered** one are indistinguishable — both look like silence.

**Guard:** log the failure arm at `warn!`/`error!`, not `debug!`. A behaviour
whose only observable success is an `info!` line needs an equally observable
failure. Until then, verify a reservation by the **presence** of
`relay reservation accepted`, never by the absence of errors.

Found on PANDA2, 2026-07-09 (pre-verify for the e2e run; reproduced on both
`/tcp/4001` and `/udp/4001/quic-v1`, disconnect at ~60.06 s both times).
Reported in `from-windows/0040`. Relay-server side is the laptop's.

---

## 4. A green build that proves nothing (methodological)

**Symptom:** `cargo build -p sovereign-app --features p2p` → exit 0, on a
machine where the same build had never been attempted. Reported as "the risk
is dead". It wasn't.

**Cause:** the branch under test (`feat/backup-owner-ui`, based on `38ac6bc`)
**predated** the code that actually breaks — the `GuardianEnroll*` events
(G2) and `recovery_finalize`. The build was green *because the code under
test was absent from the tree*. The binary that launched had the frontend's
passphrase step but no recovery commands behind it.

**Guard:** before a build result is allowed to mean anything, prove the tree
contains the code under test:

```bash
git merge-base --is-ancestor <commit-that-introduced-it> HEAD
grep -rl "<the symbol>" crates/…
```

Then say what was verified and what wasn't. Committed by claude-windows on
2026-07-09; retracted in `from-windows/0038` before it could gate the run.

---

## 5. Copy that is kind but false

Not a code failure — a documentation one, and it belongs here because it is
also silent.

The recovery wizard told the user, on a wrong passphrase: *"your guardians'
shards are still valid — just retype it."* At the time it was written, the
engine **did** mark the recovery `Failed` on a typo. The sentence was
reassuring and wrong, and nothing in any test would have caught it, because
it is a claim about the system made *in prose*.

It became true only when `recovery_finalize` (a236250) stopped failing the
recovery on a typo. **The fix made the copy honest; the copy did not make
the fix.**

**Guard:** UI copy that asserts a system property is a claim under test.
When you write "X is still safe", find the code that guarantees X, or write
something you can defend.

---

## Pattern

Four of these five share one shape: **the success path is observable and the
failure path is not.** The arg that binds, the config that parses, the
reservation that is granted, the code that is present — each announces
itself; none of their negations do.

That asymmetry is the bug behind the bugs. Where you can, make failure as
loud as success. Where you can't, *go looking for the success signal by
name* — and treat its absence as failure, not as "probably fine".

---

*Add to this file whenever a failure is found that did not announce itself.
Keep the entries concrete: symptom, cause, guard, and where it was found.*
