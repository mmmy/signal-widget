use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chrono::Utc;
use egui::Context as EguiContext;
use tracing::{debug, error};

use crate::api::{ApiClient, SignalPage};
use crate::config::AppConfig;
use crate::core::services::poller_service::build_request;
use crate::domain::SignalKey;

#[derive(Debug, Clone)]
pub enum PollerCommand {
    ForcePoll,
    MarkRead { key: SignalKey, read: bool },
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum PollerEvent {
    Snapshot {
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
}

pub struct PollerHandle {
    pub command_tx: mpsc::Sender<PollerCommand>,
    pub event_rx: mpsc::Receiver<PollerEvent>,
    join: Option<JoinHandle<()>>,
}

impl PollerHandle {
    pub fn spawn(client: ApiClient, config: AppConfig, repaint_ctx: EguiContext) -> Self {
        let (command_tx, command_rx) = mpsc::channel::<PollerCommand>();
        let (event_tx, event_rx) = mpsc::channel::<PollerEvent>();

        let join = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime build failed");

            let poll_interval = Duration::from_secs(config.poll.interval_secs.max(1));
            let mut last_poll = Instant::now() - poll_interval;

            loop {
                let wait_for = poll_interval
                    .checked_sub(last_poll.elapsed())
                    .unwrap_or_else(|| Duration::from_millis(0));

                match command_rx.recv_timeout(wait_for) {
                    Ok(PollerCommand::Shutdown) => break,
                    Ok(PollerCommand::ForcePoll) => {
                        if let Err(err) =
                            poll_once(&runtime, &client, &config, &event_tx, &repaint_ctx)
                        {
                            emit_poll_err(&event_tx, &repaint_ctx, err.to_string());
                        }
                        last_poll = Instant::now();
                    }
                    Ok(PollerCommand::MarkRead { key, read }) => {
                        let result = runtime.block_on(client.mark_read(&key, read));
                        match result {
                            Ok(true) => emit_mark_read_synced(&event_tx, &repaint_ctx, key),
                            Ok(false) => emit_sync_err(
                                &event_tx,
                                &repaint_ctx,
                                key,
                                "server returned false".to_string(),
                            ),
                            Err(err) => {
                                emit_sync_err(&event_tx, &repaint_ctx, key, err.to_string())
                            }
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }

                if last_poll.elapsed() >= poll_interval {
                    if let Err(err) = poll_once(&runtime, &client, &config, &event_tx, &repaint_ctx)
                    {
                        emit_poll_err(&event_tx, &repaint_ctx, err.to_string());
                    }
                    last_poll = Instant::now();
                }
            }
        });

        Self {
            command_tx,
            event_rx,
            join: Some(join),
        }
    }
}

impl Drop for PollerHandle {
    fn drop(&mut self) {
        let _ = self.command_tx.send(PollerCommand::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn poll_once(
    runtime: &tokio::runtime::Runtime,
    client: &ApiClient,
    config: &AppConfig,
    event_tx: &mpsc::Sender<PollerEvent>,
    repaint_ctx: &EguiContext,
) -> anyhow::Result<()> {
    let request = build_request(config);
    let Some(request) = request else {
        debug!("skip polling: no enabled groups");
        return Ok(());
    };

    let page = runtime.block_on(client.fetch_signals(&request))?;
    let event = PollerEvent::Snapshot {
        fetched_at_ms: Utc::now().timestamp_millis(),
        page,
    };
    if let Err(err) = event_tx.send(event) {
        error!("send snapshot event failed: {}", err);
    } else {
        repaint_ctx.request_repaint();
    }
    Ok(())
}

fn emit_poll_err(event_tx: &mpsc::Sender<PollerEvent>, repaint_ctx: &EguiContext, error: String) {
    let event = PollerEvent::PollFailed { error };
    if let Err(err) = event_tx.send(event) {
        tracing::error!("send poll failed event error: {}", err);
    } else {
        repaint_ctx.request_repaint();
    }
}

fn emit_sync_err(
    event_tx: &mpsc::Sender<PollerEvent>,
    repaint_ctx: &EguiContext,
    key: SignalKey,
    error: String,
) {
    let event = PollerEvent::SyncFailed { key, error };
    if let Err(err) = event_tx.send(event) {
        tracing::error!("send sync failed event error: {}", err);
    } else {
        repaint_ctx.request_repaint();
    }
}

fn emit_mark_read_synced(
    event_tx: &mpsc::Sender<PollerEvent>,
    repaint_ctx: &EguiContext,
    key: SignalKey,
) {
    let event = PollerEvent::MarkReadSynced { key };
    if let Err(err) = event_tx.send(event) {
        tracing::error!("send mark-read synced event error: {}", err);
    } else {
        repaint_ctx.request_repaint();
    }
}
