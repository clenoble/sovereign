use std::time::Duration;

use sovereign_db::schema::ChannelType;
use tokio::sync::mpsc;

use crate::channel::{CommunicationChannel, SyncResult};
use crate::error::CommsError;

/// Events emitted by the communications sync engine.
#[derive(Debug, Clone)]
pub enum CommsEvent {
    NewMessages {
        channel: ChannelType,
        count: u32,
        conversation_id: String,
    },
    SyncComplete {
        channel: ChannelType,
        result: SyncResult,
    },
    SyncError {
        channel: ChannelType,
        error: String,
    },
    ContactDiscovered {
        contact_id: String,
        name: String,
    },
}

/// Periodic sync engine that polls registered communication channels.
pub struct CommsSync {
    channels: Vec<Box<dyn CommunicationChannel>>,
    event_tx: mpsc::Sender<CommsEvent>,
    poll_interval: Duration,
}

impl CommsSync {
    pub fn new(
        event_tx: mpsc::Sender<CommsEvent>,
        poll_interval_secs: u64,
    ) -> Self {
        Self {
            channels: Vec::new(),
            event_tx,
            poll_interval: Duration::from_secs(poll_interval_secs),
        }
    }

    /// Register a communication channel to be polled.
    pub fn add_channel(&mut self, channel: Box<dyn CommunicationChannel>) {
        self.channels.push(channel);
    }

    /// Connect all registered channels.
    pub async fn connect_all(&mut self) -> Vec<Result<(), CommsError>> {
        let mut results = Vec::new();
        for ch in &mut self.channels {
            results.push(ch.connect().await);
        }
        results
    }

    /// Run the sync loop. This blocks and should be spawned as a tokio task.
    pub async fn run(mut self) {
        async fn emit_sync_error(tx: &mpsc::Sender<CommsEvent>, channel: ChannelType, error: &(dyn std::fmt::Display + Send + Sync)) {
            let _ = tx.send(CommsEvent::SyncError {
                channel,
                error: error.to_string(),
            }).await;
        }

        // Initial connect
        for ch in &mut self.channels {
            if let Err(e) = ch.connect().await {
                tracing::warn!("Channel {:?} connect failed: {e}", ch.channel_type());
                emit_sync_error(&self.event_tx, ch.channel_type(), &e).await;
            }
        }

        let mut interval = tokio::time::interval(self.poll_interval);
        loop {
            interval.tick().await;

            for ch in &mut self.channels {
                match ch.sync().await {
                    Ok(result) => {
                        if result.new_messages > 0 || result.new_contacts > 0 {
                            tracing::info!(
                                "Sync {:?}: {} new msgs, {} new contacts",
                                ch.channel_type(),
                                result.new_messages,
                                result.new_contacts,
                            );
                        }
                        let _ = self.event_tx.send(CommsEvent::SyncComplete {
                            channel: ch.channel_type(),
                            result,
                        }).await;
                    }
                    Err(e) => {
                        tracing::error!("Sync {:?} failed: {e}", ch.channel_type());
                        emit_sync_error(&self.event_tx, ch.channel_type(), &e).await;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{ChannelStatus, OutgoingMessage, SyncResult};
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use sovereign_db::schema::{Contact, Message};
    use std::sync::{Arc, Mutex};

    /// Mock communication channel for testing sync engine behavior.
    struct MockChannel {
        ctype: ChannelType,
        should_fail: bool,
        connect_calls: Arc<Mutex<u32>>,
    }

    impl MockChannel {
        fn ok(ctype: ChannelType) -> Self {
            Self {
                ctype,
                should_fail: false,
                connect_calls: Arc::new(Mutex::new(0)),
            }
        }

        fn failing(ctype: ChannelType) -> Self {
            Self {
                ctype,
                should_fail: true,
                connect_calls: Arc::new(Mutex::new(0)),
            }
        }
    }

    #[async_trait]
    impl CommunicationChannel for MockChannel {
        async fn connect(&mut self) -> Result<(), CommsError> {
            *self.connect_calls.lock().unwrap() += 1;
            if self.should_fail {
                Err(CommsError::ConfigError("mock failure".into()))
            } else {
                Ok(())
            }
        }
        async fn disconnect(&mut self) -> Result<(), CommsError> {
            Ok(())
        }
        fn status(&self) -> ChannelStatus {
            ChannelStatus::Connected
        }
        fn channel_type(&self) -> ChannelType {
            self.ctype.clone()
        }
        async fn fetch_messages(&self, _since: Option<DateTime<Utc>>) -> Result<Vec<Message>, CommsError> {
            Ok(vec![])
        }
        async fn send_message(&self, _msg: &OutgoingMessage) -> Result<String, CommsError> {
            Ok("mock-id".into())
        }
        async fn sync(&mut self) -> Result<SyncResult, CommsError> {
            Ok(SyncResult { new_messages: 0, updated_conversations: 0, new_contacts: 0 })
        }
        async fn resolve_contact(&self, address: &str) -> Result<Contact, CommsError> {
            Ok(Contact::new(address.to_string(), false))
        }
    }

    #[test]
    fn comms_event_clone() {
        let event = CommsEvent::NewMessages {
            channel: ChannelType::Email,
            count: 3,
            conversation_id: "conversation:1".into(),
        };
        let cloned = event.clone();
        match cloned {
            CommsEvent::NewMessages { count, .. } => assert_eq!(count, 3),
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn comms_sync_new() {
        let (tx, _rx) = mpsc::channel(16);
        let sync = CommsSync::new(tx, 60);
        assert!(sync.channels.is_empty());
        assert_eq!(sync.poll_interval, Duration::from_secs(60));
    }

    #[tokio::test]
    async fn connect_all_succeeds_with_mock() {
        let (tx, _rx) = mpsc::channel(16);
        let mut sync = CommsSync::new(tx, 60);

        let ch1 = MockChannel::ok(ChannelType::Email);
        let ch1_calls = ch1.connect_calls.clone();

        sync.add_channel(Box::new(ch1));
        let results = sync.connect_all().await;

        assert_eq!(results.len(), 1);
        assert!(results[0].is_ok());
        assert_eq!(*ch1_calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn connect_all_captures_error() {
        let (tx, _rx) = mpsc::channel(16);
        let mut sync = CommsSync::new(tx, 60);

        let ch_ok = MockChannel::ok(ChannelType::Email);
        let ch_fail = MockChannel::failing(ChannelType::Email);

        sync.add_channel(Box::new(ch_ok));
        sync.add_channel(Box::new(ch_fail));

        let results = sync.connect_all().await;
        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_err());
    }
}
