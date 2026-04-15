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
}

pub fn build_unread_items(
    groups: &[GroupConfig],
    signals: &HashMap<SignalKey, SignalState>,
    pending_read: &HashSet<SignalKey>,
    target: &HoverPanelTarget,
) -> Vec<UnreadItemView> {
    let mut rows = Vec::new();
    let mut global_seen = HashSet::new();

    for group in groups.iter().filter(|g| g.enabled) {
        if let HoverPanelTarget::Group(target_group_id) = target {
            if &group.id != target_group_id {
                continue;
            }
        }

        for period in &group.periods {
            for signal_type in &group.signal_types {
                let key = SignalKey::new(&group.symbol, period, signal_type);
                if matches!(target, HoverPanelTarget::Global) && !global_seen.insert(key.clone()) {
                    continue;
                }
                let Some(sig) = signals.get(&key) else {
                    continue;
                };

                let pending = pending_read.contains(&key);
                let effective_read = sig.read || pending;
                if effective_read {
                    continue;
                }

                rows.push(UnreadItemView {
                    key: key.clone(),
                    group_id: group.id.clone(),
                    symbol: key.symbol.clone(),
                    period: key.period.clone(),
                    signal_type: key.signal_type.clone(),
                    side: Side::from_code(sig.sd),
                    trigger_time_ms: sig.t,
                });
            }
        }
    }

    rows.sort_by(|a, b| b.trigger_time_ms.cmp(&a.trigger_time_ms));
    rows
}

pub fn next_close_deadline_ms(
    trigger_hovered: bool,
    panel_hovered: bool,
    now_ms: i64,
    current_deadline_ms: Option<i64>,
    delay_ms: i64,
) -> Option<i64> {
    if trigger_hovered || panel_hovered {
        None
    } else if current_deadline_ms.is_some() {
        current_deadline_ms
    } else {
        Some(now_ms + delay_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    use crate::api::SignalState;
    use crate::config::GroupConfig;
    use crate::domain::SignalKey;

    fn group(id: &str, symbol: &str, periods: &[&str], signal_types: &[&str]) -> GroupConfig {
        GroupConfig {
            id: id.to_string(),
            name: id.to_string(),
            symbol: symbol.to_string(),
            periods: periods.iter().map(|v| (*v).to_string()).collect(),
            signal_types: signal_types.iter().map(|v| (*v).to_string()).collect(),
            enabled: true,
        }
    }

    #[test]
    fn global_contains_only_effective_unread_sorted_desc() {
        let groups = vec![
            group("g1", "BTCUSDT", &["15", "60"], &["vegas"]),
            group("g2", "ETHUSDT", &["15", "60"], &["vegas"]),
        ];
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
        let groups = vec![
            group("g1", "BTCUSDT", &["15"], &["vegas"]),
            group("g2", "BTCUSDT", &["60"], &["trend"]),
            group("g3", "ETHUSDT", &["15"], &["vegas"]),
        ];
        let mut signals = HashMap::new();
        let k1 = SignalKey::new("BTCUSDT", "15", "vegas"); // matches g1 only
        let k2 = SignalKey::new("BTCUSDT", "60", "trend"); // matches g2 only (same symbol, different group id)
        let k3 = SignalKey::new("ETHUSDT", "15", "vegas"); // matches g3
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

        let keep_open_from_panel = next_close_deadline_ms(false, true, 1050, deadline, 200);
        assert_eq!(keep_open_from_panel, None);

        let keep_open = next_close_deadline_ms(true, false, 1050, deadline, 200);
        assert_eq!(keep_open, None);
    }

    #[test]
    fn global_dedupes_duplicate_signal_keys_across_groups() {
        let groups = vec![
            group("g1", "BTCUSDT", &["15"], &["vegas"]),
            group("g2", "BTCUSDT", &["15"], &["vegas"]),
        ];
        let mut signals = HashMap::new();
        let key = SignalKey::new("BTCUSDT", "15", "vegas");
        signals.insert(
            key.clone(),
            SignalState {
                sd: 1,
                t: 300,
                read: false,
            },
        );

        let global_rows = build_unread_items(
            &groups,
            &signals,
            &HashSet::new(),
            &HoverPanelTarget::Global,
        );
        assert_eq!(global_rows.len(), 1);
        assert_eq!(global_rows[0].key, key);

        let g1_rows = build_unread_items(
            &groups,
            &signals,
            &HashSet::new(),
            &HoverPanelTarget::Group("g1".into()),
        );
        let g2_rows = build_unread_items(
            &groups,
            &signals,
            &HashSet::new(),
            &HoverPanelTarget::Group("g2".into()),
        );
        assert_eq!(g1_rows.len(), 1);
        assert_eq!(g2_rows.len(), 1);
    }
}
