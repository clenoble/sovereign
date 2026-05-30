//! Shared helpers for communication channels.
//! Deduplicates conversation/contact management logic common to email, signal, whatsapp.

use std::collections::HashMap;
use std::sync::Arc;

use sovereign_db::schema::{
    ChannelAddress, ChannelType, Contact, Conversation,
};
use sovereign_db::GraphDB;

use crate::error::CommsError;
use crate::pii_hook::ContactIngestHook;

/// Get or create a conversation, using a local cache to avoid repeated DB loads.
pub async fn get_or_create_conversation(
    db: &dyn GraphDB,
    title: &str,
    channel: ChannelType,
    participant_ids: Vec<String>,
    cache: &mut HashMap<String, Conversation>,
) -> Result<Conversation, CommsError> {
    if let Some(conv) = cache.get(title) {
        return Ok(conv.clone());
    }

    let conv = Conversation::new(title.to_string(), channel, participant_ids);
    let created = db.create_conversation(conv).await.map_err(CommsError::from)?;
    cache.insert(title.to_string(), created.clone());
    Ok(created)
}

/// Resolve an address (email, phone, etc.) to a contact ID, creating a
/// stub contact if needed.
///
/// `pii_hook` (when supplied) is invoked exactly once on freshly-created
/// contacts — not on already-existing ones returned via the address index.
pub async fn resolve_contact_id(
    db: &dyn GraphDB,
    channel: ChannelType,
    address: &str,
    display_name: Option<&str>,
    pii_hook: Option<&Arc<dyn ContactIngestHook>>,
) -> Result<String, CommsError> {
    if let Some(contact) = db.find_contact_by_address(address).await? {
        return Ok(contact.id_string().unwrap_or_default());
    }

    let name = display_name
        .map(|s| s.to_string())
        .unwrap_or_else(|| address.to_string());
    let mut contact = Contact::new(name, false);
    contact.addresses.push(ChannelAddress {
        channel,
        address: address.to_string(),
        display_name: display_name.map(|s| s.to_string()),
        is_primary: true,
    });
    let created = db.create_contact(contact).await?;
    if let Some(hook) = pii_hook {
        hook.after_contact_created(&created).await;
    }
    Ok(created.id_string().unwrap_or_default())
}
