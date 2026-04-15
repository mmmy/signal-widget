use std::sync::mpsc;

use anyhow::Context as _;

use crate::core::contract::{AppCommand, AppEvent};
use crate::core::state::AppState;

#[derive(Clone)]
pub struct RuntimeHandle {
    command_tx: mpsc::Sender<AppCommand>,
    event_tx: mpsc::Sender<AppEvent>,
}

impl RuntimeHandle {
    pub fn new(command_tx: mpsc::Sender<AppCommand>, event_tx: mpsc::Sender<AppEvent>) -> Self {
        Self { command_tx, event_tx }
    }

    pub fn send(&self, cmd: AppCommand) -> anyhow::Result<()> {
        self.command_tx.send(cmd).context("send command failed")
    }

    pub fn emit(&self, event: AppEvent) -> anyhow::Result<()> {
        self.event_tx.send(event).context("send event failed")
    }

    pub fn new_for_test() -> Self {
        let (command_tx, command_rx) = mpsc::channel::<AppCommand>();
        let (event_tx, event_rx) = mpsc::channel::<AppEvent>();
        std::mem::forget(command_rx);
        std::mem::forget(event_rx);
        Self::new(command_tx, event_tx)
    }
}

pub struct Runtime {
    pub state: AppState,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::contract::AppCommand;

    #[test]
    fn runtime_accepts_force_poll_command() {
        let handle = RuntimeHandle::new_for_test();
        handle.send(AppCommand::ForcePoll).expect("send command");
    }
}
