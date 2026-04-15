use std::collections::{HashMap, HashSet};

use crate::api::SignalState;
use crate::domain::SignalKey;

use super::contract::AppSnapshot;

#[derive(Debug, Default, Clone)]
pub struct AppState {
    pub signals: HashMap<SignalKey, SignalState>,
    pub pending_read: HashSet<SignalKey>,
    pub last_poll_error: Option<String>,
}

impl AppState {
    pub fn to_snapshot(&self) -> AppSnapshot {
        let unread_count = self
            .signals
            .iter()
            .filter(|(key, sig)| !sig.read && !self.pending_read.contains(*key))
            .count();
        AppSnapshot {
            unread_count,
            last_poll_error: self.last_poll_error.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_includes_unread_count() {
        let state = AppState::default();
        let snapshot = state.to_snapshot();
        assert_eq!(snapshot.unread_count, 0);
    }
}
