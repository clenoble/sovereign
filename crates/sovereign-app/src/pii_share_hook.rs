//! `ShareIngestHook` implementation that builds the sharing ledger.
//!
//! Step 7a of the PII management & dashboard plan. Fires after every
//! outbound message is persisted (and after the per-message PII hook
//! has tokenized its body), parses `[pii:<record_id>]` tokens, and
//! writes a `ShareRecord` per (token × recipient entity) pair.
//!
//! Receives the freshly-persisted Message but re-fetches by ID before
//! scanning, since the body that lands in the channel-side
//! `create_message` call is the raw form — the canonical (tokenized)
//! body is only in the DB after `MessageIngestHook` ran. Re-fetching
//! is one extra read per outbound message; the alternative would be
//! threading the canonical body back through the comms layer, which
//! would couple the layers more tightly.

use std::sync::Arc;

use async_trait::async_trait;
use sovereign_comms::pii_hook::ShareIngestHook;
use sovereign_db::schema::{
    thing_to_raw, ChannelType, Message, MessageDirection, ShareChannel, ShareRecord,
};
use sovereign_db::traits::GraphDB;

pub struct PiiShareHook {
    db: Arc<dyn GraphDB>,
}

impl PiiShareHook {
    pub fn new(db: Arc<dyn GraphDB>) -> Self {
        Self { db }
    }

    async fn run_ledger(&self, message_id: &str) -> anyhow::Result<usize> {
        // Pull the fresh message with the canonical body.
        let message = self.db.get_message(message_id).await?;
        if message.direction != MessageDirection::Outbound {
            // Defensive: hook is only fired by send_message paths but
            // re-checking keeps the contract clear.
            return Ok(0);
        }

        // Parse tokens once.
        let token_record_ids = parse_tokens(&message.body);
        if token_record_ids.is_empty() {
            return Ok(0);
        }

        // Resolve recipient entities by joining through contacts. A
        // recipient with no entity_id (unattributed) is skipped — the
        // ledger only tracks disclosures to identified entities.
        let mut recipient_entity_ids: Vec<String> = Vec::new();
        for contact_id in &message.to_contact_ids {
            match self.db.get_contact(contact_id).await {
                Ok(contact) => {
                    if let Some(eid) = contact.entity_id {
                        recipient_entity_ids.push(eid);
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "share hook: get_contact({contact_id}) failed, skipping: {e}"
                    );
                }
            }
        }
        if recipient_entity_ids.is_empty() {
            tracing::debug!(
                "share hook: outbound msg {message_id} has no entity-attributed \
                 recipients — skipping ledger write"
            );
            return Ok(0);
        }

        let share_channel = map_channel(&message.channel);
        let shared_at = message.sent_at;
        let mut written = 0usize;
        for record_id in &token_record_ids {
            for entity_id in &recipient_entity_ids {
                let share = ShareRecord {
                    id: None,
                    pii_record_id: record_id.clone(),
                    to_entity_id: entity_id.clone(),
                    via_message_id: Some(message_id.to_string()),
                    via_url: None,
                    shared_at,
                    channel: share_channel.clone(),
                };
                if let Err(e) = self.db.create_share_record(share).await {
                    tracing::warn!(
                        "share hook: create_share_record failed for {record_id} → \
                         {entity_id}: {e}"
                    );
                    continue;
                }
                written += 1;
            }
        }
        Ok(written)
    }
}

#[async_trait]
impl ShareIngestHook for PiiShareHook {
    async fn after_outbound_message(&self, message: &Message) {
        let id = match message.id.as_ref() {
            Some(t) => thing_to_raw(t),
            None => {
                tracing::warn!("share hook: outbound message has no id, skipping");
                return;
            }
        };
        match self.run_ledger(&id).await {
            Ok(n) if n > 0 => {
                tracing::info!("share hook: msg {id} → wrote {n} ShareRecord rows");
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("share hook: ledger write failed for {id}: {e}"),
        }
    }
}

/// Parse every `[pii:<record_id>]` token in `body` and return the
/// record IDs in encounter order. Mirrors the parser in
/// `sovereign-ai/pii/resolve.rs`; duplicated here to avoid pulling
/// resolve into the share-hook scope just for the parser.
fn parse_tokens(body: &str) -> Vec<String> {
    const PREFIX: &str = "[pii:";
    let mut out = Vec::new();
    let mut cursor = 0;
    while cursor < body.len() {
        let Some(rel) = body[cursor..].find(PREFIX) else {
            break;
        };
        let start = cursor + rel;
        let after = start + PREFIX.len();
        let Some(rel_end) = body[after..].find(']') else {
            break;
        };
        let end_idx = after + rel_end;
        out.push(body[after..end_idx].to_string());
        cursor = end_idx + 1;
    }
    out
}

fn map_channel(c: &ChannelType) -> ShareChannel {
    match c {
        ChannelType::Email => ShareChannel::Email,
        ChannelType::Sms => ShareChannel::Sms,
        ChannelType::Signal => ShareChannel::Signal,
        ChannelType::WhatsApp => ShareChannel::WhatsApp,
        ChannelType::Matrix => ShareChannel::Matrix,
        ChannelType::Phone => ShareChannel::Phone,
        ChannelType::Custom(_) => ShareChannel::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_no_tokens() {
        assert!(parse_tokens("hello world").is_empty());
    }

    #[test]
    fn parse_single_token() {
        assert_eq!(parse_tokens("hi [pii:pii_record:abc]"), vec!["pii_record:abc"]);
    }

    #[test]
    fn parse_multiple_tokens_in_order() {
        let body = "[pii:a] middle [pii:b]";
        assert_eq!(parse_tokens(body), vec!["a", "b"]);
    }

    #[test]
    fn parse_unclosed_token_skipped() {
        assert!(parse_tokens("[pii:abc no closing").is_empty());
    }

    #[test]
    fn map_channel_known_kinds() {
        assert!(matches!(map_channel(&ChannelType::Email), ShareChannel::Email));
        assert!(matches!(
            map_channel(&ChannelType::Custom("telegram".into())),
            ShareChannel::Other
        ));
    }
}
