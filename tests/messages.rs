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
