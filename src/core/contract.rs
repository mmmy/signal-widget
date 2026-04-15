#![allow(dead_code)]

use crate::config::UiConfig;
use crate::domain::SignalKey;

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

impl PartialEq for AppCommand {
    fn eq(&self, other: &Self) -> bool {
        use AppCommand::*;

        match (self, other) {
            (ForcePoll, ForcePoll) => true,
            (MarkRead { key: left_key, read: left_read }, MarkRead { key: right_key, read: right_read }) => {
                left_key == right_key && left_read == right_read
            }
            (MarkGroupRead { group_id: left_group_id }, MarkGroupRead { group_id: right_group_id }) => {
                left_group_id == right_group_id
            }
            (SaveUiConfig { ui: left_ui }, SaveUiConfig { ui: right_ui }) => {
                left_ui.edge_mode == right_ui.edge_mode
                    && left_ui.edge_width == right_ui.edge_width
                    && left_ui.always_on_top == right_ui.always_on_top
                    && left_ui.notifications == right_ui.notifications
                    && left_ui.sound == right_ui.sound
            }
            (RequestCloseMainWindow, RequestCloseMainWindow) => true,
            (RequestShowMainWindow, RequestShowMainWindow) => true,
            (RequestExitApp, RequestExitApp) => true,
            _ => false,
        }
    }
}

impl Eq for AppCommand {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiAction {
    ShowMainWindow,
    HideMainWindowToTray,
    ExitProcess,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
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
