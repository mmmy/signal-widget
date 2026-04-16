use std::collections::{HashMap, HashSet};

use crate::api::{SignalPage, SignalState};
use crate::config::UiConfig;
use crate::domain::SignalKey;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AppSnapshot {
    pub signals: HashMap<SignalKey, SignalState>,
    pub pending_read: HashSet<SignalKey>,
    pub unread_count: usize,
    pub last_poll_ms: Option<i64>,
    pub last_poll_ok: Option<bool>,
    pub consecutive_poll_failures: u32,
    pub last_meta: Option<(u64, u32, u32)>,
    pub last_poll_error: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowId {
    Main,
    Widget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellCommand {
    ShowWindow(WindowId),
    HideWindow(WindowId),
    FocusWindow(WindowId),
    ExitProcess,
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
    RequestShowWidget,
    RequestHideWidget,
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
    WidgetVisibilityChanged { visible: bool },
    ShellCommand(ShellCommand),
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
