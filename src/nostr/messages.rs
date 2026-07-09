#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Messages sent from voter to EC via NIP-59 Gift Wrap.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum VoterMessage {
    Register {
        election_id: String,
        registration_token: String,
    },
    RequestToken {
        election_id: String,
        blinded_nonce: String,
    },
    CastVote {
        election_id: String,
        candidate_ids: Vec<u32>,
        h_n: String,
        token: String,
    },
}

/// Response from EC to voter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum EcResponse {
    Ok {
        action: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        blind_signature: Option<String>,
    },
    Error {
        code: EcErrorCode,
        message: String,
    },
}

/// EC error codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EcErrorCode {
    ElectionNotFound,
    ElectionClosed,
    InvalidToken,
    AlreadyRegistered,
    NotAuthorized,
    AlreadyIssued,
    NonceAlreadyUsed,
    InvalidCandidate,
    BallotInvalid,
    UnknownRules,
    InvalidMessage,
    InternalError,
    /// Any error code this client version does not know. Keeps the response
    /// parseable instead of dropping it entirely.
    #[serde(other)]
    Unknown,
}

impl std::fmt::Display for EcErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ElectionNotFound => write!(f, "Election not found"),
            Self::ElectionClosed => write!(f, "Election is closed"),
            Self::InvalidToken => write!(f, "Invalid or used registration token"),
            Self::AlreadyRegistered => write!(f, "Already registered for this election"),
            Self::NotAuthorized => write!(f, "Not authorized (not registered)"),
            Self::AlreadyIssued => write!(f, "Token already issued"),
            Self::NonceAlreadyUsed => write!(f, "Nonce already used (double vote attempt)"),
            Self::InvalidCandidate => write!(f, "Invalid candidate ID"),
            Self::BallotInvalid => write!(f, "Ballot does not match election rules"),
            Self::UnknownRules => write!(f, "Unknown voting rules"),
            Self::InvalidMessage => write!(f, "Malformed message"),
            Self::InternalError => write!(f, "EC internal error"),
            Self::Unknown => write!(f, "Unknown error code"),
        }
    }
}
