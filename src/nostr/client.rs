use nostr_sdk::prelude::*;
use tokio::sync::mpsc;
use tracing::warn;

use crate::config::AppConfig;
use crate::error::{Result, VoterError};
use crate::nostr::events::{Election, ElectionResults};
use crate::nostr::messages::EcResponse;
#[allow(unused_imports)]
use crate::nostr::messages::VoterMessage;

/// Actions produced by the Nostr client for the app event loop.
#[derive(Debug, Clone)]
pub enum NostrAction {
    ElectionUpdate(Election),
    ElectionResult(ElectionResults),
    EcResponse(EcResponse),
    ConnectionStatus(bool),
    Error(String),
}

/// Wraps the nostr-sdk Client for voter-specific operations.
pub struct NostrVoterClient {
    client: Client,
    ec_pubkey: Option<PublicKey>,
}

impl NostrVoterClient {
    /// Create and connect a new Nostr client using the given keys and config.
    ///
    /// When `nostr.ec_pubkey` is configured, it is pinned: only events signed
    /// by that key are trusted (see [`subscribe`](Self::subscribe) and
    /// [`listen`](Self::listen)).
    pub async fn connect(keys: &Keys, config: &AppConfig) -> Result<Self> {
        let ec_pubkey = match &config.nostr.ec_pubkey {
            Some(pk_str) => Some(
                PublicKey::parse(pk_str)
                    .map_err(|e| VoterError::Config(format!("invalid ec_pubkey in config: {e}")))?,
            ),
            None => {
                warn!(
                    "no ec_pubkey configured — accepting election events from ANY author. \
                     Set nostr.ec_pubkey in voter.toml to pin the Electoral Commission's key."
                );
                None
            }
        };

        let client = Client::new(keys.clone());

        for relay_url in &config.nostr.relays {
            client
                .add_relay(relay_url)
                .await
                .map_err(|e| VoterError::Nostr(format!("failed to add relay {relay_url}: {e}")))?;
        }

        client.connect().await;

        Ok(Self { client, ec_pubkey })
    }

    /// Set the EC's public key (needed to send Gift Wrap messages).
    #[allow(dead_code)]
    pub fn set_ec_pubkey(&mut self, pubkey: PublicKey) {
        self.ec_pubkey = Some(pubkey);
    }

    /// Subscribe to election announcements (Kind 35000), results (Kind 35001),
    /// and Gift Wrap messages addressed to us.
    ///
    /// With a pinned EC pubkey the election/result subscription is restricted
    /// to that author. The Gift Wrap subscription cannot be author-filtered
    /// (wraps are signed by ephemeral keys per NIP-59); the sender is verified
    /// after unwrapping instead.
    pub async fn subscribe(&self) -> Result<()> {
        let mut election_filter =
            Filter::new().kinds(vec![Kind::Custom(35_000), Kind::Custom(35_001)]);
        if let Some(ec_pk) = self.ec_pubkey {
            election_filter = election_filter.author(ec_pk);
        }

        let gift_wrap_filter = Filter::new().kind(Kind::GiftWrap).pubkey(
            self.client
                .signer()
                .await
                .map_err(|e| VoterError::Nostr(format!("no signer: {e}")))?
                .get_public_key()
                .await
                .map_err(|e| VoterError::Nostr(format!("no public key: {e}")))?,
        );

        self.client
            .subscribe(election_filter, None)
            .await
            .map_err(|e| VoterError::Nostr(format!("subscribe elections failed: {e}")))?;

        self.client
            .subscribe(gift_wrap_filter, None)
            .await
            .map_err(|e| VoterError::Nostr(format!("subscribe gift wrap failed: {e}")))?;

        Ok(())
    }

    /// Send a voter message to the EC via NIP-59 Gift Wrap.
    #[allow(dead_code)]
    pub async fn send_to_ec(&self, msg: &VoterMessage) -> Result<()> {
        let ec_pubkey = self
            .ec_pubkey
            .ok_or_else(|| VoterError::Nostr("EC public key not set".to_string()))?;

        let content = serde_json::to_string(msg)?;
        let my_pubkey = self
            .client
            .signer()
            .await
            .map_err(|e| VoterError::Nostr(format!("no signer: {e}")))?
            .get_public_key()
            .await
            .map_err(|e| VoterError::Nostr(format!("no public key: {e}")))?;
        let rumor = EventBuilder::text_note(content).build(my_pubkey);

        self.client
            .gift_wrap(&ec_pubkey, rumor, Vec::<Tag>::new())
            .await
            .map_err(|e| VoterError::Nostr(format!("gift_wrap send failed: {e}")))?;

        Ok(())
    }

