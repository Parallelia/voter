//! Offline tests for the Nostr client wrapper: connection validation and
//! command processing paths that do not need a live relay.

use tokio::sync::mpsc;

use voter::config::{AppConfig, NostrConfig};
use voter::nostr::client::{NostrAction, NostrVoterClient, REQUEST_TIMEOUT, VoterCommand};
use voter::nostr::messages::VoterMessage;

fn offline_config(relays: Vec<String>, ec_pubkey: Option<String>) -> AppConfig {
    AppConfig {
        nostr: NostrConfig {
            relays,
            ec_pubkey,
            allow_insecure_relays: false,
        },
        ..AppConfig::default()
    }
}

#[test]
fn request_timeout_is_thirty_seconds() {
    assert_eq!(REQUEST_TIMEOUT, std::time::Duration::from_secs(30));
}

#[tokio::test]
async fn connect_fails_when_configured_ec_pubkey_is_invalid() {
    // Arrange
    let keys = voter::identity::generate_keypair();
    let config = offline_config(vec![], Some("not-a-key".to_string()));

    // Act
    let result = NostrVoterClient::connect(&keys, &config).await;

    // Assert
    assert!(result.is_err());
}

#[tokio::test]
async fn connect_succeeds_with_valid_keys_and_no_relays() {
    // Arrange
    let keys = voter::identity::generate_keypair();
    let config = offline_config(vec![], None);

    // Act
    let result = NostrVoterClient::connect(&keys, &config).await;

    // Assert
    assert!(result.is_ok());
}

#[tokio::test]
async fn connect_fails_for_invalid_relay_url() {
    // Arrange
    let keys = voter::identity::generate_keypair();
    let config = offline_config(vec!["definitely not a relay url".to_string()], None);

    // Act
    let result = NostrVoterClient::connect(&keys, &config).await;

    // Assert
    assert!(result.is_err());
}

#[tokio::test]
async fn process_commands_reports_request_failed_for_unparseable_ec_pubkey() {
    // Arrange
    let keys = voter::identity::generate_keypair();
    let config = offline_config(vec![], None);
    let client = NostrVoterClient::connect(&keys, &config).await.unwrap();

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
    let (action_tx, mut action_rx) = mpsc::unbounded_channel();

    let msg = VoterMessage::Register {
        election_id: "election-1".to_string(),
        registration_token: "reg-token".to_string(),
        request_id: "req-1".to_string(),
    };
    cmd_tx
        .send(VoterCommand::Send {
            task_id: 42,
            ec_pubkey: "not-a-key".to_string(),
            msg,
        })
        .unwrap();
    // Close the command channel so process_commands returns after draining.
    drop(cmd_tx);

    // Act
    client
        .process_commands(&mut cmd_rx, action_tx, &config)
        .await;

    // Assert
    let action = action_rx.recv().await.expect("an action must be emitted");
    match action {
        NostrAction::RequestFailed(task_id, reason) => {
            assert_eq!(task_id, 42);
            assert!(reason.contains("invalid EC pubkey"), "reason: {reason}");
        }
        other => panic!("expected RequestFailed, got {other:?}"),
    }
}
