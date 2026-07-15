use std::io::{Read as _, Write as _};
use std::path::Path;

use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::{Result, VoterError};

/// Plaintext identity format stored as JSON.
#[derive(Serialize, Deserialize)]
struct IdentityFile {
    secret_key: String,
}

/// Generate a new Nostr keypair.
pub fn generate_keypair() -> Keys {
    Keys::generate()
}

/// Import a keypair from a hex-encoded secret key.
#[allow(dead_code)]
pub fn import_keypair(hex_secret: &str) -> Result<Keys> {
    let sk = SecretKey::from_hex(hex_secret)
        .map_err(|e| VoterError::Identity(format!("invalid secret key hex: {e}")))?;
    Ok(Keys::new(sk))
}

/// Save a keypair to disk. If a password is provided, encrypt with age.
///
/// The file is written with owner-only permissions (0600) on Unix: it holds
/// the voter's secret key.
pub fn save_identity(keys: &Keys, password: Option<&str>, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let secret_hex = keys.secret_key().to_secret_hex();

    match password {
        Some(pw) if !pw.is_empty() => {
            let encrypted_path = path.with_extension("age");
            let encryptor = age::Encryptor::with_user_passphrase(age::secrecy::SecretString::from(
                pw.to_string(),
            ));
            let mut output = vec![];
            let mut writer = encryptor
                .wrap_output(&mut output)
                .map_err(|e| VoterError::Identity(format!("encryption init failed: {e}")))?;
            writer
                .write_all(secret_hex.as_bytes())
                .map_err(|e| VoterError::Identity(format!("encryption write failed: {e}")))?;
            writer
                .finish()
                .map_err(|e| VoterError::Identity(format!("encryption finish failed: {e}")))?;
            write_secret_file(&encrypted_path, &output)?;
        }
        _ => {
            let identity = IdentityFile {
                secret_key: secret_hex,
            };
            let json = serde_json::to_string_pretty(&identity)?;
            write_secret_file(path, json.as_bytes())?;
        }
    }

    Ok(())
}

/// Write a file readable only by its owner (0600 on Unix).
/// Also used for the persistent state file, which stores voting tokens
/// (bearer credentials).
///
/// The write is atomic: data goes to a fresh temp file in the same directory
/// (created 0600, so secrets are never world-readable even transiently),
/// is flushed to disk, and then renamed over the destination. A crash
/// mid-write can never leave a truncated or empty file — for state.json
/// that would destroy the voter's only voting token, which the EC will
/// not reissue.
#[cfg(unix)]
pub(crate) fn write_secret_file(path: &Path, data: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let tmp_path = temp_sibling_path(path)?;
    let write_result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp_path)?;
        file.write_all(data)?;
        file.sync_all()?;
        Ok(())
    })();
    finish_atomic_write(write_result, &tmp_path, path)
}

#[cfg(not(unix))]
pub(crate) fn write_secret_file(path: &Path, data: &[u8]) -> Result<()> {
    let tmp_path = temp_sibling_path(path)?;
    let write_result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        file.write_all(data)?;
        file.sync_all()?;
        Ok(())
    })();
    finish_atomic_write(write_result, &tmp_path, path)
}

/// A unique temp path next to `path` (same directory, so the final rename
/// stays on one filesystem and is atomic).
fn temp_sibling_path(path: &Path) -> Result<std::path::PathBuf> {
    let mut rand_bytes = [0u8; 8];
    getrandom::fill(&mut rand_bytes)
        .map_err(|e| VoterError::Identity(format!("system RNG unavailable: {e}")))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| VoterError::Identity(format!("invalid path: {}", path.display())))?;
    let tmp_name = format!(
        ".{}.tmp-{}",
        file_name.to_string_lossy(),
        hex::encode(rand_bytes)
    );
    Ok(path.with_file_name(tmp_name))
}

/// Complete an atomic write: rename the temp file over the destination on
/// success, remove it on failure.
fn finish_atomic_write(write_result: Result<()>, tmp_path: &Path, path: &Path) -> Result<()> {
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(tmp_path);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(tmp_path, path) {
        let _ = std::fs::remove_file(tmp_path);
        return Err(e.into());
    }
    Ok(())
}

/// Load a keypair from disk. If the file is age-encrypted, a password is required.
pub fn load_identity(password: Option<&str>, path: &Path) -> Result<Keys> {
    let encrypted_path = path.with_extension("age");

    if encrypted_path.exists() {
        let pw = password.ok_or_else(|| {
            VoterError::Identity("password required for encrypted identity".into())
        })?;
        let data = std::fs::read(&encrypted_path)?;
        let decryptor = age::Decryptor::new_buffered(&data[..])
            .map_err(|e| VoterError::Identity(format!("decryptor init failed: {e}")))?;
        let identity = age::scrypt::Identity::new(age::secrecy::SecretString::from(pw.to_string()));
        let mut reader = decryptor
            .decrypt(Some(&identity as &dyn age::Identity).into_iter())
            .map_err(|e| VoterError::Identity(format!("decryption failed: {e}")))?;
        let mut decrypted = String::new();
        reader
            .read_to_string(&mut decrypted)
            .map_err(|e| VoterError::Identity(format!("read decrypted data failed: {e}")))?;
        let sk = SecretKey::from_hex(decrypted.trim())
            .map_err(|e| VoterError::Identity(format!("invalid decrypted key: {e}")))?;
        Ok(Keys::new(sk))
    } else if path.exists() {
        let contents = std::fs::read_to_string(path)?;
        let identity: IdentityFile = serde_json::from_str(&contents)?;
        let sk = SecretKey::from_hex(&identity.secret_key)
            .map_err(|e| VoterError::Identity(format!("invalid stored key: {e}")))?;
        Ok(Keys::new(sk))
    } else {
        Err(VoterError::Identity("identity file not found".into()))
    }
}

/// Check if an identity file exists (encrypted or plaintext).
pub fn identity_exists(path: &Path) -> bool {
    path.exists() || path.with_extension("age").exists()
}

/// Check if the identity file is encrypted.
pub fn identity_is_encrypted(path: &Path) -> bool {
    path.with_extension("age").exists()
}

/// Export the public key as a hex string.
pub fn export_public_key(keys: &Keys) -> String {
    keys.public_key().to_hex()
}
