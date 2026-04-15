use crate::config::UiConfig;
use crate::domain::SignalKey;
use crate::protocol::AppSnapshot;

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
