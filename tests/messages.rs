use voter::nostr::messages::{EcErrorCode, EcResponse, VoterMessage};

#[test]
fn serialize_register_message() {
    let msg = VoterMessage::Register {
        election_id: "elec-001".to_string(),
        registration_token: "tok-abc".to_string(),
        request_id: "req-1".to_string(),
    };
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(json["action"], "register");
    assert_eq!(json["election_id"], "elec-001");
    assert_eq!(json["registration_token"], "tok-abc");
    assert_eq!(json["request_id"], "req-1");
}

#[test]
fn serialize_request_token_message() {
    let msg = VoterMessage::RequestToken {
        election_id: "elec-001".to_string(),
        blinded_nonce: "base64data==".to_string(),
        request_id: "req-2".to_string(),
    };
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(json["action"], "request-token");
    assert_eq!(json["election_id"], "elec-001");
    assert_eq!(json["blinded_nonce"], "base64data==");
    assert_eq!(json["request_id"], "req-2");
}

#[test]
fn serialize_cast_vote_message() {
    let msg = VoterMessage::CastVote {
        election_id: "elec-001".to_string(),
        candidate_ids: vec![1, 3],
        h_n: "abc123".to_string(),
        token: "tokendata==".to_string(),
        request_id: "req-3".to_string(),
    };
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(json["action"], "cast-vote");
    assert_eq!(json["election_id"], "elec-001");
    assert_eq!(json["candidate_ids"], serde_json::json!([1, 3]));
    assert_eq!(json["h_n"], "abc123");
    assert_eq!(json["token"], "tokendata==");
    assert_eq!(json["request_id"], "req-3");
}

