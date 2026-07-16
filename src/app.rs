use std::collections::HashMap;

use crossterm::event::KeyCode;
use nostr_sdk::prelude::Keys;
use tokio::sync::mpsc;

use voter::config::AppConfig;
use voter::crypto::token::{self, PendingBlind};
use voter::nostr::client::{NostrAction, VoterCommand};
use voter::nostr::events::{Election, ElectionResults, ElectionStatus};
use voter::nostr::messages::{EcResponse, VoterMessage};
use voter::state::AppState;

/// All possible actions flowing through the app event loop.
#[derive(Debug, Clone)]
pub enum Action {
    Quit,
    KeyPress(KeyCode),
    Resize,
    Nostr(NostrAction),
    IdentityCreated(String),
    IdentityUnlocked,
}

/// The screen the app is currently showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Welcome,
    PasswordPrompt,
    ElectionList,
    ElectionDetail { election_id: String },
    Vote { election_id: String },
    Results { election_id: String },
    Settings,
}

/// Whether the app should continue running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShouldQuit {
    Yes,
    No,
}

/// Which EC request is currently in flight (at most one at a time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingKind {
    Register,
    RequestToken,
    CastVote,
}

/// Tracks the single in-flight EC request so replies, timeouts, and failures
/// can be correlated (EC responses carry no election id).
pub struct PendingRequest {
    pub id: u64,
    pub election_id: String,
    pub kind: PendingKind,
    /// Wire-level correlation id sent inside the request; the EC echoes it in
    /// its reply. Random (never a counter) so a replayed response from an
    /// earlier session can never match a current request.
    pub request_id: String,
}

/// Fresh random correlation id for one EC request (16 bytes, hex-encoded).
fn fresh_request_id() -> Result<String, getrandom::Error> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)?;
    Ok(hex::encode(bytes))
}

/// Central application state.
pub struct App {
    pub screen: Screen,
    pub previous_screen: Option<Screen>,
    pub config: AppConfig,
    pub keys: Option<Keys>,
    pub persistent_state: AppState,
    pub elections: HashMap<String, Election>,
    pub results: HashMap<String, ElectionResults>,
    pub action_tx: mpsc::UnboundedSender<Action>,
    pub show_help: bool,
    pub status_message: Option<String>,
    pub error_message: Option<String>,
    // UI state for specific screens
    pub election_list_index: usize,
    pub candidate_list_index: usize,
    pub stv_ranking: Vec<u32>,
    pub token_input: String,
    pub password_input: String,
    pub is_loading: bool,
    pub loading_step: Option<String>,
    pub connected: bool,
    /// True while typing a registration token in the election detail screen.
    pub editing_token: bool,
    /// Vote confirmation dialog state: Some(confirm_button_focused).
    pub vote_confirm: Option<bool>,
    // Protocol state
    pub cmd_tx: Option<mpsc::UnboundedSender<VoterCommand>>,
    pub pending: Option<PendingRequest>,
    pub pending_blind: Option<PendingBlind>,
    /// Where persistent state is saved. Private so production code cannot
    /// re-point it; the in-file test module overrides it with a tempdir so
    /// tests never touch the user's real ~/.config/voter/state.json.
    state_path: std::path::PathBuf,
    next_task_id: u64,
}

impl App {
    pub fn new(
        config: AppConfig,
        persistent_state: AppState,
        action_tx: mpsc::UnboundedSender<Action>,
    ) -> Self {
        let state_path = config.state_path();
        Self {
            screen: Screen::Welcome,
            previous_screen: None,
            config,
            keys: None,
            persistent_state,
            elections: HashMap::new(),
            results: HashMap::new(),
            action_tx,
            show_help: false,
            status_message: None,
            error_message: None,
            election_list_index: 0,
            candidate_list_index: 0,
            stv_ranking: Vec::new(),
            token_input: String::new(),
            password_input: String::new(),
            is_loading: false,
            loading_step: None,
            connected: false,
            editing_token: false,
            vote_confirm: None,
            cmd_tx: None,
            pending: None,
            pending_blind: None,
            state_path,
            next_task_id: 0,
        }
    }

    /// Path where persistent state is saved.
    pub fn state_path(&self) -> &std::path::Path {
        &self.state_path
    }

    /// Process an action and return whether the app should quit.
    pub fn update(&mut self, action: Action) -> ShouldQuit {
        // Clear transient errors on user actions, but preserve connection errors
        if matches!(action, Action::KeyPress(_)) {
            self.error_message = None;
        }

        match action {
            Action::Quit => return ShouldQuit::Yes,
            Action::KeyPress(key) => self.handle_key(key),
            Action::Resize => {} // triggers redraw via main loop
            Action::Nostr(nostr_action) => self.handle_nostr(nostr_action),
            Action::IdentityCreated(pubkey) => {
                self.status_message = Some(format!("Identity created: {}", &pubkey[..16]));
                self.screen = Screen::ElectionList;
            }
            Action::IdentityUnlocked => {
                self.screen = Screen::ElectionList;
            }
        }

        ShouldQuit::No
    }

    fn handle_key(&mut self, key: KeyCode) {
        // Global keys (disabled while typing into an input field)
        match key {
            KeyCode::Char('?') if !self.is_input_mode() => {
                self.show_help = !self.show_help;
                return;
            }
            KeyCode::Char('q') if !self.is_input_mode() => {
                let _ = self.action_tx.send(Action::Quit);
                return;
            }
            _ => {}
        }

        if self.show_help {
            if key == KeyCode::Esc {
                self.show_help = false;
            }
            return;
        }

        match &self.screen {
            Screen::Welcome => self.handle_welcome_key(key),
            Screen::PasswordPrompt => self.handle_password_key(key),
            Screen::ElectionList => self.handle_election_list_key(key),
            Screen::ElectionDetail { .. } => self.handle_election_detail_key(key),
            Screen::Vote { .. } => self.handle_vote_key(key),
            Screen::Results { .. } => self.handle_results_key(key),
            Screen::Settings => self.handle_settings_key(key),
        }
    }

    fn handle_nostr(&mut self, action: NostrAction) {
        match action {
            NostrAction::ElectionUpdate(election) => {
                self.elections
                    .insert(election.election_id.clone(), election);
            }
            NostrAction::ElectionResult(results) => {
                self.results.insert(results.election_id.clone(), results);
            }
            NostrAction::EcResponse(response) => {
                self.handle_ec_response(&response);
            }
            NostrAction::RequestTimeout(id) => {
                if self.pending.as_ref().is_some_and(|p| p.id == id) {
                    self.pending = None;
                    self.pending_blind = None;
                    self.is_loading = false;
                    self.loading_step = None;
                    self.error_message = Some("EC did not respond in time".to_string());
                }
            }
            NostrAction::RequestFailed(id, msg) => {
                if self.pending.as_ref().is_some_and(|p| p.id == id) {
                    self.pending = None;
                    self.pending_blind = None;
                    self.is_loading = false;
                    self.loading_step = None;
                    self.error_message = Some(format!("Request failed: {msg}"));
                }
            }
            NostrAction::ConnectionStatus(connected) => {
                self.connected = connected;
                if connected {
                    self.error_message = None;
                    self.status_message = Some("Connected to relays".to_string());
                } else {
                    self.status_message = None;
                    self.error_message = Some("Disconnected from relays".to_string());
                }
            }
            NostrAction::Error(msg) => {
                self.error_message = Some(msg);
            }
        }
    }

