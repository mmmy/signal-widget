use std::collections::{HashMap, HashSet};

use crate::api::SignalState;
use crate::domain::SignalKey;

pub fn effective_unread_keys(
    signals: &HashMap<SignalKey, SignalState>,
    pending_read: &HashSet<SignalKey>,
) -> HashSet<SignalKey> {
    signals
        .iter()
        .filter(|(key, state)| !state.read && !pending_read.contains(*key))
        .map(|(key, _)| key.clone())
        .collect()
}

pub fn collect_new_unread_keys(
    previous_unread: &HashSet<SignalKey>,
    current_unread: &HashSet<SignalKey>,
) -> Vec<SignalKey> {
    let mut keys: Vec<SignalKey> = current_unread
        .difference(previous_unread)
        .cloned()
        .collect();
    keys.sort_by(|a, b| {
        a.symbol
            .cmp(&b.symbol)
            .then(a.period.cmp(&b.period))
            .then(a.signal_type.cmp(&b.signal_type))
    });
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::SignalState;
    use crate::domain::SignalKey;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn effective_unread_filters_read_and_pending() {
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
        let pending = HashSet::new();
        let unread = effective_unread_keys(&signals, &pending);
        assert_eq!(unread.len(), 1);
    }
}
