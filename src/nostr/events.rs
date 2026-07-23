use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// An election parsed from a Kind 35000 Nostr event.
///
/// Field types must match what the EC daemon actually publishes:
/// unix timestamps as integers and snake_case status strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Election {
    pub election_id: String,
    pub name: String,
    /// Unix timestamp (seconds).
    pub start_time: i64,
    /// Unix timestamp (seconds).
    pub end_time: i64,
    pub status: ElectionStatus,
    pub rules_id: String,
    /// Base64 DER-encoded RSA public key for blind signing.
    pub rsa_pub_key: String,
    pub candidates: Vec<Candidate>,
    /// Nostr pubkey of the EC that published this election (not from JSON).
    #[serde(skip)]
    pub ec_pubkey: Option<String>,
    /// `created_at` of the event that carried this election, in unix seconds
    /// (not from JSON). Announcements are replaceable and the EC republishes
    /// them on every change, so this orders versions and lets a stale replay
    /// from a lagging relay be discarded.
    #[serde(skip)]
    pub event_created_at: Option<u64>,
    /// Hex id of the event that carried this election (not from JSON). Breaks
    /// ties between two announcements published within the same second.
    #[serde(skip)]
    pub event_id: Option<String>,
}

impl Election {
    /// Whether this announcement should replace `current`, following the
    /// NIP-01 ordering for replaceable events: the greater `created_at` wins
    /// and, on a tie, the lowest event id is retained.
    ///
    /// Announcements without event metadata (values built locally rather than
    /// received from a relay) always replace, so no update is silently lost.
    pub fn supersedes(&self, current: &Self) -> bool {
        let (Some(incoming_at), Some(current_at)) =
            (self.event_created_at, current.event_created_at)
        else {
            return true;
        };

        match incoming_at.cmp(&current_at) {
            Ordering::Greater => true,
            Ordering::Less => false,
            // Same second: NIP-01 keeps the lowest id. An identical id is the
            // same event arriving twice, so replacing is a no-op.
            Ordering::Equal => match (&self.event_id, &current.event_id) {
                (Some(incoming_id), Some(current_id)) => incoming_id <= current_id,
                _ => true,
            },
        }
    }
}

/// Election status as published by the EC (`"open"`, `"in_progress"`,
/// `"finished"`, `"cancelled"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElectionStatus {
    Open,
    InProgress,
    Finished,
    Cancelled,
}

impl std::fmt::Display for ElectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "Open"),
            Self::InProgress => write!(f, "In Progress"),
            Self::Finished => write!(f, "Finished"),
            Self::Cancelled => write!(f, "Cancelled"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub id: u32,
    pub name: String,
}

/// Election results parsed from a Kind 35001 Nostr event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElectionResults {
    pub election_id: String,
    pub elected: Vec<u32>,
    pub tally: Vec<TallyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TallyEntry {
    pub candidate_id: u32,
    /// Vote total. Fractional under STV (weighted surplus transfers).
    pub votes: f64,
}

/// Format a unix timestamp (seconds) as a human-readable UTC datetime
/// (`YYYY-MM-DD HH:MM UTC`). Used by the TUI to display election times.
pub fn format_unix_utc(ts: i64) -> String {
    let days = ts.div_euclid(86_400);
    let secs_of_day = ts.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

/// Convert days since 1970-01-01 to a (year, month, day) civil date.
/// Algorithm from Howard Hinnant's date algorithms (public domain).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // year of era
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // month index [0, 11] starting in March
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
