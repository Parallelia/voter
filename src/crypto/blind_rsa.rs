#![allow(dead_code)]

use base64::prelude::*;
use blind_rsa_signatures::{
    BlindSignature, BlindingResult, MessageRandomizer, PSS, PublicKey, Randomized, SecretKey,
    Sha384, Signature,
};
use sha2::{Digest, Sha256};

use crate::error::{Result, VoterError};

/// Type aliases matching the EC's parameterization.
pub type BrsaPk = PublicKey<Sha384, PSS, Randomized>;
pub type BrsaSk = SecretKey<Sha384, PSS, Randomized>;

/// Size of MessageRandomizer in bytes (fixed at 32 per blind-rsa-signatures).
const MESSAGE_RANDOMIZER_LEN: usize = 32;

/// Parse a base64 DER-encoded RSA public key (as published in Kind 35000
/// election events).
pub fn parse_pk_b64(b64: &str) -> Result<BrsaPk> {
    let der = BASE64_STANDARD
        .decode(b64)
        .map_err(|e| VoterError::Crypto(format!("base64 decode rsa_pub_key: {e}")))?;
    BrsaPk::from_der(&der).map_err(|e| VoterError::Crypto(format!("invalid RSA public key: {e}")))
}

/// Blind a nonce hash using the election's RSA public key.
///
/// Returns the blinding result (contains the blind message to send to EC
/// and the secret needed to finalize) and the base64-encoded blind message.
pub fn blind_nonce(pk: &BrsaPk, h_n: &[u8]) -> Result<(BlindingResult, String)> {
    let mut rng = blind_rsa_signatures::DefaultRng;
    let result = pk
        .blind(&mut rng, h_n)
        .map_err(|e| VoterError::Crypto(format!("blind failed: {e}")))?;
    let blind_msg_bytes: &[u8] = result.blind_message.as_ref();
    let blind_msg_b64 = BASE64_STANDARD.encode(blind_msg_bytes);
    Ok((result, blind_msg_b64))
}

/// Finalize (unblind) the EC's blind signature to produce a valid signature.
pub fn finalize_token(
    pk: &BrsaPk,
    blind_sig_b64: &str,
    blinding_result: &BlindingResult,
    h_n: &[u8],
) -> Result<(Signature, Option<MessageRandomizer>)> {
    let blind_sig_bytes = BASE64_STANDARD
        .decode(blind_sig_b64)
        .map_err(|e| VoterError::Crypto(format!("base64 decode blind_sig: {e}")))?;
    let blind_sig = BlindSignature(blind_sig_bytes);
    let sig = pk
        .finalize(&blind_sig, blinding_result, h_n)
        .map_err(|e| VoterError::Crypto(format!("finalize failed: {e}")))?;
    Ok((sig, blinding_result.msg_randomizer))
}

/// Verify a finalized signature against the election's public key.
pub fn verify_token(
    pk: &BrsaPk,
    sig: &Signature,
    msg_randomizer: Option<MessageRandomizer>,
    h_n: &[u8],
) -> Result<()> {
    pk.verify(sig, msg_randomizer, h_n)
        .map_err(|e| VoterError::Crypto(format!("verify failed: {e}")))
}

/// Encode a signature + optional randomizer as a base64 token string
/// for the cast-vote message.
pub fn encode_token(sig: &Signature, msg_randomizer: Option<MessageRandomizer>) -> String {
    let mut bytes: Vec<u8> = sig.0.clone();
    if let Some(r) = msg_randomizer {
        bytes.extend_from_slice(r.as_ref());
    }
    BASE64_STANDARD.encode(&bytes)
}

/// Decode a base64 token string back into signature bytes and optional
/// randomizer (MESSAGE_RANDOMIZER_LEN bytes).
pub fn decode_token(token_b64: &str) -> Result<(Vec<u8>, Option<Vec<u8>>)> {
    let bytes = BASE64_STANDARD
        .decode(token_b64)
        .map_err(|e| VoterError::Crypto(format!("base64 decode token: {e}")))?;
    if bytes.len() > MESSAGE_RANDOMIZER_LEN {
        let split = bytes.len() - MESSAGE_RANDOMIZER_LEN;
        let sig_bytes = bytes[..split].to_vec();
        let randomizer = bytes[split..].to_vec();
        Ok((sig_bytes, Some(randomizer)))
    } else {
        Ok((bytes, None))
    }
}

/// Compute SHA-256 hash of a nonce, returned as raw bytes.
pub fn compute_h_n(nonce: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(nonce);
    hasher.finalize().to_vec()
}

/// Compute SHA-256 hash of a nonce, returned as hex string (for protocol messages).
pub fn compute_h_n_hex(nonce: &[u8]) -> String {
    hex::encode(compute_h_n(nonce))
}

/// Generate a new RSA keypair for testing purposes only.
#[doc(hidden)]
pub fn generate_test_keypair() -> (BrsaPk, BrsaSk) {
    use blind_rsa_signatures::KeyPair;
    let kp =
        KeyPair::<Sha384, PSS, Randomized>::generate(&mut blind_rsa_signatures::DefaultRng, 2048)
            .expect("keygen");
    (kp.pk, kp.sk)
}
