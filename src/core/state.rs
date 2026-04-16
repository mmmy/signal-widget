use std::collections::{HashMap, HashSet};

use crate::api::{SignalPage, SignalState};
use crate::domain::SignalKey;

use super::queries::unread::effective_unread_keys;
use super::contract::AppSnapshot;

#[derive(Debug, Default, Clone)]
pub struct AppState {
    pub signals: HashMap<SignalKey, SignalState>,
    pub pending_read: HashSet<SignalKey>,
    pub local_read_floor_t: HashMap<SignalKey, i64>,
    pub last_poll_ms: Option<i64>,
    pub last_poll_ok: Option<bool>,
    pub consecutive_poll_failures: u32,
    pub last_meta: Option<(u64, u32, u32)>,
    pub last_poll_error: Option<String>,
    pub last_error: Option<String>,
}

impl AppState {
    pub fn to_snapshot(&self) -> AppSnapshot {
        let unread_count = effective_unread_keys(&self.signals, &self.pending_read).len();
        AppSnapshot {
            signals: self.signals.clone(),
            pending_read: self.pending_read.clone(),
            unread_count,
            last_poll_ms: self.last_poll_ms,
            last_poll_ok: self.last_poll_ok,
            consecutive_poll_failures: self.consecutive_poll_failures,
            last_meta: self.last_meta,
            last_poll_error: self.last_poll_error.clone(),
            last_error: self.last_error.clone(),
        }
    }

    pub fn apply_snapshot(&mut self, fetched_at_ms: i64, page: &SignalPage) {
        let mut next = HashMap::new();
        for row in &page.data {
            for (signal_type, state) in &row.signals {
                let key = SignalKey::new(row.symbol.clone(), row.period.clone(), signal_type.clone());
                let mut next_state = state.clone();
                if let Some(&floor_t) = self.local_read_floor_t.get(&key) {
                    if next_state.t <= floor_t {
                        next_state.read = true;
                    } else {
                        self.local_read_floor_t.remove(&key);
                    }
                }
                next.insert(key, next_state);
            }
        }

        self.signals = next;
        self.last_poll_ms = Some(fetched_at_ms);
        self.last_meta = Some((page.total, page.page, page.page_size));
        self.consecutive_poll_failures = 0;
        self.last_poll_ok = Some(true);
        self.last_poll_error = None;
        self.last_error = None;
    }

    pub fn apply_poll_failed(&mut self, error: String) {
        self.consecutive_poll_failures = self.consecutive_poll_failures.saturating_add(1);
        self.last_poll_ok = Some(self.consecutive_poll_failures < 2);
        self.last_poll_error = Some(error.clone());
        self.last_error = Some(error);
    }

    pub fn apply_mark_read_request(&mut self, key: &SignalKey, read: bool) {
        if read {
            if let Some(signal_t) = self.signals.get(key).map(|signal| signal.t) {
                self.local_read_floor_t.insert(key.clone(), signal_t);
            }
            if let Some(signal) = self.signals.get_mut(key) {
                signal.read = true;
            }
            self.pending_read.insert(key.clone());
        } else {
            self.pending_read.remove(key);
            self.local_read_floor_t.remove(key);
            if let Some(signal) = self.signals.get_mut(key) {
                signal.read = false;
            }
        }
    }

    pub fn apply_mark_read_synced(&mut self, key: &SignalKey) {
        self.pending_read.remove(key);
    }

    pub fn apply_sync_failed(&mut self, key: &SignalKey, error: String) {
        let was_pending = self.pending_read.remove(key);
        self.local_read_floor_t.remove(key);
        if was_pending {
            if let Some(signal) = self.signals.get_mut(key) {
                signal.read = false;
            }
        }
        self.last_error = Some(format!(
            "sync failed [{} {} {}]: {}",
            key.symbol, key.period, key.signal_type, error
        ));
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
            ..Default::default()
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

    #[test]
    fn apply_mark_read_request_marks_signal_and_tracks_pending() {
        let key = SignalKey::new("BTCUSDT", "15", "vegas");
        let mut state = AppState {
            signals: HashMap::from([(
                key.clone(),
                SignalState {
                    sd: 1,
                    t: 100,
                    read: false,
                },
            )]),
            ..Default::default()
        };

        state.apply_mark_read_request(&key, true);

        assert!(state.pending_read.contains(&key));
        assert!(state.signals.get(&key).is_some_and(|signal| signal.read));
        assert_eq!(state.local_read_floor_t.get(&key), Some(&100));
    }
}
