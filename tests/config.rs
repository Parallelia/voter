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
fn config_paths_are_non_empty() {
    let dir = voter::config::config_dir();
    let config_file = voter::config::config_path();
    let state_file = AppConfig::default().state_path();

    assert!(!dir.as_os_str().is_empty());
    assert!(config_file.ends_with("voter.toml"));
    assert!(state_file.ends_with("state.json"));
}
