use base64::prelude::*;
use blind_rsa_signatures::BlindingResult;
use serde::{Deserialize, Serialize};

use crate::crypto::blind_rsa::{self, BrsaPk};
use crate::error::{Result, VoterError};

/// A voting token obtained via the blind RSA signing protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VotingToken {
    /// The random 32-byte nonce (base64-encoded). Never sent to EC.
    pub nonce_b64: String,
    /// SHA-256(nonce) as hex. Sent as h_n in cast-vote.
    pub h_n: String,
    /// The finalized (unblinded) RSA signature (base64-encoded).
    pub signature_b64: String,
    /// The 32-byte message randomizer (base64-encoded), if present.
    pub randomizer_b64: Option<String>,
    /// Whether this token has been used to cast a vote.
    pub consumed: bool,
}

impl VotingToken {
    /// Encode this token for the cast-vote message:
    /// `base64(signature_bytes ++ randomizer_32bytes)`.
    pub fn wire_token(&self) -> Result<String> {
        let mut bytes = BASE64_STANDARD
            .decode(&self.signature_b64)
            .map_err(|e| VoterError::Crypto(format!("stored signature invalid: {e}")))?;
        if let Some(r) = &self.randomizer_b64 {
            let randomizer = BASE64_STANDARD
                .decode(r)
                .map_err(|e| VoterError::Crypto(format!("stored randomizer invalid: {e}")))?;
            bytes.extend_from_slice(&randomizer);
        }
        Ok(BASE64_STANDARD.encode(&bytes))
    }
}

/// Client-side secret state of an in-flight token request. Holds the nonce
/// and blinding factors needed to finalize the EC's blind signature.
/// Never leaves the process.
pub struct PendingBlind {
    pub election_id: String,
    nonce: [u8; 32],
    blinding: BlindingResult,
    pk: BrsaPk,
}

/// Start the token request protocol for an election:
/// generate a nonce, hash it, and blind the hash with the election's RSA key.
///
/// Returns the secret pending state plus the base64 blinded message to send
/// to the EC as `blinded_nonce`.
pub fn begin_token_request(
    election_id: &str,
    rsa_pub_key_b64: &str,
) -> Result<(PendingBlind, String)> {
    let pk = blind_rsa::parse_pk_b64(rsa_pub_key_b64)?;
    let nonce = generate_nonce()?;
    let h_n = blind_rsa::compute_h_n(&nonce);
    let (blinding, blinded_b64) = blind_rsa::blind_nonce(&pk, &h_n)?;
    Ok((
        PendingBlind {
            election_id: election_id.to_string(),
            nonce,
            blinding,
            pk,
        },
        blinded_b64,
    ))
}

/// Complete the token request protocol: unblind the EC's blind signature,
/// verify it against the election key, and package it as a [`VotingToken`].
pub fn complete_token_request(pending: PendingBlind, blind_sig_b64: &str) -> Result<VotingToken> {
    let h_n = blind_rsa::compute_h_n(&pending.nonce);
    let (sig, randomizer) =
        blind_rsa::finalize_token(&pending.pk, blind_sig_b64, &pending.blinding, &h_n)?;
    // Never store a token we couldn't verify — an invalid signature would be
    // rejected at cast time, permanently burning the voter's only token slot.
    blind_rsa::verify_token(&pending.pk, &sig, randomizer, &h_n)?;

    Ok(VotingToken {
        nonce_b64: BASE64_STANDARD.encode(pending.nonce),
        h_n: hex::encode(&h_n),
        signature_b64: BASE64_STANDARD.encode(&sig.0),
        randomizer_b64: randomizer.map(|r| {
            let bytes: &[u8] = r.as_ref();
            BASE64_STANDARD.encode(bytes)
        }),
        consumed: false,
    })
}

/// Generate a cryptographically random 32-byte nonce.
pub fn generate_nonce() -> Result<[u8; 32]> {
    let mut nonce = [0u8; 32];
    getrandom::fill(&mut nonce)
        .map_err(|e| VoterError::Crypto(format!("random nonce generation failed: {e}")))?;
    Ok(nonce)
}

/// Compute the nonce hash (h_n) as a hex-encoded SHA-256 digest.
pub fn compute_h_n(nonce: &[u8]) -> String {
    blind_rsa::compute_h_n_hex(nonce)
}
