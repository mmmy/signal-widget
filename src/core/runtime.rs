use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use anyhow::Context as _;
use eframe::egui;

use crate::core::contract::{AdapterId, AppCommand, AppEvent, UiAction};
use crate::core::policy::window_lifecycle::close_action_for_request;
use crate::core::state::AppState;
use crate::poller::PollerCommand;

enum RuntimeCommand {
    App(AppCommand),
    SetTrayAvailable(bool),
    Shutdown,
}

#[derive(Clone)]
pub struct RuntimeHandle {
    command_tx: mpsc::Sender<RuntimeCommand>,
    event_tx: mpsc::Sender<AppEvent>,
}

impl RuntimeHandle {
    fn new(command_tx: mpsc::Sender<RuntimeCommand>, event_tx: mpsc::Sender<AppEvent>) -> Self {
        Self { command_tx, event_tx }
    }

    pub fn send(&self, cmd: AppCommand) -> anyhow::Result<()> {
        self.command_tx
            .send(RuntimeCommand::App(cmd))
            .context("send command failed")
    }

    pub fn emit(&self, event: AppEvent) -> anyhow::Result<()> {
        self.event_tx.send(event).context("send event failed")
    }

    pub fn set_tray_available(&self, available: bool) -> anyhow::Result<()> {
        self.command_tx
            .send(RuntimeCommand::SetTrayAvailable(available))
            .context("set tray availability failed")
    }
}

pub struct Runtime {
    pub state: AppState,
    command_tx: mpsc::Sender<RuntimeCommand>,
    join: Option<JoinHandle<()>>,
}

impl Runtime {
    pub fn spawn(
        repaint_ctx: egui::Context,
        poller_command_tx: mpsc::Sender<PollerCommand>,
    ) -> (Self, RuntimeHandle, mpsc::Receiver<AppEvent>) {
        let (command_tx, command_rx) = mpsc::channel::<RuntimeCommand>();
        let (event_tx, event_rx) = mpsc::channel::<AppEvent>();
        let handle = RuntimeHandle::new(command_tx.clone(), event_tx.clone());
        let join = thread::spawn(move || {
            let mut tray_available = false;

            while let Ok(command) = command_rx.recv() {
                match command {
                    RuntimeCommand::App(app_command) => {
                        handle_app_command(
                            app_command,
                            tray_available,
                            &event_tx,
                            &poller_command_tx,
                        );
                        repaint_ctx.request_repaint();
                    }
                    RuntimeCommand::SetTrayAvailable(available) => {
                        tray_available = available;
                    }
                    RuntimeCommand::Shutdown => break,
                }
            }
        });

        (
            Self {
                state: AppState::default(),
                command_tx,
                join: Some(join),
            },
            handle,
            event_rx,
        )
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.command_tx.send(RuntimeCommand::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn handle_app_command(
    command: AppCommand,
    tray_available: bool,
    event_tx: &mpsc::Sender<AppEvent>,
    poller_command_tx: &mpsc::Sender<PollerCommand>,
) {
    match command {
        AppCommand::ForcePoll => {
            let _ = poller_command_tx.send(PollerCommand::ForcePoll);
        }
        AppCommand::MarkRead { key, read } => {
            let _ = poller_command_tx.send(PollerCommand::MarkRead { key, read });
        }
        AppCommand::RequestCloseMainWindow => {
            if let Some(action) = close_action_for_request(true, false, tray_available) {
                let ui_action = match action {
                    crate::core::policy::window_lifecycle::CloseAction::MinimizeToTray => {
                        UiAction::HideMainWindowToTray
                    }
                    crate::core::policy::window_lifecycle::CloseAction::CloseApp => {
                        UiAction::ExitProcess
                    }
                };
                let _ = event_tx.send(AppEvent::AdapterAction {
                    target: AdapterId::MainWindow,
                    action: ui_action,
                });
            }
        }
        AppCommand::RequestShowMainWindow => {
            let _ = event_tx.send(AppEvent::AdapterAction {
                target: AdapterId::MainWindow,
                action: UiAction::ShowMainWindow,
            });
        }
        AppCommand::RequestExitApp => {
            let _ = event_tx.send(AppEvent::AdapterAction {
                target: AdapterId::MainWindow,
                action: UiAction::ExitProcess,
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::Receiver;

    use crate::domain::SignalKey;

    fn test_handle() -> (RuntimeHandle, Receiver<AppEvent>) {
        let (command_tx, command_rx) = mpsc::channel::<RuntimeCommand>();
        let (event_tx, event_rx) = mpsc::channel::<AppEvent>();
        let handle = RuntimeHandle::new(command_tx, event_tx);
        // Keep receivers alive for the duration of the test helper.
        std::mem::forget(command_rx);
        (handle, event_rx)
    }

    #[test]
    fn runtime_accepts_force_poll_command() {
        let (handle, _event_rx) = test_handle();
        handle.send(AppCommand::ForcePoll).expect("send command");
    }

    #[test]
    fn runtime_emits_snapshot_event() {
        let (handle, _event_rx) = test_handle();
        handle
            .emit(AppEvent::SnapshotUpdated(Default::default()))
            .expect("emit event");
    }

    #[test]
    fn runtime_forwards_force_poll_to_poller() {
        let (poller_tx, poller_rx) = mpsc::channel::<PollerCommand>();
        let (_runtime, handle, _event_rx) = Runtime::spawn(egui::Context::default(), poller_tx);

        handle.send(AppCommand::ForcePoll).expect("send command");

        let command = poller_rx.recv().expect("poller command");
        assert!(matches!(command, PollerCommand::ForcePoll));
    }

    #[test]
    fn runtime_forwards_mark_read_to_poller() {
        let (poller_tx, poller_rx) = mpsc::channel::<PollerCommand>();
        let (_runtime, handle, _event_rx) = Runtime::spawn(egui::Context::default(), poller_tx);
        let key = SignalKey::new("BTCUSDT", "15", "vegas");

        handle
            .send(AppCommand::MarkRead {
                key: key.clone(),
                read: true,
            })
            .expect("send command");

        let command = poller_rx.recv().expect("poller command");
        assert!(matches!(
            command,
            PollerCommand::MarkRead {
                key: actual,
                read: true
            } if actual == key
        ));
    }

    #[test]
    fn runtime_emits_show_main_window_action() {
        let (poller_tx, _poller_rx) = mpsc::channel::<PollerCommand>();
        let (_runtime, handle, event_rx) = Runtime::spawn(egui::Context::default(), poller_tx);
        handle
            .set_tray_available(true)
            .expect("set tray availability");
        handle
            .send(AppCommand::RequestShowMainWindow)
            .expect("send command");

        let event = event_rx.recv().expect("runtime event");
        assert!(matches!(
            event,
            AppEvent::AdapterAction {
                target: AdapterId::MainWindow,
                action: UiAction::ShowMainWindow
            }
        ));
    }

    #[test]
    fn runtime_emits_hide_to_tray_when_tray_is_available() {
        let (poller_tx, _poller_rx) = mpsc::channel::<PollerCommand>();
        let (_runtime, handle, event_rx) = Runtime::spawn(egui::Context::default(), poller_tx);
        handle
            .set_tray_available(true)
            .expect("set tray availability");
        handle
            .send(AppCommand::RequestCloseMainWindow)
            .expect("send command");

        let event = event_rx.recv().expect("runtime event");
        assert!(matches!(
            event,
            AppEvent::AdapterAction {
                target: AdapterId::MainWindow,
                action: UiAction::HideMainWindowToTray
            }
        ));
    }
}
