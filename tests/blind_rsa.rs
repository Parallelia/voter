use voter::crypto::blind_rsa;

#[test]
fn blind_sign_finalize_verify_roundtrip() {
    let (pk, sk) = blind_rsa::generate_test_keypair();

    let nonce = b"test-nonce-32-bytes-padded-ok!!!";
    let h_n = blind_rsa::compute_h_n(nonce);

    // Blind
    let (blinding_result, blind_msg_b64) = blind_rsa::blind_nonce(&pk, &h_n).unwrap();
    assert!(!blind_msg_b64.is_empty());

    // Server signs the blind message
    let blind_msg_bytes =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &blind_msg_b64).unwrap();
    let blind_sig: blind_rsa_signatures::BlindSignature = sk.blind_sign(&blind_msg_bytes).unwrap();
    let blind_sig_b64 =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &blind_sig.0);

    // Finalize (unblind)
    let (sig, msg_randomizer) =
        blind_rsa::finalize_token(&pk, &blind_sig_b64, &blinding_result, &h_n).unwrap();

    // Verify
    blind_rsa::verify_token(&pk, &sig, msg_randomizer, &h_n).unwrap();
}

#[test]
fn tampered_signature_fails_verification() {
    let (pk, sk) = blind_rsa::generate_test_keypair();

    let nonce = b"another-nonce-32-bytes-padded!!";
    let h_n = blind_rsa::compute_h_n(nonce);

    let (blinding_result, blind_msg_b64) = blind_rsa::blind_nonce(&pk, &h_n).unwrap();

    let blind_msg_bytes =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &blind_msg_b64).unwrap();
    let blind_sig: blind_rsa_signatures::BlindSignature = sk.blind_sign(&blind_msg_bytes).unwrap();
    let blind_sig_b64 =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &blind_sig.0);

    let (mut sig, msg_randomizer) =
        blind_rsa::finalize_token(&pk, &blind_sig_b64, &blinding_result, &h_n).unwrap();

    // Tamper with the signature
    if let Some(byte) = sig.0.first_mut() {
        *byte ^= 0xFF;
    }

    let result = blind_rsa::verify_token(&pk, &sig, msg_randomizer, &h_n);
    assert!(result.is_err());
}

#[test]
fn encode_decode_token_roundtrip() {
    let (pk, sk) = blind_rsa::generate_test_keypair();

    let nonce = b"roundtrip-nonce-32-bytes-pad!!!";
    let h_n = blind_rsa::compute_h_n(nonce);

    let (blinding_result, blind_msg_b64) = blind_rsa::blind_nonce(&pk, &h_n).unwrap();

    let blind_msg_bytes =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &blind_msg_b64).unwrap();
    let blind_sig: blind_rsa_signatures::BlindSignature = sk.blind_sign(&blind_msg_bytes).unwrap();
    let blind_sig_b64 =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &blind_sig.0);

    let (sig, msg_randomizer) =
        blind_rsa::finalize_token(&pk, &blind_sig_b64, &blinding_result, &h_n).unwrap();

    // Encode
    let token_b64 = blind_rsa::encode_token(&sig, msg_randomizer);
    assert!(!token_b64.is_empty());

    // Decode
    let (sig_bytes, randomizer) = blind_rsa::decode_token(&token_b64).unwrap();
    assert_eq!(sig_bytes, sig.0);
    match (msg_randomizer, randomizer) {
        (Some(expected), Some(actual)) => {
            let expected_bytes: &[u8] = expected.as_ref();
            assert_eq!(expected_bytes, actual.as_slice());
        }
        (None, None) => {}
        _ => panic!("randomizer presence mismatch"),
    }
}

#[test]
fn compute_h_n_is_deterministic() {
    let nonce = b"deterministic-test";
    let h1 = blind_rsa::compute_h_n(nonce);
    let h2 = blind_rsa::compute_h_n(nonce);
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 32); // SHA-256 raw = 32 bytes
}

#[test]
fn compute_h_n_different_inputs_differ() {
    let h1 = blind_rsa::compute_h_n(b"input-a");
    let h2 = blind_rsa::compute_h_n(b"input-b");
    assert_ne!(h1, h2);
}

#[test]
fn compute_h_n_hex_returns_64_char_string() {
    let nonce = b"hex-test-nonce";
    let hex = blind_rsa::compute_h_n_hex(nonce);
    assert_eq!(hex.len(), 64);
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
}

use base64::prelude::*;

#[test]
fn decode_token_splits_trailing_32_bytes_as_randomizer() {
    // Arrange: 8 signature bytes followed by a 32-byte randomizer.
    let sig_bytes = [1u8; 8];
    let randomizer = [2u8; 32];
    let mut combined = sig_bytes.to_vec();
    combined.extend_from_slice(&randomizer);
    let token_b64 = BASE64_STANDARD.encode(&combined);

    // Act
    let (decoded_sig, decoded_randomizer) = blind_rsa::decode_token(&token_b64).unwrap();

    // Assert
    assert_eq!(decoded_sig, sig_bytes.to_vec());
    assert_eq!(decoded_randomizer, Some(randomizer.to_vec()));
}

#[test]
fn decode_token_short_input_has_no_randomizer() {
    // Arrange: exactly 32 bytes — not enough for signature + randomizer.
    let bytes = [7u8; 32];
    let token_b64 = BASE64_STANDARD.encode(bytes);

    // Act
    let (decoded_sig, decoded_randomizer) = blind_rsa::decode_token(&token_b64).unwrap();

    // Assert
    assert_eq!(decoded_sig, bytes.to_vec());
    assert_eq!(decoded_randomizer, None);
}

#[test]
fn decode_token_fails_for_invalid_base64() {
    let result = blind_rsa::decode_token("***not-base64***");

    assert!(result.is_err());
}

#[test]
fn compute_h_n_matches_known_sha256_answers() {
    // Known-answer test vectors for SHA-256.
    assert_eq!(
        hex::encode(blind_rsa::compute_h_n(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        blind_rsa::compute_h_n_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn parse_pk_b64_fails_for_invalid_base64() {
    let result = blind_rsa::parse_pk_b64("!!!definitely not base64!!!");

    assert!(result.is_err());
}

#[test]
fn parse_pk_b64_fails_for_valid_base64_invalid_der() {
    // Arrange: valid base64 that decodes to garbage, not an RSA key DER.
    let not_der = BASE64_STANDARD.encode(b"this is not a DER encoded key");

    // Act
    let result = blind_rsa::parse_pk_b64(&not_der);

    // Assert
    assert!(result.is_err());
}

#[test]
fn finalize_token_fails_for_invalid_base64_signature() {
    // Arrange
    let (pk, _sk) = blind_rsa::generate_test_keypair();
    let h_n = blind_rsa::compute_h_n(b"finalize-bad-b64");
    let (blinding_result, _blind_msg) = blind_rsa::blind_nonce(&pk, &h_n).unwrap();

    // Act
    let result = blind_rsa::finalize_token(&pk, "%%%broken%%%", &blinding_result, &h_n);

    // Assert
    assert!(result.is_err());
}
