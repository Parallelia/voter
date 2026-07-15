//! Unit-level integration tests for the token protocol helpers and the
//! stored `VotingToken` wire encoding.

use base64::prelude::*;

use voter::crypto::token::{self, VotingToken};

fn token_with(signature_b64: &str, randomizer_b64: Option<&str>) -> VotingToken {
    VotingToken {
        nonce_b64: BASE64_STANDARD.encode([7u8; 32]),
        h_n: "cd".repeat(32),
        signature_b64: signature_b64.to_string(),
        randomizer_b64: randomizer_b64.map(str::to_string),
        consumed: false,
    }
}

#[test]
fn generate_nonce_returns_32_bytes() {
    let nonce = token::generate_nonce().unwrap();

    assert_eq!(nonce.len(), 32);
}

#[test]
fn generate_nonce_produces_different_values_on_each_call() {
    let first = token::generate_nonce().unwrap();
    let second = token::generate_nonce().unwrap();

    assert_ne!(first, second);
}

#[test]
fn compute_h_n_returns_known_sha256_hex_digest() {
    // Arrange: SHA-256 of the empty input is a fixed known answer.
    let expected = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    // Act
    let h_n = token::compute_h_n(b"");

    // Assert
    assert_eq!(h_n, expected);
}

#[test]
fn wire_token_without_randomizer_is_base64_of_signature_bytes() {
    // Arrange
    let sig_bytes = vec![1u8, 2, 3, 4];
    let voting_token = token_with(&BASE64_STANDARD.encode(&sig_bytes), None);

    // Act
    let wire = voting_token.wire_token().unwrap();

    // Assert
    assert_eq!(BASE64_STANDARD.decode(&wire).unwrap(), sig_bytes);
}

#[test]
fn wire_token_appends_randomizer_bytes_after_signature() {
    // Arrange
    let sig_bytes = vec![9u8; 16];
    let randomizer = vec![5u8; 32];
    let voting_token = token_with(
        &BASE64_STANDARD.encode(&sig_bytes),
        Some(&BASE64_STANDARD.encode(&randomizer)),
    );

    // Act
    let wire = voting_token.wire_token().unwrap();

    // Assert
    let decoded = BASE64_STANDARD.decode(&wire).unwrap();
    assert_eq!(decoded.len(), sig_bytes.len() + randomizer.len());
    assert_eq!(&decoded[..16], sig_bytes.as_slice());
    assert_eq!(&decoded[16..], randomizer.as_slice());
}

#[test]
fn wire_token_fails_for_corrupt_signature_base64() {
    let voting_token = token_with("!!!not base64!!!", None);

    let result = voting_token.wire_token();

    assert!(result.is_err());
}

#[test]
fn wire_token_fails_for_corrupt_randomizer_base64() {
    let voting_token = token_with(&BASE64_STANDARD.encode([1u8; 8]), Some("%%%bad%%%"));

    let result = voting_token.wire_token();

    assert!(result.is_err());
}

#[test]
fn begin_token_request_fails_for_invalid_rsa_key() {
    let result = token::begin_token_request("election-1", "not-a-valid-rsa-key");

    assert!(result.is_err());
}