#[test]
fn deserialize_voter_message_roundtrip() {
    let msg = VoterMessage::Register {
        election_id: "elec-002".to_string(),
        registration_token: "tok-xyz".to_string(),
        request_id: "req-4".to_string(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let parsed: VoterMessage = serde_json::from_str(&json).unwrap();
    match parsed {
        VoterMessage::Register {
            election_id,
            registration_token,
            request_id,
        } => {
            assert_eq!(election_id, "elec-002");
            assert_eq!(registration_token, "tok-xyz");
            assert_eq!(request_id, "req-4");
        }
        _ => panic!("expected Register variant"),
    }
}

#[test]
fn deserialize_ec_ok_response() {
    let json = r#"{"status":"ok","action":"register"}"#;
    let resp: EcResponse = serde_json::from_str(json).unwrap();
    match resp {
        EcResponse::Ok {
            action,
            blind_signature,
            request_id,
        } => {
            assert_eq!(action, "register");
            assert!(blind_signature.is_none());
            assert!(request_id.is_none());
        }
        _ => panic!("expected Ok response"),
    }
}

#[test]
fn deserialize_ec_ok_with_blind_signature() {
    let json = r#"{"status":"ok","action":"token-issued","blind_signature":"c2lnbmF0dXJl"}"#;
    let resp: EcResponse = serde_json::from_str(json).unwrap();
    match resp {
        EcResponse::Ok {
            action,
            blind_signature,
            ..
        } => {
            assert_eq!(action, "token-issued");
            assert_eq!(blind_signature.unwrap(), "c2lnbmF0dXJl");
        }
        _ => panic!("expected Ok response"),
    }
}

#[test]
fn deserialize_ec_error_response() {
    let json = r#"{"status":"error","code":"ALREADY_REGISTERED","message":"Already registered"}"#;
    let resp: EcResponse = serde_json::from_str(json).unwrap();
    match resp {
        EcResponse::Error { code, message, .. } => {
            assert_eq!(code, EcErrorCode::AlreadyRegistered);
            assert_eq!(message, "Already registered");
        }
        _ => panic!("expected Error response"),
    }
}

#[test]
fn error_code_roundtrip() {
    let codes = vec![
        EcErrorCode::ElectionNotFound,
        EcErrorCode::ElectionClosed,
        EcErrorCode::InvalidToken,
        EcErrorCode::AlreadyRegistered,
        EcErrorCode::NotAuthorized,
        EcErrorCode::AlreadyIssued,
        EcErrorCode::NonceAlreadyUsed,
        EcErrorCode::InvalidCandidate,
        EcErrorCode::BallotInvalid,
        EcErrorCode::UnknownRules,
        EcErrorCode::InvalidMessage,
        EcErrorCode::InternalError,
    ];
    for code in codes {
        let json = serde_json::to_string(&code).unwrap();
        let parsed: EcErrorCode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, code);
    }
}

#[test]
fn ec_ok_omits_null_blind_signature() {
    let resp = EcResponse::Ok {
        action: "register".to_string(),
        blind_signature: None,
        request_id: None,
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(!json.contains("blind_signature"));
    assert!(!json.contains("request_id"));
}

#[test]
fn every_ec_error_code_has_a_human_readable_display_message() {
    // Arrange: every variant paired with its expected display text.
    let cases = [
        (EcErrorCode::ElectionNotFound, "Election not found"),
        (EcErrorCode::ElectionClosed, "Election is closed"),
        (
            EcErrorCode::InvalidToken,
            "Invalid or used registration token",
        ),
        (
            EcErrorCode::AlreadyRegistered,
            "Already registered for this election",
        ),
        (
            EcErrorCode::NotAuthorized,
            "Not authorized (not registered)",
        ),
        (EcErrorCode::AlreadyIssued, "Token already issued"),
        (
            EcErrorCode::NonceAlreadyUsed,
            "Nonce already used (double vote attempt)",
        ),
        (EcErrorCode::InvalidCandidate, "Invalid candidate ID"),
        (
            EcErrorCode::BallotInvalid,
            "Ballot does not match election rules",
        ),
        (EcErrorCode::UnknownRules, "Unknown voting rules"),
        (EcErrorCode::InvalidMessage, "Malformed message"),
        (EcErrorCode::InternalError, "EC internal error"),
        (EcErrorCode::Unknown, "Unknown error code"),
    ];

    for (code, expected) in cases {
        // Act
        let rendered = code.to_string();

        // Assert
        assert_eq!(rendered, expected);
    }
}

#[test]
fn unknown_error_code_string_deserializes_to_unknown_variant() {
    let parsed: EcErrorCode = serde_json::from_str("\"BRAND_NEW_FUTURE_CODE\"").unwrap();

    assert_eq!(parsed, EcErrorCode::Unknown);
}

#[test]
fn request_id_accessor_returns_id_for_all_message_variants() {
    // Arrange
    let register = VoterMessage::Register {
        election_id: "e1".to_string(),
        registration_token: "t".to_string(),
        request_id: "req-register".to_string(),
    };
    let request_token = VoterMessage::RequestToken {
        election_id: "e1".to_string(),
        blinded_nonce: "bn".to_string(),
        request_id: "req-token".to_string(),
    };
    let cast_vote = VoterMessage::CastVote {
        election_id: "e1".to_string(),
        candidate_ids: vec![1],
        h_n: "hn".to_string(),
        token: "tok".to_string(),
        request_id: "req-vote".to_string(),
    };

    // Act & Assert
    assert_eq!(register.request_id(), "req-register");
    assert_eq!(request_token.request_id(), "req-token");
    assert_eq!(cast_vote.request_id(), "req-vote");
}

#[test]
fn request_token_wire_format_matches_ec_protocol() {
    // Arrange
    let msg = VoterMessage::RequestToken {
        election_id: "e1".to_string(),
        blinded_nonce: "YmxpbmRlZA==".to_string(),
        request_id: "req-2".to_string(),
    };

    // Act
    let json: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();

    // Assert
    assert_eq!(json["action"], "request-token");
    assert_eq!(json["election_id"], "e1");
    assert_eq!(json["blinded_nonce"], "YmxpbmRlZA==");
    assert_eq!(json["request_id"], "req-2");
}

#[test]
fn cast_vote_wire_format_matches_ec_protocol() {
    // Arrange
    let msg = VoterMessage::CastVote {
        election_id: "e1".to_string(),
        candidate_ids: vec![3, 1],
        h_n: "aa".repeat(32),
        token: "dG9rZW4=".to_string(),
        request_id: "req-3".to_string(),
    };

    // Act
    let json: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();

    // Assert
    assert_eq!(json["action"], "cast-vote");
    assert_eq!(json["candidate_ids"], serde_json::json!([3, 1]));
    assert_eq!(json["h_n"], "aa".repeat(32));
    assert_eq!(json["token"], "dG9rZW4=");
    assert_eq!(json["request_id"], "req-3");
}
