//! Integration tests for the persistent app state (registrations and tokens).

use std::fs;

use voter::crypto::token::VotingToken;
use voter::state::AppState;

fn sample_token(consumed: bool) -> VotingToken {
    VotingToken {
        nonce_b64: "bm9uY2U=".to_string(),
        h_n: "ab".repeat(32),
        signature_b64: "c2lnbmF0dXJl".to_string(),
        randomizer_b64: None,
        consumed,
    }
}

#[test]
fn load_returns_default_state_when_file_is_missing() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");

    // Act
    let state = AppState::load(&path).unwrap();

    // Assert
    assert!(state.registrations.is_empty());
    assert!(state.tokens.is_empty());
}

#[test]
fn load_reads_back_previously_saved_state() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let mut state = AppState::default();
    state.mark_registered("election-1".to_string());
    state.store_token("election-1".to_string(), sample_token(false));
    state.save(&path).unwrap();

    // Act
    let loaded = AppState::load(&path).unwrap();

    // Assert
    assert!(loaded.is_registered("election-1"));
    assert!(loaded.get_active_token("election-1").is_some());
}

#[test]
fn load_returns_error_for_corrupt_file() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    fs::write(&path, "this is { not valid json").unwrap();

    // Act
    let result = AppState::load(&path);

    // Assert
    assert!(result.is_err());
}

#[test]
fn save_creates_missing_parent_directories() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deeply").join("nested").join("state.json");

    // Act
    AppState::default().save(&path).unwrap();

    // Assert
    assert!(path.exists());
}

#[test]
fn is_registered_returns_false_for_unknown_election() {
    let state = AppState::default();

    assert!(!state.is_registered("nope"));
}

#[test]
fn mark_registered_records_registration_with_timestamp() {
    // Arrange
    let mut state = AppState::default();

    // Act
    state.mark_registered("election-1".to_string());

    // Assert
    assert!(state.is_registered("election-1"));
    let registration = state.registrations.get("election-1").unwrap();
    assert!(registration.registered);
    assert!(!registration.registered_at.is_empty());
}

#[test]
fn get_active_token_returns_none_when_no_token_stored() {
    let state = AppState::default();

    assert!(state.get_active_token("election-1").is_none());
}

#[test]
fn get_active_token_returns_stored_unconsumed_token() {
    // Arrange
    let mut state = AppState::default();
    state.store_token("election-1".to_string(), sample_token(false));

    // Act
    let token = state.get_active_token("election-1");

    // Assert
    assert!(token.is_some_and(|t| !t.consumed));
}

#[test]
fn get_active_token_returns_none_when_token_is_consumed() {
    // Arrange
    let mut state = AppState::default();
    state.store_token("election-1".to_string(), sample_token(true));

    // Act & Assert
    assert!(state.get_active_token("election-1").is_none());
}

#[test]
fn consume_token_marks_active_token_as_consumed() {
    // Arrange
    let mut state = AppState::default();
    state.store_token("election-1".to_string(), sample_token(false));

    // Act
    state.consume_token("election-1").unwrap();

    // Assert
    assert!(state.tokens.get("election-1").unwrap().consumed);
}

#[test]
fn consume_token_fails_when_no_token_exists() {
    let mut state = AppState::default();

    let result = state.consume_token("election-1");

    assert!(result.is_err());
}

#[test]
fn consume_token_fails_on_second_consumption() {
    // Arrange
    let mut state = AppState::default();
    state.store_token("election-1".to_string(), sample_token(false));
    state.consume_token("election-1").unwrap();

    // Act
    let second = state.consume_token("election-1");

    // Assert
    assert!(second.is_err());
}

#[test]
fn has_voted_reflects_token_consumption() {
    // Arrange
    let mut state = AppState::default();

    // Assert: no token at all
    assert!(!state.has_voted("election-1"));

    // Assert: unconsumed token
    state.store_token("election-1".to_string(), sample_token(false));
    assert!(!state.has_voted("election-1"));

    // Assert: consumed token
    state.consume_token("election-1").unwrap();
    assert!(state.has_voted("election-1"));
}
