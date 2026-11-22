//! C-SWEEP SW6 (spec ch. 09): a discreet signal, on both hosts (Céline: « Un
//! signal discret »). A count in the status bar when suspected values wait,
//! and a notice at login if the queue has grown since the last login. No
//! window opens by itself. The count seen at the last login lives in the
//! persona's area (SW4): its own store.
//!
//! The words (the Fable maille, by delegation of Céline, 2026-11-17): the
//! status bar's part is `{n} personal data to review`; the notice's title is
//! `Personal data to review`, its body `{k} more since your last login, {n} in
//! all. They wait in the PII dashboard.` The count at the last login is renewed
//! at each login, whether the notice showed or not. In the shell the notice
//! yields to every other notice due at that login.
//!
//! *Signatures by the test author of lot LS(a); the bodies are the implementer's.*

use sovereign_db::schema::ReviewState;
use sovereign_db::traits::GraphDB;

/// The store record (C-DOC step 6: a small value kept in the persona's own
/// store, sealed, never synced) holding the count seen at the last login.
pub const LAST_LOGIN_COUNT_KEY: &str = "pii_review_last_login_count";

/// The suspected values waiting in this persona's store (records `Unreviewed`,
/// not deleted, not the vault's).
pub async fn waiting(db: &dyn GraphDB) -> anyhow::Result<usize> {
    let records = db
        .list_pii_records(None, Some(ReviewState::Unreviewed), Some(false))
        .await
        .map_err(|e| anyhow::anyhow!("list_pii_records: {e}"))?;
    Ok(records
        .iter()
        .filter(|r| r.deleted_at.is_none() && !r.stored_secret && r.review_state == ReviewState::Unreviewed)
        .count())
}

/// The status bar's words for `waiting` suspected values; `None` when none
/// waits.
pub fn status_words(waiting: usize) -> Option<String> {
    (waiting > 0).then(|| format!("{waiting} personal data to review"))
}

/// What a login finds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginSignal {
    pub waiting: usize,
    /// How many more wait than at the last login (0 if not more).
    pub more_since_last_login: usize,
}

/// At a login: reads the count seen at the last login in this persona's store,
/// renews it with today's, and says what the login finds.
///
/// A count never recorded (the first login since this was built) reads as 0.
pub async fn login_signal(db: &dyn GraphDB) -> anyhow::Result<LoginSignal> {
    let now = waiting(db).await?;
    let before = db
        .get_store_record(LAST_LOGIN_COUNT_KEY)
        .await
        .map_err(|e| anyhow::anyhow!("get_store_record: {e}"))?
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(0);
    db.put_store_record(LAST_LOGIN_COUNT_KEY, &now.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("put_store_record: {e}"))?;
    Ok(LoginSignal { waiting: now, more_since_last_login: now.saturating_sub(before) })
}

/// The notice at login, (title, body); `None` when the queue has not grown.
pub fn login_notice(signal: &LoginSignal) -> Option<(String, String)> {
    if signal.more_since_last_login == 0 {
        return None;
    }
    Some((
        "Personal data to review".to_string(),
        format!(
            "{} more since your last login, {} in all. They wait in the PII dashboard.",
            signal.more_since_last_login, signal.waiting
        ),
    ))
}
