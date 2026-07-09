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
    next_request_id: u64,
}

impl App {
    pub fn new(
        config: AppConfig,
        persistent_state: AppState,
        action_tx: mpsc::UnboundedSender<Action>,
    ) -> Self {
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
            next_request_id: 0,
        }
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
                let keys = voter::identity::generate_keypair();
                let path = self.config.identity.path.clone();
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
                    if !self.token_input.is_empty() {
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

        self.next_request_id += 1;
        let request_id = self.next_request_id;
        let cmd = if anonymous {
            VoterCommand::SendAnonymous {
                request_id,
                ec_pubkey,
                msg,
            }
        } else {
            VoterCommand::Send {
                request_id,
                ec_pubkey,
                msg,
            }
        };

        if cmd_tx.send(cmd).is_err() {
            self.error_message = Some("Connection task is not running".to_string());
            return;
        }

        self.pending = Some(PendingRequest {
            id: request_id,
            election_id: election_id.to_string(),
            kind,
        });
        self.is_loading = true;
        self.loading_step = Some(loading_step.to_string());
        self.error_message = None;
    }

    fn submit_registration(&mut self, election_id: &str) {
        let registration_token = self.token_input.trim().to_string();
        self.editing_token = false;
        self.token_input.clear();
        let msg = VoterMessage::Register {
            election_id: election_id.to_string(),
            registration_token,
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
                let msg = VoterMessage::RequestToken {
                    election_id: election_id.to_string(),
                    blinded_nonce,
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
        let msg = VoterMessage::CastVote {
            election_id: election_id.to_string(),
            candidate_ids: self.stv_ranking.clone(),
            h_n: voting_token.h_n.clone(),
            token: wire_token,
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
        // Only a response whose action matches the in-flight request may
        // consume the pending state. Relays can replay historical Gift Wraps
        // on reconnect; letting a stale "register-confirmed" swallow a pending
        // token request would drop the real blind signature when it arrives —
        // and the EC has already burned this voter's only token slot.
        let expected_action = |kind: PendingKind| match kind {
            PendingKind::Register => "register-confirmed",
            PendingKind::RequestToken => "token-issued",
            PendingKind::CastVote => "vote-recorded",
        };

        match response {
            EcResponse::Ok {
                action,
                blind_signature,
            } => {
                let pending = self
                    .pending
                    .take_if(|p| expected_action(p.kind) == action.as_str());
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
                        let Some(pending_blind) = self.pending_blind.take() else {
                            self.error_message =
                                Some("Received a token with no pending request".to_string());
                            return;
                        };
                        let Some(sig_b64) = blind_signature else {
                            self.error_message =
                                Some("EC response missing blind signature".to_string());
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
            EcResponse::Error { .. } => {
                // EC errors carry no action field, so they cannot be
                // correlated; treat any error as failing the in-flight
                // request (if one exists).
                if self.pending.take().is_some() {
                    self.is_loading = false;
                    self.loading_step = None;
                }
                self.pending_blind = None;
                self.error_message = Some(format_ec_response(response));
            }
        }
    }

    fn save_state(&mut self) {
        let path = self.config.state_path();
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
        } => {
            if blind_signature.is_some() {
                format!("EC: {action} (signature received)")
            } else {
                format!("EC: {action}")
            }
        }
        EcResponse::Error { code, message } => {
            format!("EC error: {code} — {message}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let (tx, _rx) = mpsc::unbounded_channel();
        App::new(AppConfig::default(), AppState::default(), tx)
    }

    fn set_pending(app: &mut App, kind: PendingKind) -> u64 {
        app.pending = Some(PendingRequest {
            id: 42,
            election_id: "e1".to_string(),
            kind,
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
        });

        assert!(app.pending.is_some(), "pending must survive a stale reply");
        assert!(app.is_loading, "still waiting for the real reply");
    }

    /// An EC error fails the in-flight request (errors carry no action field,
    /// so they cannot be correlated more precisely).
    #[test]
    fn error_response_clears_pending() {
        let mut app = test_app();
        set_pending(&mut app, PendingKind::Register);

        app.handle_ec_response(&EcResponse::Error {
            code: voter::nostr::messages::EcErrorCode::InvalidToken,
            message: "bad token".to_string(),
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
}
