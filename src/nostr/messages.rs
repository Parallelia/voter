#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Messages sent from voter to EC via NIP-59 Gift Wrap.
///
/// Every request carries a fresh random `request_id`; the EC echoes it in its
/// reply so responses can be correlated with the in-flight request and
/// replayed Gift Wraps can be told apart from the real answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum VoterMessage {
    Register {
        election_id: String,
        registration_token: String,
        request_id: String,
    },
    RequestToken {
        election_id: String,
        blinded_nonce: String,
        request_id: String,
    },
    CastVote {
        election_id: String,
        candidate_ids: Vec<u32>,
        h_n: String,
        token: String,
        request_id: String,
    },
}

impl VoterMessage {
    /// The correlation id this request was sent with.
    pub fn request_id(&self) -> &str {
        match self {
            Self::Register { request_id, .. }
            | Self::RequestToken { request_id, .. }
            | Self::CastVote { request_id, .. } => request_id,
        }
    }
}

/// Response from EC to voter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum EcResponse {
    Ok {
        action: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        blind_signature: Option<String>,
        /// Echo of the request's `request_id`. Responses that do not echo
        /// the in-flight request's id (including `None`) are never matched
        /// to it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
    Error {
        code: EcErrorCode,
        message: String,
        /// Echo of the request's `request_id`. Responses that do not echo
        /// the in-flight request's id (including `None`) are never matched
        /// to it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voter_message_serializes_request_id_on_the_wire() {
        // Arrange
        let msg = VoterMessage::Register {
            election_id: "e1".to_string(),
            registration_token: "t".to_string(),
            request_id: "req-1".to_string(),
        };

        // Act
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();

        // Assert
        assert_eq!(json["action"], "register");
        assert_eq!(json["request_id"], "req-1");
    }

    #[test]
    fn ec_response_parses_with_request_id_echo() {
        let raw = r#"{"status":"ok","action":"token-issued","blind_signature":"sig","request_id":"req-1"}"#;

        let response: EcResponse = serde_json::from_str(raw).unwrap();

        let EcResponse::Ok { request_id, .. } = response else {
            panic!("expected Ok");
        };
        assert_eq!(request_id.as_deref(), Some("req-1"));
    }

    #[test]
    fn ec_response_parses_without_request_id_from_legacy_ec() {
        let ok: EcResponse =
            serde_json::from_str(r#"{"status":"ok","action":"vote-recorded"}"#).unwrap();
        let err: EcResponse =
            serde_json::from_str(r#"{"status":"error","code":"INVALID_TOKEN","message":"m"}"#)
                .unwrap();

        let EcResponse::Ok { request_id, .. } = ok else {
            panic!("expected Ok");
        };
        assert_eq!(request_id, None);
        let EcResponse::Error { request_id, .. } = err else {
            panic!("expected Error");
        };
        assert_eq!(request_id, None);
    }
}
