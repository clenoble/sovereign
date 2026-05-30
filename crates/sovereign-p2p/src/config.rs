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
    /// When true, suppress auto-sync triggers while the device reports
    /// `ConnectivityState::Cellular`. Defaults to true on Android (mobile
    /// data is metered; mDNS doesn't work without multicast anyway), and
    /// false on desktop (wired/Wi-Fi assumed). Plays alongside the
    /// connectivity callback wired by the Android plugin (Phase 4.1).
    #[serde(default = "default_wifi_only")]
    pub wifi_only: bool,
}

fn default_listen_port() -> u16 {
    0
}

fn default_device_name() -> String {
    "Sovereign Device".into()
}

fn default_wifi_only() -> bool {
    cfg!(target_os = "android")
}

/// Network reachability classes the Android plugin reports back to the
/// Rust core via the connectivity callback. Used to gate auto-sync when
/// `P2pConfig.wifi_only` is true. Desktop builds default to `Wifi`
/// since they have no equivalent callback and assume wired/Wi-Fi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConnectivityState {
    /// Wi-Fi or wired Ethernet — unmetered.
    Wifi,
    /// Cellular data — typically metered. Suppress auto-sync when
    /// `wifi_only`.
    Cellular,
    /// No network at all. Suppress all sync activity.
    Offline,
    /// We haven't received a callback yet, or the platform doesn't
    /// distinguish — treat optimistically as Wifi for desktop.
    #[default]
    Unknown,
}

impl ConnectivityState {
    /// Whether auto-sync should fire on this connectivity, given the
    /// `wifi_only` policy. Conservative: `Unknown` is treated as Wi-Fi
    /// on desktop (where no callback exists) and as a hold on Android
    /// (the plugin will call back within seconds of first launch).
    pub fn allows_auto_sync(self, wifi_only: bool) -> bool {
        if !wifi_only {
            // No restriction: only Offline blocks.
            return self != Self::Offline;
        }
        match self {
            Self::Wifi => true,
            Self::Cellular | Self::Offline => false,
            // Mobile: hold until first connectivity callback. Desktop:
            // wifi_only is false by default, so this branch is rare.
            Self::Unknown => !cfg!(target_os = "android"),
        }
    }
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_port: default_listen_port(),
            rendezvous_server: None,
            device_name: default_device_name(),
            wifi_only: default_wifi_only(),
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
            wifi_only: true,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: P2pConfig = serde_json::from_str(&json).unwrap();
        assert!(back.enabled);
        assert_eq!(back.listen_port, 4001);
        assert!(back.wifi_only);
    }

    #[test]
    fn wifi_only_default_matches_target() {
        // Android: defaults to true (mobile data is metered, mDNS won't
        // work without multicast). Desktop: defaults to false.
        let cfg = P2pConfig::default();
        assert_eq!(cfg.wifi_only, cfg!(target_os = "android"));
    }

    #[test]
    fn legacy_v04_config_without_wifi_only_deserializes() {
        // A v0.0.4 config TOML/JSON has no `wifi_only` field. The
        // serde(default) attribute must populate it from
        // `default_wifi_only()` rather than failing.
        let legacy_json = r#"{
            "enabled": true,
            "listen_port": 0,
            "rendezvous_server": null,
            "device_name": "Old"
        }"#;
        let cfg: P2pConfig = serde_json::from_str(legacy_json).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.wifi_only, cfg!(target_os = "android"));
    }

    #[test]
    fn allows_auto_sync_when_wifi_only_off() {
        // Without the wifi_only restriction, only Offline blocks.
        assert!(ConnectivityState::Wifi.allows_auto_sync(false));
        assert!(ConnectivityState::Cellular.allows_auto_sync(false));
        assert!(ConnectivityState::Unknown.allows_auto_sync(false));
        assert!(!ConnectivityState::Offline.allows_auto_sync(false));
    }

    #[test]
    fn allows_auto_sync_when_wifi_only_on() {
        // With wifi_only, only Wi-Fi (and Unknown on desktop) sync.
        assert!(ConnectivityState::Wifi.allows_auto_sync(true));
        assert!(!ConnectivityState::Cellular.allows_auto_sync(true));
        assert!(!ConnectivityState::Offline.allows_auto_sync(true));
        // Unknown: optimistic on desktop, conservative on Android.
        assert_eq!(
            ConnectivityState::Unknown.allows_auto_sync(true),
            !cfg!(target_os = "android")
        );
    }
}
