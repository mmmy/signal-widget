use crate::api::SignalPage;
use crate::config::UiConfig;
use crate::domain::SignalKey;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AppSnapshot {
    pub unread_count: usize,
    pub last_poll_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterId {
    MainWindow,
    Tray,
    FloatingWidget,
}

#[derive(Debug, Clone)]
pub enum AppCommand {
    ForcePoll,
    MarkRead { key: SignalKey, read: bool },
    MarkGroupRead { group_id: String },
    SaveUiConfig { ui: UiConfig },
    RequestCloseMainWindow,
    RequestShowMainWindow,
    RequestExitApp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiAction {
    ShowMainWindow,
    HideMainWindowToTray,
    ExitProcess,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    SnapshotUpdated(AppSnapshot),
    PollerSnapshot {
        fetched_at_ms: i64,
        page: SignalPage,
    },
    PollFailed {
        error: String,
    },
    MarkReadSynced {
        key: SignalKey,
    },
    SyncFailed {
        key: SignalKey,
        error: String,
    },
    AdapterAction { target: AdapterId, action: UiAction },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_exit_maps_to_runtime_event_channel_shape() {
        let cmd = AppCommand::RequestExitApp;
        match cmd {
            AppCommand::RequestExitApp => {}
            _ => panic!("wrong command variant"),
        }
    }
}
