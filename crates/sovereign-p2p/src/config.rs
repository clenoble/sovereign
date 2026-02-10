use serde::{Deserialize, Serialize};

/// P2P network configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pConfig {
    /// Whether P2P networking is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Port to listen on (0 = random).
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    /// Optional rendezvous server address for WAN discovery.
    #[serde(default)]
    pub rendezvous_server: Option<String>,
    /// Human-readable device name shown to peers.
    #[serde(default = "default_device_name")]
    pub device_name: String,
}

fn default_listen_port() -> u16 {
    0
}

fn default_device_name() -> String {
    "Sovereign Device".into()
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_port: default_listen_port(),
            rendezvous_server: None,
            device_name: default_device_name(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let cfg = P2pConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.listen_port, 0);
        assert!(cfg.rendezvous_server.is_none());
    }

    #[test]
    fn serde_roundtrip() {
        let cfg = P2pConfig {
            enabled: true,
            listen_port: 4001,
            rendezvous_server: Some("/ip4/1.2.3.4/tcp/8000".into()),
            device_name: "My Laptop".into(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: P2pConfig = serde_json::from_str(&json).unwrap();
        assert!(back.enabled);
        assert_eq!(back.listen_port, 4001);
    }
}
