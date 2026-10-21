//! Helpers for the wire guarantees (G24–G31).

use chrono::Duration;
use sovereign_docs::{RemoteDocument, Stamp, Trust, WireMeta, WireState};

use super::{text_block, v2, Stack};

/// A stamp `secs` seconds after the stack's clock now.
pub fn stamp(s: &Stack, secs: i64, counter: u32, device: &str) -> Stamp {
    Stamp {
        wall_ms: (s.clock.get() + Duration::seconds(secs)).timestamp_millis(),
        counter,
        device: device.into(),
    }
}

/// A remote state of `id` holding one text block.
pub fn remote_text(id: &str, title: &str, text: &str, device: &str) -> RemoteDocument {
    RemoteDocument {
        id: id.into(),
        title: title.into(),
        content: v2(vec![text_block(1, text)]),
        thread_id: "thread:t1".into(),
        trust: Some(Trust::Owned),
        provenance: None,
        modified_at: chrono::Utc::now(),
        deleted_at: None,
        device: device.into(),
        trust_decision: None,
    }
}

/// Wire metadata with a stamp and a previous stamp.
pub fn meta(stamp: Option<Stamp>, prev: Option<Stamp>) -> WireMeta {
    WireMeta { stamp, prev_stamp: prev, ..WireMeta::default() }
}

/// The stamp of a document's current state here.
pub async fn current_stamp(s: &Stack, id: &str) -> Stamp {
    s.svc.stored_state(id).await.expect("stored state").meta.stamp.expect("every state has a stamp")
}

/// Sends `id` from `from` to `to` (export, then apply).
pub async fn send(from: &Stack, to: &Stack, id: &str) -> sovereign_docs::WireOutcome {
    let WireState { document, meta, .. } = from.svc.stored_state(id).await.expect("export");
    to.svc.apply_wire(document, meta).await.expect("apply")
}
