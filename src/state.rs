use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::crypto::token::VotingToken;
use crate::error::{Result, VoterError};

/// Local persistent state (registrations and voting tokens).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppState {
    #[serde(default)]
    pub registrations: HashMap<String, VoterRegistration>,
    #[serde(default)]
    pub tokens: HashMap<String, VotingToken>,
}

/// Registration status for a specific election.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoterRegistration {
    pub registered: bool,
    pub registered_at: String,
}

impl AppState {
    /// Load state from disk. Returns default state if file doesn't exist.
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let contents = std::fs::read_to_string(path)?;
            let state: AppState = serde_json::from_str(&contents)?;
            Ok(state)
        } else {
            Ok(AppState::default())
        }
    }

    /// Save state to disk, creating parent directories if needed.
    /// Written owner-only (0600 on Unix): the state holds voting tokens,
    /// which are bearer credentials — anyone who reads one can cast the vote.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string_pretty(self)?;
        crate::identity::write_secret_file(path, contents.as_bytes())?;
        Ok(())
    }

    /// Check if the voter is registered for an election.
    pub fn is_registered(&self, election_id: &str) -> bool {
        self.registrations
            .get(election_id)
            .is_some_and(|r| r.registered)
    }

    /// Record a successful registration.
    #[allow(dead_code)]
    pub fn mark_registered(&mut self, election_id: String) {
        self.registrations.insert(
            election_id,
            VoterRegistration {
                registered: true,
                registered_at: chrono_now(),
            },
        );
    }

    /// Store a voting token for an election.
    #[allow(dead_code)]
    pub fn store_token(&mut self, election_id: String, token: VotingToken) {
        self.tokens.insert(election_id, token);
    }

    /// Get a token for an election, if one exists and is not consumed.
    pub fn get_active_token(&self, election_id: &str) -> Option<&VotingToken> {
        self.tokens.get(election_id).filter(|t| !t.consumed)
    }

    /// Mark a token as consumed after a successful vote.
    #[allow(dead_code)]
    pub fn consume_token(&mut self, election_id: &str) -> Result<()> {
        let token = self
            .tokens
            .get_mut(election_id)
            .ok_or_else(|| VoterError::State(format!("no token for election {election_id}")))?;
        if token.consumed {
            return Err(VoterError::State(format!(
                "token for election {election_id} already consumed"
            )));
        }
        token.consumed = true;
        Ok(())
    }

    /// Check if the voter has already voted in an election.
    pub fn has_voted(&self, election_id: &str) -> bool {
        self.tokens.get(election_id).is_some_and(|t| t.consumed)
    }
}

fn chrono_now() -> String {
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    duration.as_secs().to_string()
}