    /// Send a voter message to the EC using a specific (throwaway) signer.
    #[allow(dead_code)]
    pub async fn send_to_ec_anonymous(
        &self,
        msg: &VoterMessage,
        throwaway_keys: &Keys,
        config: &AppConfig,
    ) -> Result<()> {
        let ec_pubkey = self
            .ec_pubkey
            .ok_or_else(|| VoterError::Nostr("EC public key not set".to_string()))?;

        // Create a temporary client with the throwaway keys
        let anon_client = Client::new(throwaway_keys.clone());
        for relay_url in &config.nostr.relays {
            anon_client
                .add_relay(relay_url)
                .await
                .map_err(|e| VoterError::Nostr(format!("failed to add relay {relay_url}: {e}")))?;
        }
        anon_client.connect().await;

        let content = serde_json::to_string(msg)?;
        let rumor = EventBuilder::text_note(content).build(throwaway_keys.public_key());

        let result = anon_client
            .gift_wrap(&ec_pubkey, rumor, Vec::<Tag>::new())
            .await;

        anon_client.disconnect().await;

        result.map_err(|e| VoterError::Nostr(format!("anonymous gift_wrap failed: {e}")))?;

        Ok(())
    }

    /// Start listening for Nostr events and forward them as NostrActions.
    /// This should be spawned as a tokio task.
    pub async fn listen(&self, action_tx: mpsc::UnboundedSender<NostrAction>) -> Result<()> {
        let client = self.client.clone();
        let tx = action_tx;
        let pinned_ec = self.ec_pubkey;

        client
            .handle_notifications(|notification| {
                let tx = tx.clone();
                let client = client.clone();
                async move {
                    if let RelayPoolNotification::Event { event, .. } = notification {
                        // Relay-side author filters are advisory; enforce the
                        // pinned EC key locally for election/result events.
                        if matches!(event.kind, Kind::Custom(35_000) | Kind::Custom(35_001))
                            && let Some(ec_pk) = pinned_ec
                            && event.pubkey != ec_pk
                        {
                            warn!(
                                author = %event.pubkey,
                                kind = %event.kind,
                                "ignoring election event from untrusted author"
                            );
                            return Ok(false);
                        }
                        match event.kind {
                            Kind::Custom(35_000) => {
                                match serde_json::from_str::<Election>(event.content.as_str()) {
                                    Ok(mut election) => {
                                        // Capture EC pubkey from the event author
                                        election.ec_pubkey = Some(event.pubkey.to_hex());
                                        let _ = tx.send(NostrAction::ElectionUpdate(election));
                                    }
                                    Err(e) => {
                                        warn!(error = %e, "failed to parse election event");
                                    }
                                }
                            }
                            Kind::Custom(35_001) => {
                                match serde_json::from_str::<ElectionResults>(
                                    event.content.as_str(),
                                ) {
                                    Ok(results) => {
                                        let _ = tx.send(NostrAction::ElectionResult(results));
                                    }
                                    Err(e) => {
                                        warn!(error = %e, "failed to parse results event");
                                    }
                                }
                            }
                            Kind::GiftWrap => {
                                match client.unwrap_gift_wrap(&event).await {
                                    Ok(unwrapped) => {
                                        // Only trust responses sealed by the EC.
                                        if let Some(ec_pk) = pinned_ec
                                            && unwrapped.sender != ec_pk
                                        {
                                            warn!(
                                                sender = %unwrapped.sender,
                                                "ignoring gift wrap from untrusted sender"
                                            );
                                            return Ok(false);
                                        }
                                        match serde_json::from_str::<EcResponse>(
                                            unwrapped.rumor.content.as_str(),
                                        ) {
                                            Ok(response) => {
                                                let _ =
                                                    tx.send(NostrAction::EcResponse(response));
                                            }
                                            Err(e) => {
                                                warn!(error = %e, "failed to parse EC response from gift wrap");
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!(error = %e, "failed to unwrap gift wrap");
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    Ok(false) // keep listening
                }
            })
            .await
            .map_err(|e| VoterError::Nostr(format!("notification handler error: {e}")))?;

        Ok(())
    }

    /// Disconnect from all relays.
    pub async fn disconnect(&self) {
        self.client.disconnect().await;
    }

    /// Get a reference to the underlying nostr-sdk Client.
    #[allow(dead_code)]
    pub fn inner(&self) -> &Client {
        &self.client
    }
}
