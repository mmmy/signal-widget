use std::path::PathBuf;
use std::sync::mpsc;

use crate::config::AppConfig;
use crate::core::contract::AppEvent as RuntimeEvent;
use crate::core::runtime::RuntimeHandle;
use crate::poller::PollerHandle;

pub struct MainWindowApp {
    pub has_seen_snapshot: bool,
}

impl MainWindowApp {
    pub fn new(
        _config: AppConfig,
        _config_path: PathBuf,
        _poller: PollerHandle,
        _runtime_handle: RuntimeHandle,
        _runtime_event_rx: mpsc::Receiver<RuntimeEvent>,
    ) -> Self {
        Self {
            has_seen_snapshot: false,
        }
    }

    pub fn new_for_test() -> Self {
        Self {
            has_seen_snapshot: false,
        }
    }

    pub fn on_runtime_event(&mut self, event: RuntimeEvent) {
        if matches!(event, RuntimeEvent::SnapshotUpdated(_)) {
            self.has_seen_snapshot = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drains_runtime_snapshot_into_presenter_state() {
        let mut app = MainWindowApp::new_for_test();
        app.on_runtime_event(RuntimeEvent::SnapshotUpdated(Default::default()));
        assert!(app.has_seen_snapshot);
    }
}
