pub mod channel;
pub mod channels;
pub mod config;
pub mod error;
pub mod pii_hook;
pub mod sync_engine;

pub use channel::{ChannelStatus, CommunicationChannel, OutgoingMessage, SyncResult};
pub use config::{CommsConfig, EmailAccountConfig, SignalAccountConfig, WhatsAppAccountConfig};
pub use error::CommsError;
pub use pii_hook::MessageIngestHook;
pub use sync_engine::{CommsEvent, CommsSync};
