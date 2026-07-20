use std::fs;

use voter::config::AppConfig;

#[test]
fn default_config_has_relays() {
    let config = AppConfig::default();
    assert!(
        !config.nostr.relays.is_empty(),
        "default relays should not be empty"
    );
}

#[test]
fn missing_file_creates_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voter.toml");

    let config = AppConfig::load(&path).unwrap();
    assert!(path.exists(), "config file should be created");
    assert!(!config.nostr.relays.is_empty());
}

#[test]
fn load_custom_config() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voter.toml");

    let custom = r#"
[nostr]
relays = ["wss://custom.relay"]

[ui]
theme = "light"
"#;
    fs::write(&path, custom).unwrap();

    let config = AppConfig::load(&path).unwrap();
    assert_eq!(config.nostr.relays, vec!["wss://custom.relay"]);
    assert_eq!(config.ui.theme, voter::config::Theme::Light);
}

#[test]
fn save_and_reload_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voter.toml");

    let config = AppConfig::default();
    config.save(&path).unwrap();

    let loaded = AppConfig::load(&path).unwrap();
    assert_eq!(config.nostr.relays, loaded.nostr.relays);
}

#[test]
fn invalid_toml_returns_error() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voter.toml");
    fs::write(&path, "this is [[[ not valid toml").unwrap();

    // Act
    let result = AppConfig::load(&path);

    // Assert
    assert!(result.is_err());
}

#[test]
fn defaults_use_dark_theme_and_no_pinned_ec_pubkey() {
    let config = AppConfig::default();

    assert_eq!(config.ui.theme, voter::config::Theme::Dark);
    assert_eq!(config.nostr.ec_pubkey, None);
    assert_eq!(
        config.nostr.relays,
        vec![
            "wss://relay.mostro.network".to_string(),
            "wss://nos.lol".to_string()
        ]
    );
}

#[test]
fn save_creates_missing_parent_directories() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a").join("b").join("voter.toml");

    // Act
    AppConfig::default().save(&path).unwrap();

    // Assert
    assert!(path.exists());
}

#[test]
fn theme_serde_roundtrip_for_dark_and_light() {
    use voter::config::Theme;

    for (theme, expected_json) in [(Theme::Dark, "\"dark\""), (Theme::Light, "\"light\"")] {
        // Act
        let json = serde_json::to_string(&theme).unwrap();
        let parsed: Theme = serde_json::from_str(&json).unwrap();

        // Assert
        assert_eq!(json, expected_json);
        assert_eq!(parsed, theme);
    }
}

#[test]
fn ws_relay_is_rejected_at_load() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voter.toml");
    fs::write(
        &path,
        r#"
[nostr]
relays = ["wss://good.relay", "ws://plaintext.relay"]
"#,
    )
    .unwrap();

    // Act
    let err = AppConfig::load(&path).unwrap_err();

    // Assert: the error names the offending relay and points at the fix.
    let msg = err.to_string();
    assert!(msg.contains("ws://plaintext.relay"), "got: {msg}");
    assert!(msg.contains("wss://"), "got: {msg}");
}

#[test]
fn ws_relay_is_allowed_with_explicit_opt_in() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voter.toml");
    fs::write(
        &path,
        r#"
[nostr]
relays = ["ws://127.0.0.1:7777"]
allow_insecure_relays = true
"#,
    )
    .unwrap();

    // Act
    let config = AppConfig::load(&path).unwrap();

    // Assert
    assert_eq!(config.nostr.relays, vec!["ws://127.0.0.1:7777"]);
}

#[test]
fn non_websocket_relay_scheme_is_rejected() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voter.toml");
    fs::write(
        &path,
        r#"
[nostr]
relays = ["https://not-a-relay.example"]
"#,
    )
    .unwrap();

    // Act
    let err = AppConfig::load(&path).unwrap_err();

    // Assert
    let msg = err.to_string();
    assert!(msg.contains("https://not-a-relay.example"), "got: {msg}");
}

#[test]
fn opt_in_does_not_bypass_non_websocket_schemes() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voter.toml");
    fs::write(
        &path,
        r#"
[nostr]
relays = ["https://not-a-relay.example"]
allow_insecure_relays = true
"#,
    )
    .unwrap();

    // Act + Assert: the opt-in covers ws:// only, never arbitrary schemes.
    assert!(AppConfig::load(&path).is_err());
}

#[test]
fn relay_scheme_check_ignores_case_and_surrounding_whitespace() {
    // Arrange: URL schemes are case-insensitive (RFC 3986 §3.1), and a
    // hand-edited TOML easily picks up stray spaces. nostr-sdk accepts and
    // normalizes all of these, so the validator must not reject them.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voter.toml");
    fs::write(
        &path,
        r#"
[nostr]
relays = ["WSS://upper.relay", " wss://padded.relay ", "Wss://Mixed.Relay"]
"#,
    )
    .unwrap();

    // Act
    let config = AppConfig::load(&path).unwrap();

    // Assert: accepted, and stored verbatim for nostr-sdk to normalize.
    assert_eq!(config.nostr.relays.len(), 3);
}

#[test]
fn uppercase_ws_relay_is_reported_as_unencrypted() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voter.toml");
    fs::write(
        &path,
        r#"
[nostr]
relays = ["WS://plaintext.relay"]
"#,
    )
    .unwrap();

    // Act
    let err = AppConfig::load(&path).unwrap_err();

    // Assert: caught by the insecure-relay check itself, not the catch-all,
    // so the operator gets the actionable message rather than "not a websocket".
    let msg = err.to_string();
    assert!(msg.contains("WS://plaintext.relay"), "got: {msg}");
    assert!(msg.contains("unencrypted"), "got: {msg}");
}

#[test]
fn padded_ws_relay_is_reported_as_unencrypted() {
    // Arrange
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voter.toml");
    fs::write(
        &path,
        r#"
[nostr]
relays = ["  ws://plaintext.relay  "]
"#,
    )
    .unwrap();

    // Act
    let err = AppConfig::load(&path).unwrap_err();

    // Assert
    assert!(err.to_string().contains("unencrypted"), "got: {err}");
}

#[test]
fn insecure_opt_in_defaults_to_off_and_roundtrips() {
    let config = AppConfig::default();
    assert!(!config.nostr.allow_insecure_relays);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voter.toml");
    config.save(&path).unwrap();
    let loaded = AppConfig::load(&path).unwrap();
    assert!(!loaded.nostr.allow_insecure_relays);
}

#[test]
fn save_rejects_insecure_relays_without_opt_in() {
    // Arrange: a config mutated in memory to hold a ws:// relay.
    let mut config = AppConfig::default();
    config.nostr.relays = vec!["ws://sneaky.relay".to_string()];
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voter.toml");

    // Act + Assert: persisting it must fail the same validation as load.
    assert!(config.save(&path).is_err());
    assert!(!path.exists(), "invalid config must not be written");
}

#[test]
fn config_paths_are_non_empty() {
    let dir = voter::config::config_dir();
    let config_file = voter::config::config_path();
    let state_file = AppConfig::default().state_path();

    assert!(!dir.as_os_str().is_empty());
    assert!(config_file.ends_with("voter.toml"));
    assert!(state_file.ends_with("state.json"));
}
