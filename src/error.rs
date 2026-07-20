use thiserror::Error;

#[derive(Debug, Error)]
pub enum VoterError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Identity error: {0}")]
    Identity(String),

    #[error("Cryptographic error: {0}")]
    #[allow(dead_code)]
    Crypto(String),

    #[error("Nostr error: {0}")]
    #[allow(dead_code)]
    Nostr(String),

    #[error("State persistence error: {0}")]
    #[allow(dead_code)]
    State(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("Base64 decode error: {0}")]
    #[allow(dead_code)]
    Base64(#[from] base64::DecodeError),
}

pub type Result<T> = std::result::Result<T, VoterError>;
