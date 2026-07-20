use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{Result, VoterError};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_nostr")]
    pub nostr: NostrConfig,
    #[serde(default)]
    pub identity: IdentityConfig,
    #[serde(default)]
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NostrConfig {
    #[serde(default = "default_relays")]
    pub relays: Vec<String>,
    /// Trusted Electoral Commission Nostr public key (hex or npub).
    /// When set, only election/result events signed by this key are accepted,
    /// and Gift Wrap responses from anyone else are discarded. Without it, any
    /// relay user can publish fake elections and harvest registration tokens.
    #[serde(default)]
    pub ec_pubkey: Option<String>,
    /// Permit unencrypted `ws://` relays. Off by default: NIP-59 already
    /// encrypts payloads end to end, but a plaintext relay leaks transport
    /// metadata (which relay, when, message timing/sizes) to the network.
    /// Only intended for local development against a test relay.
    #[serde(default)]
    pub allow_insecure_relays: bool,
}

impl NostrConfig {
    /// Validate every relay URL scheme.
    ///
    /// `wss://` is always accepted; `ws://` only behind
    /// [`allow_insecure_relays`](Self::allow_insecure_relays); anything else
    /// is not a websocket relay URL and is rejected outright.
    ///
    /// The scheme is matched case-insensitively and ignoring surrounding
    /// whitespace, because URL schemes are case-insensitive (RFC 3986 §3.1)
    /// and nostr-sdk accepts and normalizes both forms. Matching strictly
    /// would reject a working `WSS://` relay and — worse — would let `WS://`
    /// fall through to the catch-all instead of the insecure-relay check that
    /// exists to detect exactly that. Errors quote the entry verbatim so it
    /// can be located in the config file.
    pub fn validate(&self) -> Result<()> {
        for relay in &self.relays {
            let normalized = relay.trim().to_ascii_lowercase();
            if normalized.starts_with("wss://") {
                continue;
            }
            if normalized.starts_with("ws://") {
                if self.allow_insecure_relays {
                    continue;
                }
                return Err(VoterError::Config(format!(
                    "relay \"{relay}\" is unencrypted; use wss:// (or set \
                     allow_insecure_relays = true under [nostr] for local development)"
                )));
            }
            return Err(VoterError::Config(format!(
                "relay \"{relay}\" is not a websocket URL; relay URLs must start with wss://"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityConfig {
    #[serde(default = "default_identity_path")]
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: Theme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    Light,
}

fn default_relays() -> Vec<String> {
    vec![
        "wss://relay.mostro.network".to_string(),
        "wss://nos.lol".to_string(),
    ]
}

fn default_nostr() -> NostrConfig {
    NostrConfig {
        relays: default_relays(),
        ec_pubkey: None,
        allow_insecure_relays: false,
    }
}

fn default_identity_path() -> PathBuf {
    config_dir().join("identity.json")
}

fn default_theme() -> Theme {
    Theme::Dark
}

impl Default for NostrConfig {
    fn default() -> Self {
        default_nostr()
    }
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            path: default_identity_path(),
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
        }
    }
}

// Default is auto-derived from field defaults

/// Returns the voter config directory (~/.config/voter/).
pub fn config_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "voter")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Returns the default config file path (~/.config/voter/voter.toml).
pub fn config_path() -> PathBuf {
    config_dir().join("voter.toml")
}

impl AppConfig {
    /// Load config from the given path, or create a default if missing.
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let contents = std::fs::read_to_string(path)?;
            let config: AppConfig = toml::from_str(&contents)?;
            config.nostr.validate()?;
            Ok(config)
        } else {
            let config = AppConfig::default();
            config.save(path)?;
            Ok(config)
        }
    }

    /// Save config to the given path, creating parent directories.
    ///
    /// Runs the same relay validation as [`load`](Self::load) so an invalid
    /// configuration can neither enter nor leave the process.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.nostr.validate()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self).map_err(VoterError::TomlSer)?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    /// Returns the resolved state file path (~/.config/voter/state.json).
    pub fn state_path(&self) -> PathBuf {
        config_dir().join("state.json")
    }
}