    fn handle_welcome_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('1') | KeyCode::Char('g') => {
                let path = self.config.identity.path.clone();
                // Reaching Welcome with an identity on disk means loading it
                // failed; overwriting would destroy the registered voter key
                // and every registration bound to it.
                if voter::identity::identity_exists(&path) {
                    self.error_message = Some(format!(
                        "An identity already exists at {} but could not be loaded. \
                         Refusing to overwrite it — fix or move the file, then restart.",
                        path.display()
                    ));
                    return;
                }
                let keys = voter::identity::generate_keypair();
                match voter::identity::save_identity(&keys, None, &path) {
                    Ok(()) => {
                        let pubkey = voter::identity::export_public_key(&keys);
                        self.keys = Some(keys);
                        let _ = self.action_tx.send(Action::IdentityCreated(pubkey));
                    }
                    Err(e) => {
                        self.error_message = Some(format!("Failed to save identity: {e}"));
                    }
                }
            }
            KeyCode::Char('2') | KeyCode::Char('i') => {
                // Import identity — TODO
            }
            _ => {}
        }
    }

    fn handle_password_key(&mut self, key: KeyCode) {
        match key {
            // The only exit hatch: `q` is disabled while typing (it would be
            // part of the password) and Ctrl+C arrives as a plain 'c' — a
            // user who cannot supply the password must still be able to quit.
            KeyCode::Esc => {
                let _ = self.action_tx.send(Action::Quit);
            }
            KeyCode::Enter => {
                let path = self.config.identity.path.clone();
                let password = self.password_input.clone();
                match voter::identity::load_identity(Some(&password), &path) {
                    Ok(keys) => {
                        self.keys = Some(keys);
                        self.password_input.clear();
                        let _ = self.action_tx.send(Action::IdentityUnlocked);
                    }
                    Err(e) => {
                        self.error_message = Some(format!("Unlock failed: {e}"));
                        self.password_input.clear();
                    }
                }
            }
            KeyCode::Char(c) => {
                self.password_input.push(c);
            }
            KeyCode::Backspace => {
                self.password_input.pop();
            }
            _ => {}
        }
    }

    fn handle_election_list_key(&mut self, key: KeyCode) {
        let election_count = self.elections.len();
        match key {
            KeyCode::Char('j') | KeyCode::Down => {
                if election_count > 0 {
                    self.election_list_index =
                        (self.election_list_index + 1).min(election_count - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.election_list_index = self.election_list_index.saturating_sub(1);
            }
            KeyCode::Enter => {
                if let Some(election_id) = self.sorted_election_ids().get(self.election_list_index)
                {
                    let eid = election_id.clone();
                    self.screen = Screen::ElectionDetail { election_id: eid };
                }
            }
            KeyCode::Char('s') => {
                self.previous_screen = Some(self.screen.clone());
                self.screen = Screen::Settings;
            }
            _ => {}
        }
    }

    fn handle_election_detail_key(&mut self, key: KeyCode) {
        let Screen::ElectionDetail { ref election_id } = self.screen else {
            return;
        };
        let eid = election_id.clone();

        // Registration token input mode
        if self.editing_token {
            match key {
                KeyCode::Esc => {
                    self.editing_token = false;
                    self.token_input.clear();
                }
                KeyCode::Enter => {
                    // Trim first: a whitespace-only input would be trimmed to
                    // "" by submit_registration and sent to the EC anyway — a
                    // guaranteed error (or 30 s timeout) instead of a no-op.
                    if !self.token_input.trim().is_empty() {
                        self.submit_registration(&eid);
                    }
                }
                KeyCode::Char(c) => {
                    self.token_input.push(c);
                }
                KeyCode::Backspace => {
                    self.token_input.pop();
                }
                _ => {}
            }
            return;
        }

        match key {
            KeyCode::Esc => {
                self.token_input.clear();
                self.screen = Screen::ElectionList;
            }
            KeyCode::Enter => {
                // Start typing a registration token
                let can_register = !self.persistent_state.is_registered(&eid)
                    && self
                        .elections
                        .get(&eid)
                        .is_some_and(|e| e.status == ElectionStatus::Open);
                if can_register && self.pending.is_none() {
                    self.editing_token = true;
                    self.token_input.clear();
                }
            }
            KeyCode::Char('t') => {
                let can_request = self.persistent_state.is_registered(&eid)
                    && self.persistent_state.get_active_token(&eid).is_none()
                    && !self.persistent_state.has_voted(&eid)
                    && self
                        .elections
                        .get(&eid)
                        .is_some_and(|e| e.status == ElectionStatus::InProgress);
                if can_request && self.pending.is_none() {
                    self.request_voting_token(&eid);
                }
            }
            KeyCode::Char('r') => {
                if self.results.contains_key(&eid) {
                    self.screen = Screen::Results { election_id: eid };
                }
            }
            KeyCode::Char('v') => {
                let can_vote = self.persistent_state.get_active_token(&eid).is_some()
                    && self
                        .elections
                        .get(&eid)
                        .is_some_and(|e| e.status == ElectionStatus::InProgress);
                if can_vote {
                    self.candidate_list_index = 0;
                    self.stv_ranking.clear();
                    self.vote_confirm = None;
                    self.screen = Screen::Vote { election_id: eid };
                }
            }
            _ => {}
        }
    }

    fn handle_vote_key(&mut self, key: KeyCode) {
        if let Screen::Vote { ref election_id } = self.screen {
            let eid_owned = election_id.clone();
            let election = self.elections.get(election_id);
            let candidate_count = election.map(|e| e.candidates.len()).unwrap_or(0);
            let is_stv = election.map(|e| e.rules_id == "stv").unwrap_or(false);

            // Confirmation dialog is modal
            if let Some(confirm_focused) = self.vote_confirm {
                match key {
                    KeyCode::Esc => self.vote_confirm = None,
                    KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                        self.vote_confirm = Some(!confirm_focused);
                    }
                    KeyCode::Enter => {
                        self.vote_confirm = None;
                        if confirm_focused {
                            self.submit_vote(&eid_owned);
                        }
                    }
                    _ => {}
                }
                return;
            }

            match key {
                KeyCode::Esc => {
                    let eid = election_id.clone();
                    self.screen = Screen::ElectionDetail { election_id: eid };
                }
                KeyCode::Char('s') => {
                    if !self.stv_ranking.is_empty() && self.pending.is_none() {
                        self.vote_confirm = Some(false);
                    }
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    if candidate_count > 0 {
                        self.candidate_list_index =
                            (self.candidate_list_index + 1).min(candidate_count - 1);
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.candidate_list_index = self.candidate_list_index.saturating_sub(1);
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if let Some(candidate) =
                        election.and_then(|e| e.candidates.get(self.candidate_list_index))
                    {
                        if is_stv {
                            if !self.stv_ranking.contains(&candidate.id) {
                                self.stv_ranking.push(candidate.id);
                            }
                        } else {
                            self.stv_ranking = vec![candidate.id];
                        }
                    }
                }
                KeyCode::Char('d') if is_stv => {
                    if let Some(candidate) =
                        election.and_then(|e| e.candidates.get(self.candidate_list_index))
                    {
                        self.stv_ranking.retain(|&id| id != candidate.id);
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_results_key(&mut self, key: KeyCode) {
        if let Screen::Results { ref election_id } = self.screen
            && key == KeyCode::Esc
        {
            let eid = election_id.clone();
            self.screen = Screen::ElectionDetail { election_id: eid };
        }
    }

    fn handle_settings_key(&mut self, key: KeyCode) {
        if key == KeyCode::Esc {
            self.go_back();
        }
    }

    fn go_back(&mut self) {
        if let Some(prev) = self.previous_screen.take() {
            self.screen = prev;
        } else {
            self.screen = Screen::ElectionList;
        }
    }

    fn is_input_mode(&self) -> bool {
        matches!(self.screen, Screen::PasswordPrompt) || self.editing_token
    }

    // --- EC protocol actions -------------------------------------------------

    /// The EC pubkey to address: the configured pin wins; otherwise the
    /// (verified) author of the election announcement.
    fn resolve_ec_pubkey(&self, election_id: &str) -> Option<String> {
        self.config
            .nostr
            .ec_pubkey
            .clone()
            .or_else(|| self.elections.get(election_id)?.ec_pubkey.clone())
    }

    fn send_command(
        &mut self,
        election_id: &str,
        kind: PendingKind,
        msg: VoterMessage,
        anonymous: bool,
        loading_step: &str,
    ) {
        let Some(ec_pubkey) = self.resolve_ec_pubkey(election_id) else {
            self.error_message = Some("EC public key unknown for this election".to_string());
            return;
        };
        let Some(cmd_tx) = self.cmd_tx.as_ref() else {
            self.error_message = Some("Not connected to relays".to_string());
            return;
        };

        self.next_task_id += 1;
        let task_id = self.next_task_id;
        let request_id = msg.request_id().to_string();
        let cmd = if anonymous {
            VoterCommand::SendAnonymous {
                task_id,
                ec_pubkey,
                msg,
            }
        } else {
            VoterCommand::Send {
                task_id,
                ec_pubkey,
                msg,
            }
        };

        if cmd_tx.send(cmd).is_err() {
            self.error_message = Some("Connection task is not running".to_string());
            return;
        }

        self.pending = Some(PendingRequest {
            id: task_id,
            election_id: election_id.to_string(),
            kind,
            request_id,
        });
        self.is_loading = true;
        self.loading_step = Some(loading_step.to_string());
        self.error_message = None;
    }

    fn submit_registration(&mut self, election_id: &str) {
        let registration_token = self.token_input.trim().to_string();
        self.editing_token = false;
        self.token_input.clear();
        let Ok(request_id) = fresh_request_id() else {
            self.error_message = Some("System RNG unavailable".to_string());
            return;
        };
        let msg = VoterMessage::Register {
            election_id: election_id.to_string(),
            registration_token,
            request_id,
        };
        self.send_command(
            election_id,
            PendingKind::Register,
            msg,
            false,
            "Registering with EC…",
        );
    }

    fn request_voting_token(&mut self, election_id: &str) {
        let Some(election) = self.elections.get(election_id) else {
            return;
        };
        // Blind a fresh nonce with the election's RSA key; keep the blinding
        // secret locally until the EC's blind signature arrives.
        match token::begin_token_request(election_id, &election.rsa_pub_key) {
            Ok((pending_blind, blinded_nonce)) => {
                let Ok(request_id) = fresh_request_id() else {
                    self.error_message = Some("System RNG unavailable".to_string());
                    return;
                };
                let msg = VoterMessage::RequestToken {
                    election_id: election_id.to_string(),
                    blinded_nonce,
                    request_id,
                };
                self.pending_blind = Some(pending_blind);
                self.send_command(
                    election_id,
                    PendingKind::RequestToken,
                    msg,
                    false,
                    "Requesting blind-signed voting token…",
                );
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to prepare token request: {e}"));
            }
        }
    }

    fn submit_vote(&mut self, election_id: &str) {
        let Some(voting_token) = self.persistent_state.get_active_token(election_id) else {
            self.error_message = Some("No voting token for this election".to_string());
            return;
        };
        let wire_token = match voting_token.wire_token() {
            Ok(t) => t,
            Err(e) => {
                self.error_message = Some(format!("Stored token is corrupt: {e}"));
                return;
            }
        };
        let Ok(request_id) = fresh_request_id() else {
            self.error_message = Some("System RNG unavailable".to_string());
            return;
        };
        let msg = VoterMessage::CastVote {
            election_id: election_id.to_string(),
            candidate_ids: self.stv_ranking.clone(),
            h_n: voting_token.h_n.clone(),
            token: wire_token,
            request_id,
        };
        // Cast anonymously: the ballot must never be linkable to the voter's
        // persistent identity.
        self.send_command(
            election_id,
            PendingKind::CastVote,
            msg,
            true,
            "Casting vote anonymously…",
        );
    }

    fn handle_ec_response(&mut self, response: &EcResponse) {
        // Only a response correlated with the in-flight request may consume
        // the pending state. Relays can replay historical Gift Wraps on
        // reconnect; letting a stale reply swallow a pending token request
        // would drop the real blind signature when it arrives — and the EC
        // has already burned this voter's only token slot.
        //
        // Correlation is strict: every request is sent with a request_id, so
        // only a response echoing that exact id may consume the pending
        // state. Responses without an echo (pre-correlation EC, or replays of
        // pre-upgrade Gift Wraps) are displayed but never trusted; against an
        // EC that cannot echo ids, requests fail via the send timeout.
        let expected_action = |kind: PendingKind| match kind {
            PendingKind::Register => "register-confirmed",
            PendingKind::RequestToken => "token-issued",
            PendingKind::CastVote => "vote-recorded",
        };
        let id_matches = |pending: &PendingRequest, echoed: &Option<String>| {
            echoed.as_deref() == Some(pending.request_id.as_str())
        };

        match response {
            EcResponse::Ok {
                action,
                blind_signature,
                request_id,
            } => {
                let pending = self.pending.take_if(|p| {
                    expected_action(p.kind) == action.as_str() && id_matches(p, request_id)
                });
                if pending.is_some() {
                    self.is_loading = false;
                    self.loading_step = None;
                }
                match (action.as_str(), pending) {
                    ("register-confirmed", Some(p)) => {
                        self.persistent_state.mark_registered(p.election_id);
                        self.save_state();
                        self.status_message = Some("Registered ✓".to_string());
                    }
                    ("token-issued", Some(p)) => {
                        // Check the signature before consuming the blinding
                        // secret: taking it on a malformed reply would lose
                        // the only material that can unblind a retry.
                        let Some(sig_b64) = blind_signature else {
                            self.error_message =
                                Some("EC response missing blind signature".to_string());
                            return;
                        };
                        let Some(pending_blind) = self.pending_blind.take() else {
                            self.error_message =
                                Some("Received a token with no pending request".to_string());
                            return;
                        };
                        match token::complete_token_request(pending_blind, sig_b64) {
                            Ok(voting_token) => {
                                self.persistent_state
                                    .store_token(p.election_id, voting_token);
                                self.save_state();
                                self.status_message =
                                    Some("Voting token received and verified ✓".to_string());
                            }
                            Err(e) => {
                                self.error_message =
                                    Some(format!("Token verification failed: {e}"));
                            }
                        }
                    }
                    ("vote-recorded", Some(p)) => {
                        if let Err(e) = self.persistent_state.consume_token(&p.election_id) {
                            self.error_message = Some(format!("Vote recorded, state error: {e}"));
                        } else {
                            self.save_state();
                            self.status_message = Some("Vote recorded ✓".to_string());
                        }
                        self.stv_ranking.clear();
                        self.screen = Screen::ElectionDetail {
                            election_id: p.election_id,
                        };
                    }
                    // Unsolicited or replayed response: display it, but leave
                    // any in-flight request untouched.
                    _ => {
                        self.status_message = Some(format_ec_response(response));
                    }
                }
            }
            EcResponse::Error { request_id, .. } => {
                if self.pending.is_none() {
                    // Nothing in flight; just surface the error.
                    self.error_message = Some(format_ec_response(response));
                    return;
                }
                if self
                    .pending
                    .take_if(|p| id_matches(p, request_id))
                    .is_some()
                {
                    self.is_loading = false;
                    self.loading_step = None;
                    self.pending_blind = None;
                    self.error_message = Some(format_ec_response(response));
                } else {
                    // Error echoing a different request id: a replayed Gift
                    // Wrap from an earlier request. Show it, but leave the
                    // in-flight request (and its blinding secret) untouched.
                    self.status_message = Some(format_ec_response(response));
                }
            }
        }
    }

    fn save_state(&mut self) {
        let path = self.state_path.clone();
        if let Err(e) = self.persistent_state.save(&path) {
            self.error_message = Some(format!("Failed to save state: {e}"));
        }
    }

    /// Returns election IDs sorted by name.
    pub fn sorted_election_ids(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.elections.keys().cloned().collect();
        ids.sort_by(|a, b| {
            let name_a = self.elections.get(a).map(|e| e.name.as_str()).unwrap_or("");
            let name_b = self.elections.get(b).map(|e| e.name.as_str()).unwrap_or("");
            name_a.cmp(name_b)
        });
        ids
    }
}

fn format_ec_response(response: &EcResponse) -> String {
    match response {
        EcResponse::Ok {
            action,
            blind_signature,
            ..
        } => {
            if blind_signature.is_some() {
                format!("EC: {action} (signature received)")
            } else {
                format!("EC: {action}")
            }
        }
        EcResponse::Error { code, message, .. } => {
            format!("EC error: {code} — {message}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use base64::prelude::*;
    use tempfile::TempDir;

    use voter::config::IdentityConfig;
    use voter::crypto::blind_rsa;
    use voter::nostr::events::Candidate;

    fn test_app() -> App {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(AppConfig::default(), AppState::default(), tx);
        // Redirect persistence into a tempdir: handlers that succeed call
        // save_state(), which must never touch the user's real config dir.
        // The tempdir handle is intentionally leaked so the path stays valid
        // for the whole test; the OS reclaims it with the temp filesystem.
        let dir = tempfile::tempdir().expect("create tempdir");
        app.state_path = dir.path().join("state.json");
        std::mem::forget(dir);
        app
    }

    /// An App fully isolated inside a tempdir: both the persistent state path
    /// and the identity path point into the tempdir, so no test can ever read
    /// or write the user's real ~/.config/voter files.
    fn isolated_app() -> (App, mpsc::UnboundedReceiver<Action>, TempDir) {
        let dir = tempfile::tempdir().expect("create tempdir");
        let (tx, rx) = mpsc::unbounded_channel();
        let config = AppConfig {
            identity: IdentityConfig {
                path: dir.path().join("identity.json"),
            },
            ..AppConfig::default()
        };
        let mut app = App::new(config, AppState::default(), tx);
        app.state_path = dir.path().join("state.json");
        (app, rx, dir)
    }

    fn press(app: &mut App, key: KeyCode) {
        app.update(Action::KeyPress(key));
    }

    fn attach_cmd_channel(app: &mut App) -> mpsc::UnboundedReceiver<VoterCommand> {
        let (tx, rx) = mpsc::unbounded_channel();
        app.cmd_tx = Some(tx);
        rx
    }

    fn plurality_election(id: &str, name: &str, status: ElectionStatus) -> Election {
        Election {
            election_id: id.to_string(),
            name: name.to_string(),
            start_time: 1_700_000_000,
            end_time: 1_700_086_400,
            status,
            rules_id: "plurality".to_string(),
            rsa_pub_key: "unused".to_string(),
            candidates: vec![
                Candidate {
                    id: 1,
                    name: "Alice".to_string(),
                },
                Candidate {
                    id: 2,
                    name: "Bob".to_string(),
                },
            ],
            ec_pubkey: Some("ec-pubkey-hex".to_string()),
        }
    }

    fn stv_election(id: &str, name: &str) -> Election {
        let base = plurality_election(id, name, ElectionStatus::InProgress);
        Election {
            rules_id: "stv".to_string(),
            candidates: vec![
                Candidate {
                    id: 1,
                    name: "Alice".to_string(),
                },
                Candidate {
                    id: 2,
                    name: "Bob".to_string(),
                },
                Candidate {
                    id: 3,
                    name: "Carol".to_string(),
                },
            ],
            ..base
        }
    }

    /// An election whose rsa_pub_key is a real base64-DER key, plus the
    /// matching secret key so a test can play the EC's blind-signer role
    /// (mirrors tests/token_flow.rs).
    fn rsa_election(id: &str, status: ElectionStatus) -> (Election, blind_rsa::BrsaSk) {
        let (pk, sk) = blind_rsa::generate_test_keypair();
        let pk_b64 = BASE64_STANDARD.encode(pk.to_der().expect("pk to der"));
        let election = Election {
            rsa_pub_key: pk_b64,
            ..plurality_election(id, "RSA Election", status)
        };
        (election, sk)
    }

    /// A syntactically valid stored voting token (all base64 fields decode).
    fn stored_token(consumed: bool) -> token::VotingToken {
        token::VotingToken {
            nonce_b64: BASE64_STANDARD.encode([7u8; 32]),
            h_n: hex::encode([9u8; 32]),
            signature_b64: BASE64_STANDARD.encode([1u8; 16]),
            randomizer_b64: None,
            consumed,
        }
    }

    fn set_pending(app: &mut App, kind: PendingKind) -> u64 {
        app.pending = Some(PendingRequest {
            id: 42,
            election_id: "e1".to_string(),
            kind,
            request_id: "req-42".to_string(),
        });
        app.is_loading = true;
        42
    }

    /// A replayed/unsolicited Ok response whose action does not match the
    /// in-flight request must NOT consume the pending state: swallowing a
    /// pending token request would drop the real blind signature when it
    /// arrives, permanently burning the voter's token slot.
    #[test]
    fn mismatched_ok_response_does_not_consume_pending() {
        let mut app = test_app();
        set_pending(&mut app, PendingKind::RequestToken);

        app.handle_ec_response(&EcResponse::Ok {
            action: "register-confirmed".to_string(),
            blind_signature: None,
            request_id: None,
        });

        assert!(app.pending.is_some(), "pending must survive a stale reply");
        assert!(app.is_loading, "still waiting for the real reply");
    }

    /// A replayed Ok whose action matches but whose request_id echoes an
    /// older request must not consume the pending state either — action
    /// matching alone cannot tell two requests of the same kind apart.
    #[test]
    fn ok_with_stale_request_id_does_not_consume_pending() {
        let mut app = test_app();
        set_pending(&mut app, PendingKind::Register);

        app.handle_ec_response(&EcResponse::Ok {
            action: "register-confirmed".to_string(),
            blind_signature: None,
            request_id: Some("req-OLD".to_string()),
        });

        assert!(app.pending.is_some(), "pending must survive a stale reply");
        assert!(app.is_loading);
    }

    /// The real reply — matching action and echoed request_id — consumes the
    /// pending state.
    #[test]
    fn ok_with_matching_request_id_consumes_pending() {
        let mut app = test_app();
        set_pending(&mut app, PendingKind::Register);

        app.handle_ec_response(&EcResponse::Ok {
            action: "register-confirmed".to_string(),
            blind_signature: None,
            request_id: Some("req-42".to_string()),
        });

        assert!(app.pending.is_none());
        assert!(!app.is_loading);
        assert!(app.status_message.is_some());
    }

    /// An error without a request_id echo cannot be correlated — it is a
    /// replayed pre-upgrade Gift Wrap or a pre-correlation EC. Either way it
    /// must not abort the in-flight request; against an EC that cannot echo
    /// ids the request fails via the send timeout instead.
    #[test]
    fn error_without_request_id_does_not_clear_pending() {
        let mut app = test_app();
        set_pending(&mut app, PendingKind::Register);

        app.handle_ec_response(&EcResponse::Error {
            code: voter::nostr::messages::EcErrorCode::InvalidToken,
            message: "bad token".to_string(),
            request_id: None,
        });

        assert!(app.pending.is_some(), "uncorrelated errors must be ignored");
        assert!(app.is_loading);
        assert!(app.error_message.is_none());
    }

    /// Same strictness for Ok: a matching action without the request_id echo
    /// (e.g. a replayed pre-upgrade "token-issued") must not consume pending
    /// state — action matching alone cannot tell two requests apart.
    #[test]
    fn ok_without_request_id_does_not_consume_pending() {
        let mut app = test_app();
        set_pending(&mut app, PendingKind::Register);

        app.handle_ec_response(&EcResponse::Ok {
            action: "register-confirmed".to_string(),
            blind_signature: None,
            request_id: None,
        });

        assert!(app.pending.is_some());
        assert!(app.is_loading);
    }

    /// A replayed error echoing an older request_id must not abort the
    /// in-flight request: a stale error aborting a newer token request would
    /// drop the blinding secret needed for the real blind signature.
    #[test]
    fn error_with_stale_request_id_leaves_pending_untouched() {
        let mut app = test_app();
        set_pending(&mut app, PendingKind::RequestToken);

        app.handle_ec_response(&EcResponse::Error {
            code: voter::nostr::messages::EcErrorCode::InternalError,
            message: "boom".to_string(),
            request_id: Some("req-OLD".to_string()),
        });

        assert!(app.pending.is_some(), "pending must survive a stale error");
        assert!(app.is_loading);
        assert!(
            app.error_message.is_none(),
            "stale errors surface as status, not as the request's failure"
        );
    }

    /// An error echoing the in-flight request_id fails that request.
    #[test]
    fn error_with_matching_request_id_clears_pending() {
        let mut app = test_app();
        set_pending(&mut app, PendingKind::Register);

        app.handle_ec_response(&EcResponse::Error {
            code: voter::nostr::messages::EcErrorCode::InvalidToken,
            message: "bad token".to_string(),
            request_id: Some("req-42".to_string()),
        });

        assert!(app.pending.is_none());
        assert!(!app.is_loading);
        assert!(app.error_message.is_some());
    }

    /// A timeout for a previous request must not cancel a newer one.
    #[test]
    fn stale_timeout_is_ignored_matching_timeout_clears() {
        let mut app = test_app();
        set_pending(&mut app, PendingKind::CastVote);

        app.handle_nostr(NostrAction::RequestTimeout(41));
        assert!(app.pending.is_some(), "stale timeout must be ignored");

        app.handle_nostr(NostrAction::RequestTimeout(42));
        assert!(app.pending.is_none());
        assert!(!app.is_loading);
    }

    /// A whitespace-only registration token must be a local no-op: it would
    /// be trimmed to "" and sent to the EC as an empty token, guaranteeing an
    /// error (or a 30 s timeout) instead of local rejection.
    #[test]
    fn whitespace_only_registration_token_is_not_submitted() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(AppConfig::default(), AppState::default(), tx);
        app.screen = Screen::ElectionDetail {
            election_id: "e1".to_string(),
        };
        app.editing_token = true;
        app.token_input = "   ".to_string();

        app.handle_key(KeyCode::Enter);

        assert!(app.pending.is_none(), "no request may be started");
        assert!(app.editing_token, "input mode stays active for correction");
    }

    /// Esc on the password prompt must quit the app. Global `q` is disabled
    /// while typing and Ctrl+C arrives as a plain 'c' in raw mode, so without
    /// this a user who cannot supply the password is trapped in the
    /// alternate screen and has to kill the process externally.
    #[test]
    fn password_prompt_esc_quits_the_app() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut app = App::new(AppConfig::default(), AppState::default(), tx);
        app.screen = Screen::PasswordPrompt;
        app.password_input = "half-typed".to_string();

        app.handle_key(KeyCode::Esc);

        let action = rx.try_recv().expect("Esc must emit an action");
        assert!(matches!(action, Action::Quit), "Esc must request quit");
    }

    /// Pressing `g` on the Welcome screen must never overwrite an existing
    /// identity file. Reaching Welcome with an identity on disk means loading
    /// it failed (transient I/O error, corrupt-but-recoverable JSON);
    /// generating a new key would destroy the registered voter key and every
    /// registration bound to it.
    #[test]
    fn welcome_generate_refuses_to_overwrite_existing_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identity.json");
        std::fs::write(&path, "{corrupt-but-recoverable").unwrap();

        let (tx, _rx) = mpsc::unbounded_channel();
        let mut config = AppConfig::default();
        config.identity.path = path.clone();
        let mut app = App::new(config, AppState::default(), tx);
        app.screen = Screen::Welcome;

        app.handle_key(KeyCode::Char('g'));

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{corrupt-but-recoverable",
            "existing identity file must not be touched"
        );
        assert!(app.keys.is_none(), "no new key may be generated");
        assert!(app.error_message.is_some(), "user must be told why");
    }

    /// Same guard when only the encrypted sidecar exists: `identity.age`
    /// present but `identity.json` absent must also block generation.
    #[test]
    fn welcome_generate_refuses_when_encrypted_identity_exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identity.json");
        std::fs::write(path.with_extension("age"), b"age-data").unwrap();

        let (tx, _rx) = mpsc::unbounded_channel();
        let mut config = AppConfig::default();
        config.identity.path = path.clone();
        let mut app = App::new(config, AppState::default(), tx);
        app.screen = Screen::Welcome;

        app.handle_key(KeyCode::Char('g'));

        assert!(app.keys.is_none());
        assert!(app.error_message.is_some());
        assert!(!path.exists(), "no plaintext identity may be created");
    }

    /// Same for failures reported by the Nostr task.
    #[test]
    fn stale_failure_is_ignored_matching_failure_clears() {
        let mut app = test_app();
        set_pending(&mut app, PendingKind::RequestToken);

        app.handle_nostr(NostrAction::RequestFailed(7, "boom".to_string()));
        assert!(app.pending.is_some());

        app.handle_nostr(NostrAction::RequestFailed(42, "boom".to_string()));
        assert!(app.pending.is_none());
        assert!(app.error_message.is_some());
    }

    // --- update() dispatch ---------------------------------------------------

    #[test]
    fn quit_action_returns_should_quit_yes() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();

        // Act
        let result = app.update(Action::Quit);

        // Assert
        assert_eq!(result, ShouldQuit::Yes);
    }

    #[test]
    fn key_press_clears_transient_error_message() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.screen = Screen::ElectionList;
        app.error_message = Some("stale error".to_string());

        // Act
        let result = app.update(Action::KeyPress(KeyCode::Char('x')));

        // Assert
        assert_eq!(result, ShouldQuit::No);
        assert!(app.error_message.is_none());
    }

    #[test]
    fn resize_action_is_a_no_op() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.status_message = Some("hello".to_string());

        // Act
        let result = app.update(Action::Resize);

        // Assert
        assert_eq!(result, ShouldQuit::No);
        assert_eq!(app.screen, Screen::Welcome);
        assert_eq!(app.status_message.as_deref(), Some("hello"));
    }

    #[test]
    fn identity_created_sets_status_and_moves_to_election_list() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        let pubkey = "ab".repeat(32);

        // Act
        app.update(Action::IdentityCreated(pubkey.clone()));

        // Assert
        assert_eq!(app.screen, Screen::ElectionList);
        let status = app.status_message.expect("status message set");
        assert!(status.contains(&pubkey[..16]));
    }

    #[test]
    fn identity_unlocked_moves_to_election_list() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();

        // Act
        app.update(Action::IdentityUnlocked);

        // Assert
        assert_eq!(app.screen, Screen::ElectionList);
    }

    // --- Global keys -----------------------------------------------------------

    #[test]
    fn question_mark_toggles_help_on_and_off() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.screen = Screen::ElectionList;

        // Act + Assert
        press(&mut app, KeyCode::Char('?'));
        assert!(app.show_help);
        press(&mut app, KeyCode::Char('?'));
        assert!(!app.show_help);
    }

    #[test]
    fn q_emits_quit_action_outside_input_mode() {
        // Arrange
        let (mut app, mut rx, _dir) = isolated_app();
        app.screen = Screen::ElectionList;

        // Act
        press(&mut app, KeyCode::Char('q'));

        // Assert
        assert!(matches!(rx.try_recv(), Ok(Action::Quit)));
    }

    #[test]
    fn q_and_question_mark_are_typed_into_password_input_in_input_mode() {
        // Arrange
        let (mut app, mut rx, _dir) = isolated_app();
        app.screen = Screen::PasswordPrompt;

        // Act
        press(&mut app, KeyCode::Char('q'));
        press(&mut app, KeyCode::Char('?'));

        // Assert
        assert_eq!(app.password_input, "q?");
        assert!(!app.show_help);
        assert!(rx.try_recv().is_err(), "no Quit action must be emitted");
    }

    #[test]
    fn esc_closes_help_and_other_keys_are_ignored_while_help_open() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.screen = Screen::ElectionList;
        app.elections.insert(
            "e1".to_string(),
            plurality_election("e1", "General", ElectionStatus::Open),
        );
        app.show_help = true;

        // Act: navigation and Enter must be swallowed while help is open
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);

        // Assert
        assert_eq!(app.election_list_index, 0);
        assert_eq!(app.screen, Screen::ElectionList);

        // Act: Esc closes help
        press(&mut app, KeyCode::Esc);

        // Assert
        assert!(!app.show_help);
    }

    // --- Welcome screen --------------------------------------------------------

    #[test]
    fn welcome_g_generates_identity_in_tempdir_and_emits_identity_created() {
        // Arrange
        let (mut app, mut rx, _dir) = isolated_app();

        // Act
        press(&mut app, KeyCode::Char('g'));

        // Assert
        let keys = app.keys.as_ref().expect("keys generated");
        assert!(
            app.config.identity.path.exists(),
            "identity file must be written inside the tempdir"
        );
        match rx.try_recv() {
            Ok(Action::IdentityCreated(pubkey)) => {
                assert_eq!(pubkey, keys.public_key().to_hex());
            }
            other => panic!("expected IdentityCreated, got {other:?}"),
        }
    }

    #[test]
    fn welcome_i_and_unknown_keys_are_no_ops() {
        // Arrange
        let (mut app, mut rx, _dir) = isolated_app();

        // Act
        press(&mut app, KeyCode::Char('i'));
        press(&mut app, KeyCode::Char('x'));

        // Assert
        assert!(app.keys.is_none());
        assert_eq!(app.screen, Screen::Welcome);
        assert!(rx.try_recv().is_err());
    }

    // --- Password prompt ---------------------------------------------------------

    #[test]
    fn password_prompt_accumulates_chars_and_backspace_pops() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.screen = Screen::PasswordPrompt;

        // Act
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Char('b'));
        press(&mut app, KeyCode::Char('c'));
        press(&mut app, KeyCode::Backspace);
        press(&mut app, KeyCode::Left); // ignored key

        // Assert
        assert_eq!(app.password_input, "ab");
    }

    #[test]
    fn password_prompt_wrong_password_sets_error_and_clears_input() {
        // Arrange: a real age-encrypted identity inside the tempdir
        let (mut app, _rx, _dir) = isolated_app();
        let keys = voter::identity::generate_keypair();
        voter::identity::save_identity(&keys, Some("correct-horse"), &app.config.identity.path)
            .expect("save encrypted identity");
        app.screen = Screen::PasswordPrompt;
        app.password_input = "wrong".to_string();

        // Act
        press(&mut app, KeyCode::Enter);

        // Assert
        let error = app.error_message.expect("unlock error set");
        assert!(error.contains("Unlock failed"));
        assert!(app.password_input.is_empty());
        assert!(app.keys.is_none());
    }

    #[test]
    fn password_prompt_correct_password_unlocks_and_emits_identity_unlocked() {
        // Arrange
        let (mut app, mut rx, _dir) = isolated_app();
        let keys = voter::identity::generate_keypair();
        voter::identity::save_identity(&keys, Some("correct-horse"), &app.config.identity.path)
            .expect("save encrypted identity");
        app.screen = Screen::PasswordPrompt;
        app.password_input = "correct-horse".to_string();

        // Act
        press(&mut app, KeyCode::Enter);

        // Assert
        let unlocked = app.keys.as_ref().expect("keys unlocked");
        assert_eq!(unlocked.public_key(), keys.public_key());
        assert!(app.password_input.is_empty());
        assert!(matches!(rx.try_recv(), Ok(Action::IdentityUnlocked)));
    }

    // --- Election list -----------------------------------------------------------

    #[test]
    fn election_list_navigation_clamps_on_empty_and_non_empty_lists() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.screen = Screen::ElectionList;

        // Act + Assert: empty list — both directions stay at 0
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.election_list_index, 0);
        press(&mut app, KeyCode::Char('k'));
        assert_eq!(app.election_list_index, 0);

        // Arrange: two elections
        app.elections.insert(
            "e1".to_string(),
            plurality_election("e1", "Alpha", ElectionStatus::Open),
        );
        app.elections.insert(
            "e2".to_string(),
            plurality_election("e2", "Beta", ElectionStatus::Open),
        );

        // Act + Assert: down clamps at last index, up clamps at 0
        press(&mut app, KeyCode::Down);
        assert_eq!(app.election_list_index, 1);
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.election_list_index, 1, "down must clamp at the end");
        press(&mut app, KeyCode::Up);
        assert_eq!(app.election_list_index, 0);
        press(&mut app, KeyCode::Char('k'));
        assert_eq!(app.election_list_index, 0, "up must clamp at 0");
    }

    #[test]
    fn election_list_enter_with_no_elections_is_a_no_op() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.screen = Screen::ElectionList;

        // Act
        press(&mut app, KeyCode::Enter);

        // Assert
        assert_eq!(app.screen, Screen::ElectionList);
    }

    #[test]
    fn election_list_enter_opens_detail_of_name_sorted_selection() {
        // Arrange: insertion order differs from name order
        let (mut app, _rx, _dir) = isolated_app();
        app.screen = Screen::ElectionList;
        app.elections.insert(
            "id-z".to_string(),
            plurality_election("id-z", "Alpha", ElectionStatus::Open),
        );
        app.elections.insert(
            "id-a".to_string(),
            plurality_election("id-a", "Zeta", ElectionStatus::Open),
        );
        app.election_list_index = 1;

        // Act
        press(&mut app, KeyCode::Enter);

        // Assert: index 1 in name order (Alpha, Zeta) is "Zeta" = id-a
        assert_eq!(
            app.screen,
            Screen::ElectionDetail {
                election_id: "id-a".to_string()
            }
        );
    }

    #[test]
    fn election_list_s_opens_settings_and_remembers_previous_screen() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.screen = Screen::ElectionList;

        // Act
        press(&mut app, KeyCode::Char('s'));

        // Assert
        assert_eq!(app.screen, Screen::Settings);
        assert_eq!(app.previous_screen, Some(Screen::ElectionList));
    }

    #[test]
    fn sorted_election_ids_are_sorted_by_name() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.elections.insert(
            "id-c".to_string(),
            plurality_election("id-c", "Zebra", ElectionStatus::Open),
        );
        app.elections.insert(
            "id-a".to_string(),
            plurality_election("id-a", "Mango", ElectionStatus::Open),
        );
        app.elections.insert(
            "id-b".to_string(),
            plurality_election("id-b", "Apple", ElectionStatus::Open),
        );

        // Act
        let ids = app.sorted_election_ids();

        // Assert
        assert_eq!(ids, vec!["id-b", "id-a", "id-c"]);
    }

    // --- Election detail -----------------------------------------------------------

    #[test]
    fn detail_esc_returns_to_list_and_clears_token_input() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.elections.insert(
            "e1".to_string(),
            plurality_election("e1", "General", ElectionStatus::Open),
        );
        app.screen = Screen::ElectionDetail {
            election_id: "e1".to_string(),
        };
        app.token_input = "half-typed".to_string();

        // Act
        press(&mut app, KeyCode::Esc);

        // Assert
        assert_eq!(app.screen, Screen::ElectionList);
        assert!(app.token_input.is_empty());
    }

    #[test]
    fn detail_enter_starts_token_editing_only_when_open_and_unregistered() {
        // Arrange: open election, not registered
        let (mut app, _rx, _dir) = isolated_app();
        app.elections.insert(
            "e1".to_string(),
            plurality_election("e1", "General", ElectionStatus::Open),
        );
        app.screen = Screen::ElectionDetail {
            election_id: "e1".to_string(),
        };

        // Act
        press(&mut app, KeyCode::Enter);

        // Assert
        assert!(app.editing_token, "open + unregistered must start editing");

        // Arrange: registered voter must not re-enter editing
        app.editing_token = false;
        app.persistent_state.mark_registered("e1".to_string());

        // Act
        press(&mut app, KeyCode::Enter);

        // Assert
        assert!(!app.editing_token, "registered voter must not edit a token");
    }

    #[test]
    fn token_editing_accepts_chars_backspace_and_esc_cancels() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.elections.insert(
            "e1".to_string(),
            plurality_election("e1", "General", ElectionStatus::Open),
        );
        app.screen = Screen::ElectionDetail {
            election_id: "e1".to_string(),
        };
        app.editing_token = true;

        // Act: type, then correct a typo
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Char('b'));
        press(&mut app, KeyCode::Backspace);

        // Assert
        assert_eq!(app.token_input, "a");

        // Act: Esc cancels editing and clears the buffer
        press(&mut app, KeyCode::Esc);

        // Assert
        assert!(!app.editing_token);
        assert!(app.token_input.is_empty());
        assert_eq!(
            app.screen,
            Screen::ElectionDetail {
                election_id: "e1".to_string()
            },
            "Esc in editing mode must not leave the detail screen"
        );
    }

    #[test]
    fn submit_registration_sends_register_command_and_sets_pending() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.elections.insert(
            "e1".to_string(),
            plurality_election("e1", "General", ElectionStatus::Open),
        );
        let mut cmd_rx = attach_cmd_channel(&mut app);
        app.screen = Screen::ElectionDetail {
            election_id: "e1".to_string(),
        };
        app.editing_token = true;
        app.token_input = "  reg-token-123  ".to_string();

        // Act
        press(&mut app, KeyCode::Enter);

        // Assert
        assert!(!app.editing_token);
        assert!(app.token_input.is_empty());
        assert!(app.is_loading);
        assert!(app.loading_step.is_some());
        let pending = app.pending.as_ref().expect("pending register request");
        assert_eq!(pending.kind, PendingKind::Register);
        assert_eq!(pending.election_id, "e1");
        match cmd_rx.try_recv().expect("command sent") {
            VoterCommand::Send {
                ec_pubkey,
                msg:
                    VoterMessage::Register {
                        election_id,
                        registration_token,
                        request_id,
                    },
                ..
            } => {
                assert_eq!(ec_pubkey, "ec-pubkey-hex");
                assert_eq!(election_id, "e1");
                assert_eq!(registration_token, "reg-token-123", "token must be trimmed");
                assert_eq!(request_id, pending.request_id);
            }
            other => panic!("expected Register Send, got {other:?}"),
        }
    }

    #[test]
    fn submit_registration_without_cmd_tx_reports_not_connected() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.elections.insert(
            "e1".to_string(),
            plurality_election("e1", "General", ElectionStatus::Open),
        );
        app.screen = Screen::ElectionDetail {
            election_id: "e1".to_string(),
        };
        app.editing_token = true;
        app.token_input = "reg-token".to_string();

        // Act
        press(&mut app, KeyCode::Enter);

        // Assert
        assert_eq!(
            app.error_message.as_deref(),
            Some("Not connected to relays")
        );
        assert!(app.pending.is_none());
        assert!(!app.is_loading);
    }

    #[test]
    fn submit_registration_without_resolvable_ec_pubkey_reports_unknown_key() {
        // Arrange: no pinned EC key and no announcement author
        let (mut app, _rx, _dir) = isolated_app();
        let election = Election {
            ec_pubkey: None,
            ..plurality_election("e1", "General", ElectionStatus::Open)
        };
        app.elections.insert("e1".to_string(), election);
        let mut cmd_rx = attach_cmd_channel(&mut app);
        app.screen = Screen::ElectionDetail {
            election_id: "e1".to_string(),
        };
        app.editing_token = true;
        app.token_input = "reg-token".to_string();

        // Act
        press(&mut app, KeyCode::Enter);

        // Assert
        assert_eq!(
            app.error_message.as_deref(),
            Some("EC public key unknown for this election")
        );
        assert!(app.pending.is_none());
        assert!(cmd_rx.try_recv().is_err(), "no command may be sent");
    }

    #[test]
    fn t_requests_token_when_registered_and_in_progress() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        let (election, _sk) = rsa_election("e1", ElectionStatus::InProgress);
        app.elections.insert("e1".to_string(), election);
        app.persistent_state.mark_registered("e1".to_string());
        let mut cmd_rx = attach_cmd_channel(&mut app);
        app.screen = Screen::ElectionDetail {
            election_id: "e1".to_string(),
        };

        // Act
        press(&mut app, KeyCode::Char('t'));

        // Assert
        let pending = app.pending.as_ref().expect("pending token request");
        assert_eq!(pending.kind, PendingKind::RequestToken);
        assert!(app.pending_blind.is_some(), "blinding secret must be held");
        assert!(app.is_loading);
        match cmd_rx.try_recv().expect("command sent") {
            VoterCommand::Send {
                msg:
                    VoterMessage::RequestToken {
                        election_id,
                        blinded_nonce,
                        request_id,
                    },
                ..
            } => {
                assert_eq!(election_id, "e1");
                assert!(!blinded_nonce.is_empty());
                assert_eq!(request_id, pending.request_id);
            }
            other => panic!("expected RequestToken Send, got {other:?}"),
        }
    }

    #[test]
    fn t_is_blocked_when_unregistered_wrong_status_token_held_or_voted() {
        let cases: [(&str, ElectionStatus, bool, Option<bool>); 4] = [
            // (case, status, registered, token consumed if stored)
            ("unregistered", ElectionStatus::InProgress, false, None),
            ("wrong status", ElectionStatus::Open, true, None),
            ("token held", ElectionStatus::InProgress, true, Some(false)),
            (
                "already voted",
                ElectionStatus::InProgress,
                true,
                Some(true),
            ),
        ];

        for (case, status, registered, token_consumed) in cases {
            // Arrange
            let (mut app, _rx, _dir) = isolated_app();
            app.elections.insert(
                "e1".to_string(),
                plurality_election("e1", "General", status),
            );
            if registered {
                app.persistent_state.mark_registered("e1".to_string());
            }
            if let Some(consumed) = token_consumed {
                app.persistent_state
                    .store_token("e1".to_string(), stored_token(consumed));
            }
            let mut cmd_rx = attach_cmd_channel(&mut app);
            app.screen = Screen::ElectionDetail {
                election_id: "e1".to_string(),
            };

            // Act
            press(&mut app, KeyCode::Char('t'));

            // Assert
            assert!(app.pending.is_none(), "case '{case}': must not send");
            assert!(app.pending_blind.is_none(), "case '{case}'");
            assert!(app.error_message.is_none(), "case '{case}'");
            assert!(cmd_rx.try_recv().is_err(), "case '{case}': no command");
        }
    }

    #[test]
    fn r_opens_results_only_when_results_exist() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.elections.insert(
            "e1".to_string(),
            plurality_election("e1", "General", ElectionStatus::Finished),
        );
        app.screen = Screen::ElectionDetail {
            election_id: "e1".to_string(),
        };

        // Act: no results yet
        press(&mut app, KeyCode::Char('r'));

        // Assert
        assert_eq!(
            app.screen,
            Screen::ElectionDetail {
                election_id: "e1".to_string()
            }
        );

        // Arrange: results arrive
        app.results.insert(
            "e1".to_string(),
            ElectionResults {
                election_id: "e1".to_string(),
                elected: vec![1],
                tally: vec![],
            },
        );

        // Act
        press(&mut app, KeyCode::Char('r'));

        // Assert
        assert_eq!(
            app.screen,
            Screen::Results {
                election_id: "e1".to_string()
            }
        );
    }

    #[test]
    fn v_opens_vote_screen_only_with_active_token() {
        // Arrange: in-progress election but no token
        let (mut app, _rx, _dir) = isolated_app();
        app.elections.insert(
            "e1".to_string(),
            plurality_election("e1", "General", ElectionStatus::InProgress),
        );
        app.screen = Screen::ElectionDetail {
            election_id: "e1".to_string(),
        };

        // Act
        press(&mut app, KeyCode::Char('v'));

        // Assert
        assert_eq!(
            app.screen,
            Screen::ElectionDetail {
                election_id: "e1".to_string()
            },
            "without a token 'v' must be a no-op"
        );

        // Arrange: active token, plus stale selection state to be reset
        app.persistent_state
            .store_token("e1".to_string(), stored_token(false));
        app.candidate_list_index = 1;
        app.stv_ranking = vec![2];
        app.vote_confirm = Some(true);

        // Act
        press(&mut app, KeyCode::Char('v'));

        // Assert
        assert_eq!(
            app.screen,
            Screen::Vote {
                election_id: "e1".to_string()
            }
        );
        assert_eq!(app.candidate_list_index, 0);
        assert!(app.stv_ranking.is_empty());
        assert!(app.vote_confirm.is_none());
    }

    // --- Vote screen ---------------------------------------------------------------

    fn vote_screen_app(election: Election) -> (App, mpsc::UnboundedReceiver<Action>, TempDir) {
        let (mut app, rx, dir) = isolated_app();
        let eid = election.election_id.clone();
        app.elections.insert(eid.clone(), election);
        app.screen = Screen::Vote { election_id: eid };
        (app, rx, dir)
    }

    #[test]
    fn plurality_enter_replaces_selection_with_single_candidate() {
        // Arrange
        let (mut app, _rx, _dir) = vote_screen_app(plurality_election(
            "e1",
            "General",
            ElectionStatus::InProgress,
        ));

        // Act: select the first candidate, then move and select the second
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.stv_ranking, vec![1]);
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char(' '));

        // Assert: plurality keeps exactly one selection
        assert_eq!(app.stv_ranking, vec![2]);
    }

    #[test]
    fn stv_enter_appends_unique_and_d_removes() {
        // Arrange
        let (mut app, _rx, _dir) = vote_screen_app(stv_election("e1", "Council"));

        // Act: rank candidate 1 twice (dup must be ignored), then candidate 2
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);

        // Assert
        assert_eq!(app.stv_ranking, vec![1, 2], "no duplicate ranks");

        // Act: 'd' removes the highlighted candidate (still index 1 = Bob)
        press(&mut app, KeyCode::Char('d'));

        // Assert
        assert_eq!(app.stv_ranking, vec![1]);

        // Act: 'd' on an unranked candidate changes nothing
        press(&mut app, KeyCode::Char('d'));
        assert_eq!(app.stv_ranking, vec![1]);
    }

    #[test]
    fn vote_navigation_clamps_at_both_ends() {
        // Arrange: two candidates
        let (mut app, _rx, _dir) = vote_screen_app(plurality_election(
            "e1",
            "General",
            ElectionStatus::InProgress,
        ));

        // Act + Assert
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.candidate_list_index, 1, "down clamps at last candidate");
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Char('k'));
        assert_eq!(app.candidate_list_index, 0, "up clamps at 0");
    }

    #[test]
    fn s_opens_confirm_dialog_only_with_a_ranking() {
        // Arrange
        let (mut app, _rx, _dir) = vote_screen_app(plurality_election(
            "e1",
            "General",
            ElectionStatus::InProgress,
        ));

        // Act: empty ranking
        press(&mut app, KeyCode::Char('s'));

        // Assert
        assert!(app.vote_confirm.is_none());

        // Act: with a ranking
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('s'));

        // Assert: dialog opens with the cancel button focused
        assert_eq!(app.vote_confirm, Some(false));
    }

    #[test]
    fn confirm_dialog_toggles_and_closes_without_submitting() {
        // Arrange
        let (mut app, _rx, _dir) = vote_screen_app(plurality_election(
            "e1",
            "General",
            ElectionStatus::InProgress,
        ));
        let mut cmd_rx = attach_cmd_channel(&mut app);
        app.stv_ranking = vec![1];
        app.vote_confirm = Some(false);

        // Act + Assert: Tab/Left/Right all toggle focus
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.vote_confirm, Some(true));
        press(&mut app, KeyCode::Left);
        assert_eq!(app.vote_confirm, Some(false));
        press(&mut app, KeyCode::Right);
        assert_eq!(app.vote_confirm, Some(true));

        // Act: Esc closes the dialog
        press(&mut app, KeyCode::Esc);
        assert!(app.vote_confirm.is_none());
        assert_eq!(
            app.screen,
            Screen::Vote {
                election_id: "e1".to_string()
            },
            "Esc closes the dialog, not the vote screen"
        );

        // Act: Enter on the unfocused (cancel) button closes without submit
        app.vote_confirm = Some(false);
        press(&mut app, KeyCode::Enter);

        // Assert
        assert!(app.vote_confirm.is_none());
        assert!(app.pending.is_none());
        assert!(cmd_rx.try_recv().is_err(), "no vote may be submitted");
    }

    #[test]
    fn confirm_dialog_focused_enter_submits_anonymous_cast_vote() {
        // Arrange
        let (mut app, _rx, _dir) = vote_screen_app(plurality_election(
            "e1",
            "General",
            ElectionStatus::InProgress,
        ));
        let token = stored_token(false);
        let expected_h_n = token.h_n.clone();
        app.persistent_state.store_token("e1".to_string(), token);
        let mut cmd_rx = attach_cmd_channel(&mut app);
        app.stv_ranking = vec![2];
        app.vote_confirm = Some(true);

        // Act
        press(&mut app, KeyCode::Enter);

        // Assert
        assert!(app.vote_confirm.is_none());
        let pending = app.pending.as_ref().expect("pending cast vote");
        assert_eq!(pending.kind, PendingKind::CastVote);
        match cmd_rx.try_recv().expect("command sent") {
            VoterCommand::SendAnonymous {
                msg:
                    VoterMessage::CastVote {
                        election_id,
                        candidate_ids,
                        h_n,
                        token: wire_token,
                        request_id,
                    },
                ..
            } => {
                assert_eq!(election_id, "e1");
                assert_eq!(candidate_ids, vec![2]);
                assert_eq!(h_n, expected_h_n);
                assert!(!wire_token.is_empty());
                assert_eq!(request_id, pending.request_id);
            }
            other => panic!("expected SendAnonymous CastVote, got {other:?}"),
        }
    }

    #[test]
    fn submit_vote_without_token_sets_error() {
        // Arrange: no stored token
        let (mut app, _rx, _dir) = vote_screen_app(plurality_election(
            "e1",
            "General",
            ElectionStatus::InProgress,
        ));
        let mut cmd_rx = attach_cmd_channel(&mut app);
        app.stv_ranking = vec![1];
        app.vote_confirm = Some(true);

        // Act
        press(&mut app, KeyCode::Enter);

        // Assert
        assert_eq!(
            app.error_message.as_deref(),
            Some("No voting token for this election")
        );
        assert!(app.pending.is_none());
        assert!(cmd_rx.try_recv().is_err());
    }

    #[test]
    fn submit_vote_with_corrupt_stored_token_sets_error() {
        // Arrange: stored signature is not valid base64
        let (mut app, _rx, _dir) = vote_screen_app(plurality_election(
            "e1",
            "General",
            ElectionStatus::InProgress,
        ));
        let corrupt = token::VotingToken {
            signature_b64: "%%%not-base64%%%".to_string(),
            ..stored_token(false)
        };
        app.persistent_state.store_token("e1".to_string(), corrupt);
        let mut cmd_rx = attach_cmd_channel(&mut app);
        app.stv_ranking = vec![1];
        app.vote_confirm = Some(true);

        // Act
        press(&mut app, KeyCode::Enter);

        // Assert
        let error = app.error_message.expect("corrupt token error");
        assert!(error.starts_with("Stored token is corrupt"));
        assert!(app.pending.is_none());
        assert!(cmd_rx.try_recv().is_err());
    }

    #[test]
    fn vote_esc_returns_to_election_detail() {
        // Arrange
        let (mut app, _rx, _dir) = vote_screen_app(plurality_election(
            "e1",
            "General",
            ElectionStatus::InProgress,
        ));

        // Act
        press(&mut app, KeyCode::Esc);

        // Assert
        assert_eq!(
            app.screen,
            Screen::ElectionDetail {
                election_id: "e1".to_string()
            }
        );
    }

    // --- Results & Settings ----------------------------------------------------------

    #[test]
    fn results_esc_returns_to_election_detail() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.screen = Screen::Results {
            election_id: "e1".to_string(),
        };

        // Act
        press(&mut app, KeyCode::Esc);

        // Assert
        assert_eq!(
            app.screen,
            Screen::ElectionDetail {
                election_id: "e1".to_string()
            }
        );
    }

    #[test]
    fn settings_esc_returns_to_previous_screen_or_election_list() {
        // Arrange: with a remembered previous screen
        let (mut app, _rx, _dir) = isolated_app();
        app.screen = Screen::Settings;
        app.previous_screen = Some(Screen::ElectionDetail {
            election_id: "e1".to_string(),
        });

        // Act
        press(&mut app, KeyCode::Esc);

        // Assert
        assert_eq!(
            app.screen,
            Screen::ElectionDetail {
                election_id: "e1".to_string()
            }
        );
        assert!(app.previous_screen.is_none());

        // Arrange: without a previous screen
        app.screen = Screen::Settings;

        // Act
        press(&mut app, KeyCode::Esc);

        // Assert
        assert_eq!(app.screen, Screen::ElectionList);
    }

    // --- handle_nostr ------------------------------------------------------------------

    #[test]
    fn election_update_inserts_and_replaces_by_id() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();

        // Act
        app.update(Action::Nostr(NostrAction::ElectionUpdate(
            plurality_election("e1", "First", ElectionStatus::Open),
        )));
        app.update(Action::Nostr(NostrAction::ElectionUpdate(
            plurality_election("e1", "Renamed", ElectionStatus::InProgress),
        )));

        // Assert
        assert_eq!(app.elections.len(), 1);
        let election = app.elections.get("e1").expect("election stored");
        assert_eq!(election.name, "Renamed");
        assert_eq!(election.status, ElectionStatus::InProgress);
    }

    #[test]
    fn election_result_is_inserted() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();

        // Act
        app.update(Action::Nostr(NostrAction::ElectionResult(
            ElectionResults {
                election_id: "e1".to_string(),
                elected: vec![2],
                tally: vec![],
            },
        )));

        // Assert
        let results = app.results.get("e1").expect("results stored");
        assert_eq!(results.elected, vec![2]);
    }

    #[test]
    fn connection_status_updates_connected_flag_and_messages() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.error_message = Some("old error".to_string());

        // Act: connected
        app.update(Action::Nostr(NostrAction::ConnectionStatus(true)));

        // Assert
        assert!(app.connected);
        assert_eq!(app.status_message.as_deref(), Some("Connected to relays"));
        assert!(app.error_message.is_none());

        // Act: disconnected
        app.update(Action::Nostr(NostrAction::ConnectionStatus(false)));

        // Assert
        assert!(!app.connected);
        assert!(app.status_message.is_none());
        assert_eq!(
            app.error_message.as_deref(),
            Some("Disconnected from relays")
        );
    }

    #[test]
    fn nostr_error_sets_error_message() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();

        // Act
        app.update(Action::Nostr(NostrAction::Error(
            "relay exploded".to_string(),
        )));

        // Assert
        assert_eq!(app.error_message.as_deref(), Some("relay exploded"));
    }

    // --- handle_ec_response happy paths --------------------------------------------------

    #[test]
    fn register_confirmed_persists_registration_to_tempdir_state_file() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        set_pending(&mut app, PendingKind::Register);

        // Act
        app.handle_ec_response(&EcResponse::Ok {
            action: "register-confirmed".to_string(),
            blind_signature: None,
            request_id: Some("req-42".to_string()),
        });

        // Assert
        assert!(app.persistent_state.is_registered("e1"));
        assert_eq!(app.status_message.as_deref(), Some("Registered ✓"));
        assert!(app.pending.is_none());
        assert!(!app.is_loading);
        assert!(
            app.state_path.exists(),
            "state must be saved to the tempdir"
        );
        let reloaded = AppState::load(&app.state_path).expect("reload persisted state");
        assert!(reloaded.is_registered("e1"));
    }

    #[test]
    fn token_issued_with_valid_blind_signature_stores_verified_token() {
        // Arrange: a registered, in-progress election with a real RSA key
        let (mut app, _rx, _dir) = isolated_app();
        let (election, sk) = rsa_election("e1", ElectionStatus::InProgress);
        app.elections.insert("e1".to_string(), election);
        app.persistent_state.mark_registered("e1".to_string());
        let mut cmd_rx = attach_cmd_channel(&mut app);
        app.screen = Screen::ElectionDetail {
            election_id: "e1".to_string(),
        };
        press(&mut app, KeyCode::Char('t'));
        let (blinded_nonce, request_id) = match cmd_rx.try_recv().expect("token request sent") {
            VoterCommand::Send {
                msg:
                    VoterMessage::RequestToken {
                        blinded_nonce,
                        request_id,
                        ..
                    },
                ..
            } => (blinded_nonce, request_id),
            other => panic!("expected RequestToken Send, got {other:?}"),
        };

        // Act: play the EC — blind-sign the blinded nonce and echo the id
        let blinded = BASE64_STANDARD.decode(&blinded_nonce).expect("valid b64");
        let blind_sig = sk.blind_sign(&blinded).expect("EC blind sign");
        let blind_sig_b64 = BASE64_STANDARD.encode(&blind_sig.0);
        app.handle_ec_response(&EcResponse::Ok {
            action: "token-issued".to_string(),
            blind_signature: Some(blind_sig_b64),
            request_id: Some(request_id),
        });

        // Assert
        assert!(app.pending.is_none());
        assert!(app.pending_blind.is_none());
        assert!(!app.is_loading);
        assert_eq!(
            app.status_message.as_deref(),
            Some("Voting token received and verified ✓")
        );
        let stored = app
            .persistent_state
            .get_active_token("e1")
            .expect("verified token stored");
        assert!(!stored.h_n.is_empty());
        let reloaded = AppState::load(&app.state_path).expect("reload persisted state");
        assert!(reloaded.get_active_token("e1").is_some());
    }

    #[test]
    fn token_issued_without_blind_signature_reports_missing_signature() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        set_pending(&mut app, PendingKind::RequestToken);

        // Act
        app.handle_ec_response(&EcResponse::Ok {
            action: "token-issued".to_string(),
            blind_signature: None,
            request_id: Some("req-42".to_string()),
        });

        // Assert
        assert!(app.pending.is_none());
        assert!(!app.is_loading);
        let error = app.error_message.expect("missing signature error");
        assert!(error.contains("missing blind signature"));
        assert!(app.persistent_state.get_active_token("e1").is_none());
    }

    #[test]
    fn token_issued_with_garbage_signature_fails_verification() {
        // Arrange: real request in flight so a genuine blinding secret exists
        let (mut app, _rx, _dir) = isolated_app();
        let (election, _sk) = rsa_election("e1", ElectionStatus::InProgress);
        app.elections.insert("e1".to_string(), election);
        app.persistent_state.mark_registered("e1".to_string());
        let mut cmd_rx = attach_cmd_channel(&mut app);
        app.screen = Screen::ElectionDetail {
            election_id: "e1".to_string(),
        };
        press(&mut app, KeyCode::Char('t'));
        let request_id = match cmd_rx.try_recv().expect("token request sent") {
            VoterCommand::Send {
                msg: VoterMessage::RequestToken { request_id, .. },
                ..
            } => request_id,
            other => panic!("expected RequestToken Send, got {other:?}"),
        };

        // Act: a garbage blind signature must never become a stored token
        let garbage = BASE64_STANDARD.encode([0u8; 256]);
        app.handle_ec_response(&EcResponse::Ok {
            action: "token-issued".to_string(),
            blind_signature: Some(garbage),
            request_id: Some(request_id),
        });

        // Assert
        let error = app.error_message.expect("verification error");
        assert!(error.contains("Token verification failed"));
        assert!(app.persistent_state.get_active_token("e1").is_none());
        assert!(app.pending.is_none());
        assert!(app.pending_blind.is_none());
    }

    #[test]
    fn vote_recorded_consumes_token_and_returns_to_detail() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.persistent_state
            .store_token("e1".to_string(), stored_token(false));
        app.stv_ranking = vec![1];
        app.screen = Screen::Vote {
            election_id: "e1".to_string(),
        };
        set_pending(&mut app, PendingKind::CastVote);

        // Act
        app.handle_ec_response(&EcResponse::Ok {
            action: "vote-recorded".to_string(),
            blind_signature: None,
            request_id: Some("req-42".to_string()),
        });

        // Assert
        assert!(app.persistent_state.has_voted("e1"));
        assert!(app.stv_ranking.is_empty());
        assert_eq!(
            app.screen,
            Screen::ElectionDetail {
                election_id: "e1".to_string()
            }
        );
        assert_eq!(app.status_message.as_deref(), Some("Vote recorded ✓"));
        let reloaded = AppState::load(&app.state_path).expect("reload persisted state");
        assert!(reloaded.has_voted("e1"));
    }

    #[test]
    fn vote_recorded_without_stored_token_reports_state_error() {
        // Arrange: no token was ever stored for this election
        let (mut app, _rx, _dir) = isolated_app();
        app.stv_ranking = vec![1];
        app.screen = Screen::Vote {
            election_id: "e1".to_string(),
        };
        set_pending(&mut app, PendingKind::CastVote);

        // Act
        app.handle_ec_response(&EcResponse::Ok {
            action: "vote-recorded".to_string(),
            blind_signature: None,
            request_id: Some("req-42".to_string()),
        });

        // Assert
        let error = app.error_message.expect("state error surfaced");
        assert!(error.contains("state error"));
        assert!(app.stv_ranking.is_empty());
        assert_eq!(
            app.screen,
            Screen::ElectionDetail {
                election_id: "e1".to_string()
            }
        );
    }

    // --- EC response formatting ------------------------------------------------------------

    #[test]
    fn uncorrelated_responses_format_status_and_error_messages() {
        // Arrange: nothing in flight
        let (mut app, _rx, _dir) = isolated_app();

        // Act + Assert: Ok with a signature
        app.handle_ec_response(&EcResponse::Ok {
            action: "token-issued".to_string(),
            blind_signature: Some("sig".to_string()),
            request_id: None,
        });
        assert_eq!(
            app.status_message.as_deref(),
            Some("EC: token-issued (signature received)")
        );

        // Act + Assert: Ok without a signature
        app.handle_ec_response(&EcResponse::Ok {
            action: "vote-recorded".to_string(),
            blind_signature: None,
            request_id: None,
        });
        assert_eq!(app.status_message.as_deref(), Some("EC: vote-recorded"));

        // Act + Assert: Error with nothing pending goes to error_message
        app.handle_ec_response(&EcResponse::Error {
            code: voter::nostr::messages::EcErrorCode::InternalError,
            message: "boom".to_string(),
            request_id: None,
        });
        let error = app.error_message.expect("EC error surfaced");
        assert!(error.starts_with("EC error:"));
        assert!(error.contains("boom"));
    }

    // --- Remaining edge paths ----------------------------------------------------------

    #[test]
    fn ec_response_is_routed_through_nostr_action_dispatch() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();

        // Act: an uncorrelated Ok arriving via the normal Nostr action path
        app.update(Action::Nostr(NostrAction::EcResponse(EcResponse::Ok {
            action: "register-confirmed".to_string(),
            blind_signature: None,
            request_id: None,
        })));

        // Assert
        assert_eq!(
            app.status_message.as_deref(),
            Some("EC: register-confirmed")
        );
    }

    #[test]
    fn welcome_g_reports_error_when_identity_path_is_not_writable() {
        // Arrange: the identity path's parent is a regular file, so directory
        // creation fails — still entirely inside the tempdir.
        let (mut app, mut rx, dir) = isolated_app();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"not a directory").expect("create blocker file");
        app.config.identity.path = blocker.join("identity.json");

        // Act
        press(&mut app, KeyCode::Char('g'));

        // Assert
        let error = app.error_message.expect("save error surfaced");
        assert!(error.contains("Failed to save identity"));
        assert!(app.keys.is_none());
        assert!(rx.try_recv().is_err(), "no IdentityCreated may be emitted");
    }

    #[test]
    fn token_editing_ignores_unrelated_keys() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();
        app.elections.insert(
            "e1".to_string(),
            plurality_election("e1", "General", ElectionStatus::Open),
        );
        app.screen = Screen::ElectionDetail {
            election_id: "e1".to_string(),
        };
        app.editing_token = true;
        app.token_input = "abc".to_string();

        // Act
        press(&mut app, KeyCode::Left);
        press(&mut app, KeyCode::F(1));

        // Assert
        assert!(app.editing_token);
        assert_eq!(app.token_input, "abc");
    }

    #[test]
    fn detail_and_vote_screens_ignore_unknown_keys() {
        // Arrange: detail screen
        let (mut app, _rx, _dir) = isolated_app();
        app.elections.insert(
            "e1".to_string(),
            plurality_election("e1", "General", ElectionStatus::InProgress),
        );
        app.screen = Screen::ElectionDetail {
            election_id: "e1".to_string(),
        };

        // Act + Assert
        press(&mut app, KeyCode::Char('z'));
        assert_eq!(
            app.screen,
            Screen::ElectionDetail {
                election_id: "e1".to_string()
            }
        );

        // Arrange: vote screen
        app.screen = Screen::Vote {
            election_id: "e1".to_string(),
        };

        // Act + Assert
        press(&mut app, KeyCode::Char('x'));
        assert!(app.stv_ranking.is_empty());
        assert_eq!(
            app.screen,
            Screen::Vote {
                election_id: "e1".to_string()
            }
        );
    }

    #[test]
    fn confirm_dialog_ignores_unknown_keys() {
        // Arrange
        let (mut app, _rx, _dir) = vote_screen_app(plurality_election(
            "e1",
            "General",
            ElectionStatus::InProgress,
        ));
        app.stv_ranking = vec![1];
        app.vote_confirm = Some(false);

        // Act
        press(&mut app, KeyCode::Char('x'));

        // Assert
        assert_eq!(app.vote_confirm, Some(false));
    }

    #[test]
    fn vote_selection_with_out_of_range_index_is_a_no_op() {
        // Arrange
        let (mut app, _rx, _dir) = vote_screen_app(stv_election("e1", "Council"));
        app.stv_ranking = vec![1];
        app.candidate_list_index = 99;

        // Act
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));

        // Assert
        assert_eq!(app.stv_ranking, vec![1]);
    }

    #[test]
    fn send_command_with_closed_channel_reports_connection_task_down() {
        // Arrange: a cmd channel whose receiver has been dropped
        let (mut app, _rx, _dir) = isolated_app();
        app.elections.insert(
            "e1".to_string(),
            plurality_election("e1", "General", ElectionStatus::Open),
        );
        let cmd_rx = attach_cmd_channel(&mut app);
        drop(cmd_rx);
        app.screen = Screen::ElectionDetail {
            election_id: "e1".to_string(),
        };
        app.editing_token = true;
        app.token_input = "reg-token".to_string();

        // Act
        press(&mut app, KeyCode::Enter);

        // Assert
        assert_eq!(
            app.error_message.as_deref(),
            Some("Connection task is not running")
        );
        assert!(app.pending.is_none());
        assert!(!app.is_loading);
    }

    #[test]
    fn screen_specific_handlers_ignore_keys_when_screen_mismatches() {
        // Arrange: defensive guards for handlers invoked with a foreign screen
        let (mut app, _rx, _dir) = isolated_app();
        app.screen = Screen::ElectionList;

        // Act
        app.handle_election_detail_key(KeyCode::Enter);
        app.handle_vote_key(KeyCode::Enter);
        app.handle_results_key(KeyCode::Esc);

        // Assert
        assert_eq!(app.screen, Screen::ElectionList);
        assert!(app.stv_ranking.is_empty());
        assert!(!app.editing_token);
    }

    #[test]
    fn request_voting_token_for_unknown_election_is_a_no_op() {
        // Arrange
        let (mut app, _rx, _dir) = isolated_app();

        // Act
        app.request_voting_token("missing");

        // Assert
        assert!(app.pending.is_none());
        assert!(app.pending_blind.is_none());
        assert!(app.error_message.is_none());
    }

    #[test]
    fn t_with_invalid_rsa_key_reports_prepare_failure() {
        // Arrange: registered + in-progress, but rsa_pub_key is not a real key
        let (mut app, _rx, _dir) = isolated_app();
        app.elections.insert(
            "e1".to_string(),
            plurality_election("e1", "General", ElectionStatus::InProgress),
        );
        app.persistent_state.mark_registered("e1".to_string());
        let mut cmd_rx = attach_cmd_channel(&mut app);
        app.screen = Screen::ElectionDetail {
            election_id: "e1".to_string(),
        };

        // Act
        press(&mut app, KeyCode::Char('t'));

        // Assert
        let error = app.error_message.expect("prepare failure surfaced");
        assert!(error.contains("Failed to prepare token request"));
        assert!(app.pending.is_none());
        assert!(app.pending_blind.is_none());
        assert!(cmd_rx.try_recv().is_err(), "no command may be sent");
    }

    #[test]
    fn token_issued_with_no_pending_blind_reports_missing_request() {
        // Arrange: a correlated pending request but no blinding secret held
        let (mut app, _rx, _dir) = isolated_app();
        set_pending(&mut app, PendingKind::RequestToken);

        // Act
        app.handle_ec_response(&EcResponse::Ok {
            action: "token-issued".to_string(),
            blind_signature: Some("sig".to_string()),
            request_id: Some("req-42".to_string()),
        });

        // Assert
        let error = app.error_message.expect("mismatch error surfaced");
        assert!(error.contains("no pending request"));
        assert!(app.persistent_state.get_active_token("e1").is_none());
    }

    #[test]
    fn save_state_failure_sets_error_message() {
        // Arrange: state path whose parent is a regular file inside the tempdir
        let (mut app, _rx, dir) = isolated_app();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"not a directory").expect("create blocker file");
        app.state_path = blocker.join("state.json");
        set_pending(&mut app, PendingKind::Register);

        // Act
        app.handle_ec_response(&EcResponse::Ok {
            action: "register-confirmed".to_string(),
            blind_signature: None,
            request_id: Some("req-42".to_string()),
        });

        // Assert: registration succeeded in memory but persistence failed loudly
        assert!(app.persistent_state.is_registered("e1"));
        let error = app.error_message.expect("save failure surfaced");
        assert!(error.contains("Failed to save state"));
    }
}
