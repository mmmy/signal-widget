use std::collections::{HashMap, HashSet};

use crate::api::SignalState;
use crate::config::GroupConfig;
use crate::domain::{Side, SignalKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoverPanelTarget {
    Global,
    Group(String),
}

#[derive(Debug, Clone)]
pub struct HoverPanelState {
    pub target: HoverPanelTarget,
    pub close_deadline_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct UnreadItemView {
    pub key: SignalKey,
    pub group_id: String,
    pub symbol: String,
    pub period: String,
    pub signal_type: String,
    pub side: Side,
    pub trigger_time_ms: i64,
    pub pending: bool,
}

pub fn build_unread_items(
    _groups: &[GroupConfig],
    _signals: &HashMap<SignalKey, SignalState>,
    _pending_read: &HashSet<SignalKey>,
    _target: &HoverPanelTarget,
) -> Vec<UnreadItemView> {
    panic!("red phase: build_unread_items not implemented");
}

pub fn next_close_deadline_ms(
    _trigger_hovered: bool,
    _panel_hovered: bool,
    _now_ms: i64,
    _current_deadline_ms: Option<i64>,
    _delay_ms: i64,
) -> Option<i64> {
    panic!("red phase: next_close_deadline_ms not implemented");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    use crate::api::SignalState;
    use crate::config::GroupConfig;
    use crate::domain::SignalKey;

    fn group(id: &str, symbol: &str) -> GroupConfig {
        GroupConfig {
            id: id.to_string(),
            name: id.to_string(),
            symbol: symbol.to_string(),
            periods: vec!["15".into(), "60".into()],
            signal_types: vec!["vegas".into()],
            enabled: true,
        }
    }

    #[test]
    fn global_contains_only_effective_unread_sorted_desc() {
        let groups = vec![group("g1", "BTCUSDT"), group("g2", "ETHUSDT")];
        let mut signals = HashMap::new();
        let k1 = SignalKey::new("BTCUSDT", "15", "vegas");
        let k2 = SignalKey::new("ETHUSDT", "15", "vegas");
        let k3 = SignalKey::new("BTCUSDT", "60", "vegas");
        let k4 = SignalKey::new("ETHUSDT", "60", "vegas");

        signals.insert(
            k1.clone(),
            SignalState {
                sd: 1,
                t: 300,
                read: false,
            },
        );
        signals.insert(
            k2.clone(),
            SignalState {
                sd: -1,
                t: 200,
                read: false,
            },
        );
        signals.insert(
            k3.clone(),
            SignalState {
                sd: 1,
                t: 100,
                read: true,
            },
        );
        signals.insert(
            k4.clone(),
            SignalState {
                sd: -1,
                t: 250,
                read: false,
            },
        );

        let mut pending = HashSet::new();
        pending.insert(k2.clone());

        let rows = build_unread_items(&groups, &signals, &pending, &HoverPanelTarget::Global);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].key, k1);
        assert_eq!(rows[1].key, k4);
        assert!(rows[0].trigger_time_ms >= rows[1].trigger_time_ms);
    }

    #[test]
    fn group_target_filters_to_single_group() {
        let groups = vec![group("g1", "BTCUSDT"), group("g2", "ETHUSDT")];
        let mut signals = HashMap::new();
        let k1 = SignalKey::new("BTCUSDT", "15", "vegas");
        let k2 = SignalKey::new("ETHUSDT", "15", "vegas");
        signals.insert(
            k1.clone(),
            SignalState {
                sd: 1,
                t: 300,
                read: false,
            },
        );
        signals.insert(
            k2.clone(),
            SignalState {
                sd: -1,
                t: 200,
                read: false,
            },
        );

        let rows = build_unread_items(
            &groups,
            &signals,
            &HashSet::new(),
            &HoverPanelTarget::Group("g1".into()),
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, k1);
    }

    #[test]
    fn close_deadline_set_and_cleared_by_hover_state() {
        let deadline = next_close_deadline_ms(false, false, 1000, None, 200);
        assert_eq!(deadline, Some(1200));

        let preserved = next_close_deadline_ms(false, false, 1050, deadline, 200);
        assert_eq!(preserved, deadline);

        let keep_open = next_close_deadline_ms(true, false, 1050, deadline, 200);
        assert_eq!(keep_open, None);
    }
}
