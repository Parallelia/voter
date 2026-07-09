//! End-to-end test of the client-side token protocol helpers against an
//! EC-style blind signer, including the wire-format token the EC verifies
//! at cast-vote time.

use base64::prelude::*;
use blind_rsa_signatures::{MessageRandomizer, Signature};

use voter::crypto::blind_rsa;
use voter::crypto::token::{begin_token_request, complete_token_request};

#[test]
fn full_token_request_roundtrip_produces_verifiable_wire_token() {
    // Election keypair as the EC would publish it (base64 DER).
    let (pk, sk) = blind_rsa::generate_test_keypair();
    let pk_b64 = BASE64_STANDARD.encode(pk.to_der().expect("pk to der"));

    // Voter side: blind a fresh nonce.
    let (pending, blinded_nonce_b64) =
        begin_token_request("election-1", &pk_b64).expect("begin token request");
    assert_eq!(pending.election_id, "election-1");

    // EC side: blind-sign the blinded nonce.
    let blinded = BASE64_STANDARD.decode(&blinded_nonce_b64).unwrap();
    let blind_sig = sk.blind_sign(&blinded).expect("EC blind sign");
    let blind_sig_b64 = BASE64_STANDARD.encode(&blind_sig.0);

    // Voter side: finalize, verify, and package the token.
    let token = complete_token_request(pending, &blind_sig_b64).expect("complete token request");
    assert!(!token.consumed);

    // The wire token must be base64(signature ++ randomizer(32)) and verify
    // against the election key over the h_n bytes — exactly what the EC's
    // cast-vote handler checks.
    let wire = token.wire_token().expect("wire token");
    let (sig_bytes, randomizer_bytes) = blind_rsa::decode_token(&wire).expect("decode wire token");
    let randomizer_bytes = randomizer_bytes.expect("randomized mode includes a randomizer");
    let randomizer_arr: [u8; 32] = randomizer_bytes.as_slice().try_into().unwrap();

    let h_n_bytes = hex::decode(&token.h_n).expect("h_n is hex");
    blind_rsa::verify_token(
        &pk,
        &Signature(sig_bytes),
        Some(MessageRandomizer::new(randomizer_arr)),
        &h_n_bytes,
    )
    .expect("wire token must verify against election key");
}

#[test]
fn corrupted_blind_signature_is_rejected_not_stored() {
    let (pk, _sk) = blind_rsa::generate_test_keypair();
    let pk_b64 = BASE64_STANDARD.encode(pk.to_der().expect("pk to der"));

    let (pending, _blinded) = begin_token_request("election-1", &pk_b64).expect("begin");

    // A garbage blind signature must fail finalize/verify — never be stored.
    let garbage = BASE64_STANDARD.encode([0u8; 256]);
    assert!(complete_token_request(pending, &garbage).is_err());
}
