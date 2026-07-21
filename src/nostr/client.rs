use nostr_sdk::prelude::*;
use tokio::sync::mpsc;
use tracing::warn;

use crate::config::AppConfig;
use crate::error::{Result, VoterError};
use crate::nostr::events::{Election, ElectionResults};
use crate::nostr::messages::{EcResponse, VoterMessage};

/// How long to wait for an EC reply before reporting a timeout.
pub const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Actions produced by the Nostr client for the app event loop.
#[derive(Debug, Clone)]
pub enum NostrAction {
    ElectionUpdate(Election),
    ElectionResult(ElectionResults),
    EcResponse(EcResponse),
    ConnectionStatus(bool),
    Error(String),
    /// The request with this id received no EC response in time.
    RequestTimeout(u64),
    /// The request with this id could not be sent or failed outright.
    RequestFailed(u64, String),
}

/// Commands sent from the app to the Nostr task.
#[derive(Debug, Clone)]
pub enum VoterCommand {
    /// Gift-wrap a message to the EC from the voter's persistent identity
    /// (register, request-token). The reply arrives via the main listener.
    Send {
        task_id: u64,
        ec_pubkey: String,
        msg: VoterMessage,
    },
    /// Gift-wrap a message to the EC from a fresh throwaway keypair
    /// (cast-vote) and wait for the EC's reply on that keypair.
    SendAnonymous {
        task_id: u64,
        ec_pubkey: String,
        msg: VoterMessage,
    },
}

