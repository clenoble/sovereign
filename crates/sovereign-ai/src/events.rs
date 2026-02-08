pub use sovereign_core::interfaces::OrchestratorEvent;

/// Events from the voice pipeline to the UI.
#[derive(Debug, Clone)]
pub enum VoiceEvent {
    WakeWordDetected,
    ListeningStarted,
    TranscriptionReady(String),
    ListeningStopped,
    TtsSpeaking(String),
    TtsDone,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_event_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<VoiceEvent>();
    }

    #[test]
    fn voice_event_clone() {
        let event = VoiceEvent::TranscriptionReady("hello".into());
        let cloned = event.clone();
        if let VoiceEvent::TranscriptionReady(text) = cloned {
            assert_eq!(text, "hello");
        } else {
            panic!("Clone should preserve variant");
        }
    }

    #[test]
    fn orchestrator_event_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<OrchestratorEvent>();
    }
}
