use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::Context as _;
use eframe::egui;
use tokio::sync::{broadcast, watch};

use crate::core::contract::{AppCommand, AppEvent, AppSnapshot, ShellCommand, WindowId};
use crate::core::policy::window_lifecycle::close_action_for_request;
use crate::core::state::AppState;
use crate::poller::{PollerCommand, PollerEvent};

enum RuntimeCommand {
    App(AppCommand),
    SetTrayAvailable(bool),
    Shutdown,
}

#[derive(Clone)]
pub struct RuntimeHandle {
    command_tx: mpsc::Sender<RuntimeCommand>,
    event_tx: broadcast::Sender<AppEvent>,
    snapshot_rx: watch::Receiver<AppSnapshot>,
}

impl RuntimeHandle {
    fn new(
        command_tx: mpsc::Sender<RuntimeCommand>,
        event_tx: broadcast::Sender<AppEvent>,
        snapshot_rx: watch::Receiver<AppSnapshot>,
    ) -> Self {
        Self {
            command_tx,
            event_tx,
            snapshot_rx,
        }
    }

    pub fn send(&self, cmd: AppCommand) -> anyhow::Result<()> {
        self.command_tx
            .send(RuntimeCommand::App(cmd))
            .context("send command failed")
    }

    pub fn emit(&self, event: AppEvent) -> anyhow::Result<()> {
        let _ = self.event_tx.send(event);
        Ok(())
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<AppEvent> {
        self.event_tx.subscribe()
    }

    pub fn subscribe_snapshot(&self) -> watch::Receiver<AppSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn set_tray_available(&self, available: bool) -> anyhow::Result<()> {
        self.command_tx
            .send(RuntimeCommand::SetTrayAvailable(available))
            .context("set tray availability failed")
    }
}

pub struct Runtime {
    command_tx: mpsc::Sender<RuntimeCommand>,
    join: Option<JoinHandle<()>>,
}

impl Runtime {
    pub fn spawn(
        repaint_ctx: egui::Context,
        poller_command_tx: mpsc::Sender<PollerCommand>,
        poller_event_rx: mpsc::Receiver<PollerEvent>,
    ) -> (Self, RuntimeHandle) {
        Self::spawn_inner(repaint_ctx, poller_command_tx, poller_event_rx)
    }

