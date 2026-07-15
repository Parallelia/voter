//! The anonymous cast-vote roundtrip must only accept an EC reply that
//! echoes the request's correlation id. Accepting any parseable response
//! aborted the watchdog while the app (correctly) rejected the reply as
//! uncorrelated — leaving the pending request and loading spinner stuck
//! forever, with every EC action gated behind them until restart.

use voter::nostr::client::response_echoes_request_id;
use voter::nostr::messages::{EcErrorCode, EcResponse};

fn ok_response(request_id: Option<&str>) -> EcResponse {
    EcResponse::Ok {
        action: "vote-recorded".to_string(),
        blind_signature: None,
        request_id: request_id.map(str::to_string),
    }
}

fn error_response(request_id: Option<&str>) -> EcResponse {
    EcResponse::Error {
        code: EcErrorCode::InternalError,
        message: "boom".to_string(),
        request_id: request_id.map(str::to_string),
    }
}

#[test]
fn ok_response_echoing_the_request_id_matches() {
    assert!(response_echoes_request_id(
        &ok_response(Some("req-1")),
        "req-1"
    ));
}

#[test]
fn ok_response_without_echo_does_not_match() {
    assert!(!response_echoes_request_id(&ok_response(None), "req-1"));
}

#[test]
fn ok_response_with_stale_echo_does_not_match() {
    assert!(!response_echoes_request_id(
        &ok_response(Some("req-OLD")),
        "req-1"
    ));
}

#[test]
fn error_response_echoing_the_request_id_matches() {
    assert!(response_echoes_request_id(
        &error_response(Some("req-1")),
        "req-1"
    ));
}

#[test]
fn error_response_without_echo_does_not_match() {
    assert!(!response_echoes_request_id(&error_response(None), "req-1"));
}
