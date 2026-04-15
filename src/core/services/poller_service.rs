use std::collections::BTreeSet;

use crate::api::FetchSignalsRequest;
use crate::config::{AppConfig, GroupConfig};

pub(crate) fn build_request(config: &AppConfig) -> Option<FetchSignalsRequest> {
    let enabled: Vec<&GroupConfig> = config.groups.iter().filter(|g| g.enabled).collect();
    if enabled.is_empty() {
        return None;
    }

    let symbols = join_unique(enabled.iter().map(|g| g.symbol.clone()));
    let periods = join_unique(enabled.iter().flat_map(|g| g.periods.clone()));
    let signal_types = join_unique(enabled.iter().flat_map(|g| g.signal_types.clone()));

    Some(FetchSignalsRequest {
        symbols,
        periods: if periods.is_empty() {
            None
        } else {
            Some(periods)
        },
        signal_types: if signal_types.is_empty() {
            None
        } else {
            Some(signal_types)
        },
        page: Some(1),
        page_size: Some(config.poll.page_size.min(100)),
    })
}

pub(crate) fn join_unique<I>(iter: I) -> String
where
    I: IntoIterator<Item = String>,
{
    let set: BTreeSet<String> = iter
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    set.into_iter().collect::<Vec<_>>().join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    #[test]
    fn build_request_skips_when_no_enabled_groups() {
        let mut config = AppConfig::default();
        config.groups.iter_mut().for_each(|g| g.enabled = false);
        assert!(build_request(&config).is_none());
    }
}
