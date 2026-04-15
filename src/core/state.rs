use std::collections::{HashMap, HashSet};

use crate::api::SignalState;
use crate::domain::SignalKey;

use super::queries::unread::effective_unread_keys;
use super::contract::AppSnapshot;

#[derive(Debug, Default, Clone)]
pub struct AppState {
    pub signals: HashMap<SignalKey, SignalState>,
    pub pending_read: HashSet<SignalKey>,
    pub last_poll_error: Option<String>,
}

impl AppState {
    pub fn to_snapshot(&self) -> AppSnapshot {
        let unread_count = effective_unread_keys(&self.signals, &self.pending_read).len();
        AppSnapshot {
            unread_count,
            last_poll_error: self.last_poll_error.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn snapshot_includes_unread_count() {
        let state = AppState::default();
        let snapshot = state.to_snapshot();
        assert_eq!(snapshot.unread_count, 0);
    }

    #[test]
    fn snapshot_excludes_pending_read_entries_from_unread_count() {
        let key = SignalKey::new("BTCUSDT", "15", "vegas");
        let mut signals = HashMap::new();
        signals.insert(
            key.clone(),
            SignalState {
                sd: 1,
                t: 1,
                read: false,
            },
        );

        let state = AppState {
            signals,
            pending_read: HashSet::from([key]),
            last_poll_error: None,
        };

        let snapshot = state.to_snapshot();
        assert_eq!(snapshot.unread_count, 0);
    }

    #[test]
    fn snapshot_propagates_last_poll_error() {
        let state = AppState {
            last_poll_error: Some("poll failed".to_string()),
            ..Default::default()
        };

        let snapshot = state.to_snapshot();
        assert_eq!(snapshot.last_poll_error.as_deref(), Some("poll failed"));
    }
}
