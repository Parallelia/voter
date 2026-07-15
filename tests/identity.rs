use voter::identity;

#[test]
fn generate_and_save_plaintext() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.json");

    let keys = identity::generate_keypair();
    identity::save_identity(&keys, None, &path).unwrap();

    assert!(path.exists());
    let loaded = identity::load_identity(None, &path).unwrap();
    assert_eq!(
        identity::export_public_key(&keys),
        identity::export_public_key(&loaded)
    );
}

#[test]
fn encrypt_decrypt_with_password() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.json");

    let keys = identity::generate_keypair();
    identity::save_identity(&keys, Some("testpass"), &path).unwrap();

    let encrypted_path = path.with_extension("age");
    assert!(encrypted_path.exists());

    let loaded = identity::load_identity(Some("testpass"), &path).unwrap();
    assert_eq!(
        identity::export_public_key(&keys),
        identity::export_public_key(&loaded)
    );
}

#[test]
fn wrong_password_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.json");

    let keys = identity::generate_keypair();
    identity::save_identity(&keys, Some("correct"), &path).unwrap();

    let result = identity::load_identity(Some("wrong"), &path);
    assert!(result.is_err());
}

#[test]
fn identity_exists_plaintext() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.json");

    assert!(!identity::identity_exists(&path));

    let keys = identity::generate_keypair();
    identity::save_identity(&keys, None, &path).unwrap();

    assert!(identity::identity_exists(&path));
    assert!(!identity::identity_is_encrypted(&path));
}

#[test]
fn identity_exists_encrypted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.json");

    let keys = identity::generate_keypair();
    identity::save_identity(&keys, Some("pass"), &path).unwrap();

    assert!(identity::identity_exists(&path));
    assert!(identity::identity_is_encrypted(&path));
}

#[test]
fn load_nonexistent_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nonexistent.json");

    let result = identity::load_identity(None, &path);
    assert!(result.is_err());
}

#[test]
fn export_public_key_is_hex() {
    let keys = identity::generate_keypair();
    let pubkey = identity::export_public_key(&keys);
    assert_eq!(pubkey.len(), 64);
    assert!(pubkey.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn import_keypair_roundtrip() {
    let keys = identity::generate_keypair();
    let secret_hex = keys.secret_key().to_secret_hex();
    let imported = identity::import_keypair(&secret_hex).unwrap();
    assert_eq!(
        identity::export_public_key(&keys),
        identity::export_public_key(&imported)
    );
}

#[test]
fn import_keypair_invalid_hex_fails() {
    let result = identity::import_keypair("not-a-valid-hex-key");
    assert!(result.is_err());
}

/// Identity files must end up owner-only (0600) even when overwriting a
/// pre-existing file that had broader permissions (e.g. created by an older
/// version): OpenOptions::mode() alone only applies at creation time.
#[cfg(unix)]
#[test]
fn save_identity_tightens_permissions_of_existing_file() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.json");

    // Simulate an old identity file with world-readable permissions.
    std::fs::write(&path, "{}").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

    let keys = identity::generate_keypair();
    identity::save_identity(&keys, None, &path).unwrap();

    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600, "identity file must be owner-only");
}

#[test]
fn load_plaintext_identity_ignores_supplied_password() {
    // Arrange: a plaintext identity, no encryption.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.json");
    let keys = identity::generate_keypair();
    identity::save_identity(&keys, None, &path).unwrap();

    // Act: supply a password anyway — it must be ignored for plaintext files.
    let loaded = identity::load_identity(Some("irrelevant-password"), &path).unwrap();

    // Assert
    assert_eq!(loaded.public_key(), keys.public_key());
}

#[test]
fn save_with_empty_password_writes_plaintext_file() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.json");
    let keys = identity::generate_keypair();

    // Act: Some("") counts as "no password" — current behavior is plaintext.
    identity::save_identity(&keys, Some(""), &path).unwrap();

    // Assert
    assert!(path.exists(), "plaintext file must be written");
    assert!(
        !path.with_extension("age").exists(),
        "no encrypted file must be written for an empty password"
    );
    let loaded = identity::load_identity(None, &path).unwrap();
    assert_eq!(loaded.public_key(), keys.public_key());
}

#[test]
fn load_fails_for_corrupt_identity_json() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.json");
    std::fs::write(&path, "{ this is not json").unwrap();

    // Act
    let result = identity::load_identity(None, &path);

    // Assert
    assert!(result.is_err());
}

#[test]
fn load_fails_for_valid_json_with_invalid_secret_key_hex() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.json");
    std::fs::write(&path, r#"{"secret_key": "zzzz-not-hex"}"#).unwrap();

    // Act
    let result = identity::load_identity(None, &path);

    // Assert
    assert!(result.is_err());
}

#[test]
fn load_fails_for_encrypted_file_with_garbage_content() {
    // Arrange: a .age file that is not a valid age ciphertext.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.json");
    std::fs::write(path.with_extension("age"), b"garbage, not age data").unwrap();

    // Act
    let result = identity::load_identity(Some("any-password"), &path);

    // Assert
    assert!(result.is_err());
}

#[test]
fn load_encrypted_identity_without_password_fails() {
    // Arrange: a properly encrypted identity.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.json");
    let keys = identity::generate_keypair();
    identity::save_identity(&keys, Some("secret-pw"), &path).unwrap();

    // Act
    let result = identity::load_identity(None, &path);

    // Assert
    assert!(result.is_err());
}
