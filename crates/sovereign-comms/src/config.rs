use serde::Deserialize;

/// Top-level communications configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct CommsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    #[serde(default)]
    pub email: Option<EmailAccountConfig>,
    #[serde(default)]
    pub signal: Option<SignalAccountConfig>,
    #[serde(default)]
    pub whatsapp: Option<WhatsAppAccountConfig>,
}

fn default_poll_interval() -> u64 {
    300 // 5 minutes
}

impl Default for CommsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            poll_interval_secs: default_poll_interval(),
            email: None,
            signal: None,
            whatsapp: None,
        }
    }
}

/// Email account configuration.
/// Password is NOT stored here — use KeyDatabase or environment variable.
#[derive(Debug, Clone, Deserialize)]
pub struct EmailAccountConfig {
    pub imap_host: String,
    #[serde(default = "default_imap_port")]
    pub imap_port: u16,
    pub smtp_host: String,
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    pub username: String,
    /// Display name for outgoing emails.
    #[serde(default)]
    pub display_name: Option<String>,
}

fn default_imap_port() -> u16 {
    993
}

fn default_smtp_port() -> u16 {
    587
}

/// Signal linked-device configuration.
/// Connects as a secondary device (like Signal Desktop).
/// Password/credentials are handled by the Signal protocol key store.
#[derive(Debug, Clone, Deserialize)]
pub struct SignalAccountConfig {
    /// Phone number registered with Signal (e.g., "+15551234567").
    pub phone_number: String,
    /// Path to the Signal protocol store directory.
    #[serde(default = "default_signal_store_path")]
    pub store_path: String,
    /// Display name shown to contacts.
    #[serde(default)]
    pub device_name: Option<String>,
}

fn default_signal_store_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    format!("{home}/.sovereign/signal")
}

/// WhatsApp Cloud API configuration.
/// Uses Meta's official Business API (requires a Business account + access token).
#[derive(Debug, Clone, Deserialize)]
pub struct WhatsAppAccountConfig {
    /// Phone number ID from the WhatsApp Business dashboard.
    pub phone_number_id: String,
    /// WhatsApp Business Account ID.
    pub business_account_id: String,
    /// Base URL for the Graph API.
    #[serde(default = "default_whatsapp_api_url")]
    pub api_url: String,
    /// API version to use.
    #[serde(default = "default_whatsapp_api_version")]
    pub api_version: String,
    /// Display name for the business profile.
    #[serde(default)]
    pub display_name: Option<String>,
}

fn default_whatsapp_api_url() -> String {
    "https://graph.facebook.com".into()
}

fn default_whatsapp_api_version() -> String {
    "v21.0".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comms_config_defaults() {
        let cfg = CommsConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.poll_interval_secs, 300);
        assert!(cfg.email.is_none());
        assert!(cfg.signal.is_none());
        assert!(cfg.whatsapp.is_none());
    }

    #[test]
    fn deserialize_email_config() {
        let toml = r#"
            imap_host = "imap.example.com"
            smtp_host = "smtp.example.com"
            username = "user@example.com"
        "#;
        let cfg: EmailAccountConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.imap_host, "imap.example.com");
        assert_eq!(cfg.imap_port, 993);
        assert_eq!(cfg.smtp_port, 587);
        assert_eq!(cfg.username, "user@example.com");
        assert!(cfg.display_name.is_none());
    }

    #[test]
    fn deserialize_signal_config() {
        let toml = r#"
            phone_number = "+15551234567"
        "#;
        let cfg: SignalAccountConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.phone_number, "+15551234567");
        assert!(cfg.store_path.contains("signal"));
        assert!(cfg.device_name.is_none());
    }

    #[test]
    fn deserialize_whatsapp_config() {
        let toml = r#"
            phone_number_id = "123456789"
            business_account_id = "987654321"
        "#;
        let cfg: WhatsAppAccountConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.phone_number_id, "123456789");
        assert_eq!(cfg.business_account_id, "987654321");
        assert!(cfg.api_url.contains("graph.facebook.com"));
        assert_eq!(cfg.api_version, "v21.0");
    }
}
