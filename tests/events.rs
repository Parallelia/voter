//! Regression tests: the event schemas must parse exactly what the EC daemon
//! publishes (see the `ec` repository, `src/nostr/publisher.rs`). The EC uses
//! unix-timestamp integers, snake_case status strings, and float vote totals.

use voter::nostr::events::{Election, ElectionResults, ElectionStatus, format_unix_utc};
use voter::nostr::messages::{EcErrorCode, EcResponse};

/// Verbatim shape of a Kind 35000 event published by the EC.
#[test]
fn parses_ec_election_announcement() {
    let json = r#"{
        "election_id": "V1StGXR8_Z5jdHi6B-myT",
        "name": "Board Election 2026",
        "start_time": 1751932800,
        "end_time": 1751936400,
        "status": "in_progress",
        "rules_id": "plurality",
        "rsa_pub_key": "MIIBIjANBgkq...",
        "candidates": [
            {"id": 1, "name": "Alice"},
            {"id": 2, "name": "Bob"}
        ]
    }"#;

    let election: Election = serde_json::from_str(json).expect("must parse EC announcement");
    assert_eq!(election.election_id, "V1StGXR8_Z5jdHi6B-myT");
    assert_eq!(election.status, ElectionStatus::InProgress);
    assert_eq!(election.start_time, 1_751_932_800);
    assert_eq!(election.end_time, 1_751_936_400);
    assert_eq!(election.candidates.len(), 2);
}

#[test]
fn parses_all_ec_status_values() {
    for (raw, expected) in [
        ("open", ElectionStatus::Open),
        ("in_progress", ElectionStatus::InProgress),
        ("finished", ElectionStatus::Finished),
        ("cancelled", ElectionStatus::Cancelled),
    ] {
        let parsed: ElectionStatus =
            serde_json::from_str(&format!("\"{raw}\"")).unwrap_or_else(|e| {
                panic!("status {raw:?} must parse: {e}");
            });
        assert_eq!(parsed, expected);
    }
}

/// Verbatim shape of a Kind 35001 result event published by the EC.
/// STV vote totals are fractional (weighted surplus transfers); plurality
/// totals serialize as `3.0`. Both must parse.
#[test]
fn parses_ec_results_with_fractional_votes() {
    let json = r#"{
        "election_id": "V1StGXR8_Z5jdHi6B-myT",
        "name": "Board Election 2026",
        "rules_id": "stv",
        "elected": [1, 3],
        "tally": [
            {"candidate_id": 1, "votes": 3.0, "status": "elected"},
            {"candidate_id": 2, "votes": 2.5, "status": "excluded"},
            {"candidate_id": 3, "votes": 3.0, "status": "elected"}
        ],
        "count_sheet": [
            {"round": 1, "action": "Elected: 1 (quota 3.0000)", "tallies": []}
        ]
    }"#;

    let results: ElectionResults = serde_json::from_str(json).expect("must parse EC results");
    assert_eq!(results.elected, vec![1, 3]);
    assert_eq!(results.tally[1].votes, 2.5);
}

/// An error code introduced by a newer EC must not make the whole response
/// unreadable.
#[test]
fn unknown_error_code_still_parses() {
    let json = r#"{"status":"error","code":"SOME_FUTURE_CODE","message":"whatever"}"#;
    let resp: EcResponse = serde_json::from_str(json).expect("must parse");
    match resp {
        EcResponse::Error { code, .. } => assert_eq!(code, EcErrorCode::Unknown),
        _ => panic!("expected error response"),
    }
}

#[test]
fn formats_unix_timestamps_as_utc() {
    assert_eq!(format_unix_utc(0), "1970-01-01 00:00 UTC");
    assert_eq!(format_unix_utc(1_751_932_800), "2025-07-08 00:00 UTC");
    // Leap-year day
    assert_eq!(format_unix_utc(1_709_164_800), "2024-02-29 00:00 UTC");
}

#[test]
fn formats_negative_timestamp_before_epoch() {
    assert_eq!(format_unix_utc(-86_400), "1969-12-31 00:00 UTC");
}

#[test]
fn formats_end_of_year_timestamp() {
    // 2023-12-31T23:59:59Z — seconds are truncated to the minute.
    assert_eq!(format_unix_utc(1_704_067_199), "2023-12-31 23:59 UTC");
}

#[test]
fn election_status_display_covers_all_variants() {
    let cases = [
        (ElectionStatus::Open, "Open"),
        (ElectionStatus::InProgress, "In Progress"),
        (ElectionStatus::Finished, "Finished"),
        (ElectionStatus::Cancelled, "Cancelled"),
    ];

    for (status, expected) in cases {
        assert_eq!(status.to_string(), expected);
    }
}

/// `ec_pubkey` is `#[serde(skip)]`: it must stay `None` after deserialization
/// even if a malicious event embeds an `ec_pubkey` field in the JSON body.
#[test]
fn election_ec_pubkey_is_never_taken_from_json() {
    let json = r#"{
        "election_id": "e1",
        "name": "Test Election",
        "start_time": 1751932800,
        "end_time": 1751936400,
        "status": "open",
        "rules_id": "plurality",
        "rsa_pub_key": "AAAA",
        "candidates": [{"id": 1, "name": "Alice"}],
        "ec_pubkey": "attacker-controlled"
    }"#;

    let election: Election = serde_json::from_str(json).unwrap();

    assert_eq!(election.ec_pubkey, None);
}

#[test]
fn unknown_election_status_string_fails_to_parse() {
    let result = serde_json::from_str::<ElectionStatus>("\"paused\"");

    assert!(result.is_err());
}