    fn spawn_inner(
        repaint_ctx: egui::Context,
        poller_command_tx: mpsc::Sender<PollerCommand>,
        poller_event_rx: mpsc::Receiver<PollerEvent>,
    ) -> (Self, RuntimeHandle) {
        let (event_tx, _) = broadcast::channel::<AppEvent>(64);
        let (snapshot_tx, snapshot_rx) = watch::channel(AppSnapshot::default());
        let (command_tx, command_rx) = mpsc::channel::<RuntimeCommand>();
        let handle = RuntimeHandle::new(command_tx.clone(), event_tx.clone(), snapshot_rx.clone());
        let join = thread::spawn(move || {
            let mut tray_available = false;
            let mut state = AppState::default();
            let poller_event_rx = poller_event_rx;

            loop {
                while let Ok(event) = poller_event_rx.try_recv() {
                    handle_poller_event(event, &mut state, &event_tx, &snapshot_tx);
                    repaint_ctx.request_repaint();
                }

                match command_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(RuntimeCommand::App(app_command)) => {
                        handle_app_command(
                            app_command,
                            &mut state,
                            tray_available,
                            &event_tx,
                            &snapshot_tx,
                            &poller_command_tx,
                        );
                        repaint_ctx.request_repaint();
                    }
                    Ok(RuntimeCommand::SetTrayAvailable(available)) => {
                        tray_available = available;
                    }
                    Ok(RuntimeCommand::Shutdown) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        (
            Self {
                command_tx,
                join: Some(join),
            },
            handle,
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
    state: &mut AppState,
    tray_available: bool,
    event_tx: &broadcast::Sender<AppEvent>,
    snapshot_tx: &watch::Sender<AppSnapshot>,
    poller_command_tx: &mpsc::Sender<PollerCommand>,
) {
    match command {
        AppCommand::ForcePoll => {
            let _ = poller_command_tx.send(PollerCommand::ForcePoll);
        }
        AppCommand::MarkRead { key, read } => {
            state.apply_mark_read_request(&key, read);
            publish_snapshot(state, event_tx, snapshot_tx);
            let _ = poller_command_tx.send(PollerCommand::MarkRead { key, read });
        }
        AppCommand::RequestCloseMainWindow => {
            if let Some(action) = close_action_for_request(true, false, tray_available) {
                let shell_command = match action {
                    crate::core::policy::window_lifecycle::CloseAction::MinimizeToTray => {
                        ShellCommand::HideWindow(WindowId::Main)
                    }
                    crate::core::policy::window_lifecycle::CloseAction::CloseApp => {
                        ShellCommand::ExitProcess
                    }
                };
                let _ = event_tx.send(AppEvent::ShellCommand(shell_command));
            }
        }
        AppCommand::RequestShowMainWindow => {
            let _ = event_tx.send(AppEvent::ShellCommand(ShellCommand::ShowWindow(
                WindowId::Main,
            )));
            let _ = event_tx.send(AppEvent::ShellCommand(ShellCommand::FocusWindow(
                WindowId::Main,
            )));
        }
        AppCommand::RequestExitApp => {
            let _ = event_tx.send(AppEvent::ShellCommand(ShellCommand::ExitProcess));
        }
        AppCommand::RequestShowWidget => {
            let _ = event_tx.send(AppEvent::WidgetVisibilityChanged { visible: true });
            let _ = event_tx.send(AppEvent::ShellCommand(ShellCommand::ShowWindow(
                WindowId::Widget,
            )));
        }
        AppCommand::RequestHideWidget => {
            let _ = event_tx.send(AppEvent::WidgetVisibilityChanged { visible: false });
            let _ = event_tx.send(AppEvent::ShellCommand(ShellCommand::HideWindow(
                WindowId::Widget,
            )));
        }
        _ => {}
    }
}

fn publish_snapshot(
    state: &AppState,
    event_tx: &broadcast::Sender<AppEvent>,
    snapshot_tx: &watch::Sender<AppSnapshot>,
) {
    let snapshot = state.to_snapshot();
    let _ = snapshot_tx.send(snapshot.clone());
    let _ = event_tx.send(AppEvent::SnapshotUpdated(snapshot));
}

fn handle_poller_event(
    event: PollerEvent,
    state: &mut AppState,
    event_tx: &broadcast::Sender<AppEvent>,
    snapshot_tx: &watch::Sender<AppSnapshot>,
) {
    match event {
        PollerEvent::Snapshot {
            fetched_at_ms,
            page,
        } => {
            state.apply_snapshot(fetched_at_ms, &page);
            let _ = event_tx.send(AppEvent::PollerSnapshot {
                fetched_at_ms,
                page,
            });
            publish_snapshot(state, event_tx, snapshot_tx);
        }
        PollerEvent::PollFailed { error } => {
            state.apply_poll_failed(error.clone());
            publish_snapshot(state, event_tx, snapshot_tx);
            let _ = event_tx.send(AppEvent::PollFailed { error });
        }
        PollerEvent::MarkReadSynced { key } => {
            state.apply_mark_read_synced(&key);
            publish_snapshot(state, event_tx, snapshot_tx);
            let _ = event_tx.send(AppEvent::MarkReadSynced { key });
        }
        PollerEvent::SyncFailed { key, error } => {
            state.apply_sync_failed(&key, error.clone());
            publish_snapshot(state, event_tx, snapshot_tx);
            let _ = event_tx.send(AppEvent::SyncFailed { key, error });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::api::SignalPage;
    use crate::domain::SignalKey;
    use crate::poller::PollerEvent;

    fn test_handle() -> RuntimeHandle {
        let (command_tx, command_rx) = mpsc::channel::<RuntimeCommand>();
        let (event_tx, _) = broadcast::channel::<AppEvent>(64);
        let (_snapshot_tx, snapshot_rx) = watch::channel(AppSnapshot::default());
        let handle = RuntimeHandle::new(command_tx, event_tx, snapshot_rx);
        // Keep receivers alive for the duration of the test helper.
        std::mem::forget(command_rx);
        handle
    }

    fn spawn_for_test(
        poller_command_tx: mpsc::Sender<PollerCommand>,
        poller_event_rx: mpsc::Receiver<PollerEvent>,
    ) -> (Runtime, RuntimeHandle) {
        Runtime::spawn_inner(egui::Context::default(), poller_command_tx, poller_event_rx)
    }

    #[test]
    fn runtime_accepts_force_poll_command() {
        let handle = test_handle();
        handle.send(AppCommand::ForcePoll).expect("send command");
    }

    #[test]
    fn runtime_emits_snapshot_event() {
        let handle = test_handle();
        handle
            .emit(AppEvent::SnapshotUpdated(Default::default()))
            .expect("emit event");
    }

    #[test]
    fn runtime_forwards_force_poll_to_poller() {
        let (poller_tx, poller_rx) = mpsc::channel::<PollerCommand>();
        let (_runtime, handle) = spawn_for_test(poller_tx, mpsc::channel::<PollerEvent>().1);

        handle.send(AppCommand::ForcePoll).expect("send command");

        let command = poller_rx.recv().expect("poller command");
        assert!(matches!(command, PollerCommand::ForcePoll));
    }

    #[test]
    fn runtime_forwards_mark_read_to_poller() {
        let (poller_tx, poller_rx) = mpsc::channel::<PollerCommand>();
        let (_runtime, handle) = spawn_for_test(poller_tx, mpsc::channel::<PollerEvent>().1);
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
        let (_runtime, handle) = spawn_for_test(poller_tx, mpsc::channel::<PollerEvent>().1);
        let mut event_rx = handle.subscribe_events();
        handle
            .set_tray_available(true)
            .expect("set tray availability");
        handle
            .send(AppCommand::RequestShowMainWindow)
            .expect("send command");

        let event = event_rx.blocking_recv().expect("runtime event");
        assert!(matches!(
            event,
            AppEvent::ShellCommand(ShellCommand::ShowWindow(WindowId::Main))
        ));
    }

    #[test]
    fn runtime_emits_hide_to_tray_when_tray_is_available() {
        let (poller_tx, _poller_rx) = mpsc::channel::<PollerCommand>();
        let (_runtime, handle) = spawn_for_test(poller_tx, mpsc::channel::<PollerEvent>().1);
        let mut event_rx = handle.subscribe_events();
        handle
            .set_tray_available(true)
            .expect("set tray availability");
        handle
            .send(AppCommand::RequestCloseMainWindow)
            .expect("send command");

        let event = event_rx.blocking_recv().expect("runtime event");
        assert!(matches!(
            event,
            AppEvent::ShellCommand(ShellCommand::HideWindow(WindowId::Main))
        ));
    }

    #[test]
    fn runtime_forwards_snapshot_poller_event_to_app_event() {
        let (event_tx, _) = broadcast::channel::<AppEvent>(64);
        let (snapshot_tx, _snapshot_rx) = watch::channel(AppSnapshot::default());
        let mut event_rx = event_tx.subscribe();
        let mut state = AppState::default();
        let page = SignalPage {
            total: 0,
            page: 1,
            page_size: 100,
            data: vec![],
        };

        handle_poller_event(
            PollerEvent::Snapshot {
                fetched_at_ms: 42,
                page: page.clone(),
            },
            &mut state,
            &event_tx,
            &snapshot_tx,
        );

        let event = event_rx.blocking_recv().expect("runtime event");
        assert!(matches!(
            event,
            AppEvent::PollerSnapshot {
                fetched_at_ms: 42,
                page: actual
            } if actual == page
        ));
    }

    #[test]
    fn runtime_forwards_sync_failed_poller_event_to_app_event() {
        let (event_tx, _) = broadcast::channel::<AppEvent>(64);
        let (snapshot_tx, _snapshot_rx) = watch::channel(AppSnapshot::default());
        let mut event_rx = event_tx.subscribe();
        let mut state = AppState::default();
        let key = SignalKey::new("BTCUSDT", "15", "vegas");

        handle_poller_event(
            PollerEvent::SyncFailed {
                key: key.clone(),
                error: "boom".to_string(),
            },
            &mut state,
            &event_tx,
            &snapshot_tx,
        );

        let _snapshot_event = event_rx.blocking_recv().expect("snapshot event");
        let event = event_rx.blocking_recv().expect("runtime event");
        assert!(matches!(
            event,
            AppEvent::SyncFailed {
                key: actual,
                error
            } if actual == key && error == "boom"
        ));
    }

    #[test]
    fn runtime_broadcasts_widget_visibility_to_multiple_subscribers() {
        let (poller_tx, _poller_rx) = mpsc::channel::<PollerCommand>();
        let (_runtime, handle) = Runtime::spawn(
            egui::Context::default(),
            poller_tx,
            mpsc::channel::<PollerEvent>().1,
        );
        let mut rx_one = handle.subscribe_events();
        let mut rx_two = handle.subscribe_events();

        handle
            .send(AppCommand::RequestShowWidget)
            .expect("show widget");

        assert!(matches!(
            rx_one.blocking_recv().expect("event one"),
            AppEvent::WidgetVisibilityChanged { visible: true }
        ));
        assert!(matches!(
            rx_two.blocking_recv().expect("event two"),
            AppEvent::WidgetVisibilityChanged { visible: true }
        ));
    }

    #[test]
    fn runtime_snapshot_subscription_receives_poll_updates() {
        let (poller_tx, _poller_rx) = mpsc::channel::<PollerCommand>();
        let (poller_event_tx, poller_event_rx) = mpsc::channel::<PollerEvent>();
        let (_runtime, handle) =
            Runtime::spawn(egui::Context::default(), poller_tx, poller_event_rx);
        let mut snapshot_rx = handle.subscribe_snapshot();

        let page = SignalPage {
            total: 1,
            page: 1,
            page_size: 100,
            data: vec![],
        };

        poller_event_tx
            .send(PollerEvent::Snapshot {
                fetched_at_ms: 42,
                page,
            })
            .expect("poll event");

        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
            .block_on(snapshot_rx.changed())
            .expect("snapshot change");
        assert_eq!(snapshot_rx.borrow().last_poll_ms, Some(42));
    }
}