/// Whether an EC response echoes the given request correlation id.
///
/// Responses that do not echo the id (including legacy responses without
/// one) must never terminate a wait for a specific request: the app rejects
/// them as uncorrelated, so accepting them here would silently disarm the
/// timeout and leave the request pending forever.
pub fn response_echoes_request_id(response: &EcResponse, request_id: &str) -> bool {
    let echoed = match response {
        EcResponse::Ok { request_id, .. } | EcResponse::Error { request_id, .. } => request_id,
    };
    echoed.as_deref() == Some(request_id)
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

        // limit(0): live events only. Historical Gift Wraps are replayed on
        // every (re)subscribe, predate any in-flight request by definition,
        // and must never reach the response-correlation path.
        let gift_wrap_filter = Filter::new().kind(Kind::GiftWrap).limit(0).pubkey(
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

    /// Send a voter message to the EC via NIP-59 Gift Wrap using the voter's
    /// persistent identity. The EC's reply arrives through the main listener.
    pub async fn send_to_ec(&self, ec_pubkey: &PublicKey, msg: &VoterMessage) -> Result<()> {
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
            .gift_wrap(ec_pubkey, rumor, Vec::<Tag>::new())
            .await
            .map_err(|e| VoterError::Nostr(format!("gift_wrap send failed: {e}")))?;

        Ok(())
    }

    /// Send a voter message to the EC from a fresh throwaway keypair and wait
    /// for the EC's Gift Wrap reply addressed to that keypair.
    ///
    /// Used for cast-vote: the ballot must never be linkable to the voter's
    /// persistent identity, and the EC replies to whichever key sent the
    /// message, so the reply has to be awaited on the throwaway key.
    pub async fn send_anonymous_and_wait(
        &self,
        ec_pubkey: &PublicKey,
        msg: &VoterMessage,
        config: &AppConfig,
    ) -> Result<EcResponse> {
        let throwaway_keys = Keys::generate();
        let anon_client = Client::new(throwaway_keys.clone());
        for relay_url in &config.nostr.relays {
            anon_client
                .add_relay(relay_url)
                .await
                .map_err(|e| VoterError::Nostr(format!("failed to add relay {relay_url}: {e}")))?;
        }
        anon_client.connect().await;

        let result = Self::anonymous_roundtrip(&anon_client, &throwaway_keys, ec_pubkey, msg).await;

        anon_client.disconnect().await;
        result
    }

    async fn anonymous_roundtrip(
        anon_client: &Client,
        throwaway_keys: &Keys,
        ec_pubkey: &PublicKey,
        msg: &VoterMessage,
    ) -> Result<EcResponse> {
        // Subscribe for the reply before sending so it cannot be missed.
        let reply_filter = Filter::new()
            .kind(Kind::GiftWrap)
            .pubkey(throwaway_keys.public_key())
            .limit(0);
        anon_client
            .subscribe(reply_filter, None)
            .await
            .map_err(|e| VoterError::Nostr(format!("subscribe reply failed: {e}")))?;

        let mut notifications = anon_client.notifications();

        let content = serde_json::to_string(msg)?;
        let rumor = EventBuilder::text_note(content).build(throwaway_keys.public_key());
        anon_client
            .gift_wrap(ec_pubkey, rumor, Vec::<Tag>::new())
            .await
            .map_err(|e| VoterError::Nostr(format!("anonymous gift_wrap failed: {e}")))?;

        let wait = async {
            loop {
                let notification = notifications
                    .recv()
                    .await
                    .map_err(|e| VoterError::Nostr(format!("notification stream closed: {e}")))?;
                let RelayPoolNotification::Event { event, .. } = notification else {
                    continue;
                };
                if event.kind != Kind::GiftWrap {
                    continue;
                }
                let Ok(unwrapped) = anon_client.unwrap_gift_wrap(&event).await else {
                    continue;
                };
                // Only the EC's reply counts.
                if unwrapped.sender != *ec_pubkey {
                    warn!(sender = %unwrapped.sender, "ignoring reply from untrusted sender");
                    continue;
                }
                match serde_json::from_str::<EcResponse>(unwrapped.rumor.content.as_str()) {
                    // Only the reply to THIS request may end the wait. A
                    // reply without the echoed id would be rejected by the
                    // app as uncorrelated; returning it here would disarm
                    // the timeout and strand the request forever.
                    Ok(response) => {
                        if response_echoes_request_id(&response, msg.request_id()) {
                            return Ok(response);
                        }
                        warn!("ignoring EC reply that does not echo the request id");
                    }
                    Err(e) => warn!(error = %e, "failed to parse EC reply"),
                }
            }
        };

        tokio::time::timeout(REQUEST_TIMEOUT, wait)
            .await
            .map_err(|_| VoterError::Nostr("EC did not respond in time".to_string()))?
    }

    /// Process app commands until the channel closes. Runs alongside
    /// [`listen`](Self::listen); one command is handled at a time (the app
    /// enforces a single in-flight request).
    pub async fn process_commands(
        &self,
        cmd_rx: &mut mpsc::UnboundedReceiver<VoterCommand>,
        action_tx: mpsc::UnboundedSender<NostrAction>,
        config: &AppConfig,
    ) {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                VoterCommand::Send {
                    task_id,
                    ec_pubkey,
                    msg,
                } => {
                    let ec_pk = match PublicKey::parse(&ec_pubkey) {
                        Ok(pk) => pk,
                        Err(e) => {
                            let _ = action_tx.send(NostrAction::RequestFailed(
                                task_id,
                                format!("invalid EC pubkey: {e}"),
                            ));
                            continue;
                        }
                    };
                    match self.send_to_ec(&ec_pk, &msg).await {
                        Ok(()) => {
                            // The reply arrives via the main listener; arm a
                            // timeout so the app never hangs on a lost reply.
                            let tx = action_tx.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep(REQUEST_TIMEOUT).await;
                                let _ = tx.send(NostrAction::RequestTimeout(task_id));
                            });
                        }
                        Err(e) => {
                            let _ =
                                action_tx.send(NostrAction::RequestFailed(task_id, e.to_string()));
                        }
                    }
                }
                VoterCommand::SendAnonymous {
                    task_id,
                    ec_pubkey,
                    msg,
                } => {
                    let ec_pk = match PublicKey::parse(&ec_pubkey) {
                        Ok(pk) => pk,
                        Err(e) => {
                            let _ = action_tx.send(NostrAction::RequestFailed(
                                task_id,
                                format!("invalid EC pubkey: {e}"),
                            ));
                            continue;
                        }
                    };
                    // Watchdog: this future is dropped if the main listener
                    // disconnects (tokio::select in the caller), which would
                    // otherwise leave the app waiting forever — the internal
                    // timeout dies with the future. The spawned task survives
                    // the drop; a stale timeout is ignored by the app.
                    let watchdog_tx = action_tx.clone();
                    let watchdog = tokio::spawn(async move {
                        tokio::time::sleep(REQUEST_TIMEOUT + std::time::Duration::from_secs(5))
                            .await;
                        let _ = watchdog_tx.send(NostrAction::RequestTimeout(task_id));
                    });
                    match self.send_anonymous_and_wait(&ec_pk, &msg, config).await {
                        Ok(response) => {
                            let _ = action_tx.send(NostrAction::EcResponse(response));
                        }
                        Err(e) => {
                            let _ =
                                action_tx.send(NostrAction::RequestFailed(task_id, e.to_string()));
                        }
                    }
                    watchdog.abort();
                }
            }
        }
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
                                        // Version stamp: announcements are
                                        // replaceable, so an older replay must
                                        // not overwrite what we already have.
                                        election.event_created_at =
                                            Some(event.created_at.as_secs());
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
