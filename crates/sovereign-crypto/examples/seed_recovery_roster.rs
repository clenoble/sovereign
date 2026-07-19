//! Dev tool: seed an F1 guardian roster into an existing workspace, so the
//! owner UI's *populated* state can actually be looked at.
//!
//! Why this exists: the roster is sealed under the KEK, and the KEK only exists
//! after a real login. Without this, the only renderable states are "no-auth"
//! and "not set up" — the populated roster (slot labels, dates, armed-at-5)
//! could be unit-tested but never *seen*. The one run-blocking bug of the F1 UI
//! (an enrollment button parked under the taskbar) shipped precisely because a
//! surface was reasoned about instead of rendered.
//!
//! Public APIs only — no test hooks in production code, and deliberately NOT a
//! synthetic-data path inside the app: fake guardians on a real recovery screen
//! would be worse than no screen at all.
//!
//! ```text
//! cargo run -p sovereign-crypto --example seed_recovery_roster -- <data_dir> <password> [n_enrolled]
//! ```
//!
//! `data_dir` is a SOVEREIGN_DATA_DIR (it must already contain `crypto/auth.store` —
//! onboard first, e.g. `SHELL_AUTH_AUTO=1`). Enrolls `n_enrolled` (default 3)
//! slots with placeholder labels.

use std::path::PathBuf;

use sovereign_crypto::auth::AuthStore;
use sovereign_crypto::recovery_store::RecoveryStore;

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let dir: PathBuf = args
        .next()
        .ok_or("usage: seed_recovery_roster <data_dir> <password> [n_enrolled]")?
        .into();
    let password = args
        .next()
        .ok_or("usage: seed_recovery_roster <data_dir> <password> [n_enrolled]")?;
    let n: usize = args.next().unwrap_or_else(|| "3".into()).parse().unwrap_or(3);

    let crypto_dir = dir.join("crypto");
    let store_path = crypto_dir.join("auth.store");
    if !store_path.exists() {
        return Err(format!(
            "no auth.store at {} — onboard that workspace first",
            store_path.display()
        ));
    }

    let auth = AuthStore::load(&store_path).map_err(|e| format!("load auth.store: {e}"))?;
    let session = auth
        .authenticate(password.as_bytes())
        .map_err(|_| "wrong password for that workspace".to_string())?;

    let rs = RecoveryStore::new(&crypto_dir);
    let mut setup = rs.load_or_create(&session.kek, &session.account_key)?;

    let labels = ["Mum", "Alex", "Priya", "Sam", "Jo"];
    let mut enrolled = 0usize;
    for i in 0..setup.slots.len().min(n) {
        if setup.slots[i].is_enrolled() {
            continue;
        }
        let shard = setup.slots[i].shard_id.clone();
        setup.mark_enrolled(
            &shard,
            &format!("12D3KooWSeed{i}"),
            labels[i % labels.len()],
            "2026-07-14T10:11:12Z",
        )
        .map_err(|e| format!("mark_enrolled: {e}"))?;
        enrolled += 1;
    }
    rs.save(&setup, &session.kek)?;
    rs.write_recovery_card(&setup, "seed-tag", &["/dns4/seed.example/tcp/4001".into()])?;

    println!(
        "seeded: {} newly enrolled, roster now {}/{} ({})",
        enrolled,
        setup.enrolled_count(),
        setup.slots.len(),
        if setup.is_armed() { "armed" } else { "not armed" }
    );
    Ok(())
}
