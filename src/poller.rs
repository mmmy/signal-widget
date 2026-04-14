use std::collections::BTreeSet;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chrono::Utc;
use tracing::{debug, error};

use crate::api::{ApiClient, FetchSignalsRequest, SignalPage};
use crate::config::{AppConfig, GroupConfig};
use crate::domain::SignalKey;

#[derive(Debug, Clone)]
pub enum PollerCommand {
    ForcePoll,
    MarkRead { key: SignalKey, read: bool },
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum PollerEvent {
    Snapshot { fetched_at_ms: i64, page: SignalPage },
    PollFailed { error: String },
    SyncFailed { key: SignalKey, error: String },
}

pub struct PollerHandle {
    pub command_tx: mpsc::Sender<PollerCommand>,
    pub event_rx: mpsc::Receiver<PollerEvent>,
    join: Option<JoinHandle<()>>,
}

impl PollerHandle {
    pub fn spawn(client: ApiClient, config: AppConfig) -> Self {
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
                        if let Err(err) = poll_once(&runtime, &client, &config, &event_tx) {
                            emit_poll_err(&event_tx, err.to_string());
                        }
                        last_poll = Instant::now();
                    }
                    Ok(PollerCommand::MarkRead { key, read }) => {
                        let result = runtime.block_on(client.mark_read(&key, read));
                        if let Err(err) = result {
                            emit_sync_err(&event_tx, key, err.to_string());
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }

                if last_poll.elapsed() >= poll_interval {
                    if let Err(err) = poll_once(&runtime, &client, &config, &event_tx) {
                        emit_poll_err(&event_tx, err.to_string());
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
    }
    Ok(())
}

fn build_request(config: &AppConfig) -> Option<FetchSignalsRequest> {
    let enabled: Vec<&GroupConfig> = config.groups.iter().filter(|g| g.enabled).collect();
    if enabled.is_empty() {
        return None;
    }

    let symbols = join_unique(enabled.iter().map(|g| g.symbol.clone()));
    let periods = join_unique(enabled.iter().flat_map(|g| g.periods.clone()));
    let signal_types = join_unique(enabled.iter().flat_map(|g| g.signal_types.clone()));

    Some(FetchSignalsRequest {
        symbols,
        periods: if periods.is_empty() { None } else { Some(periods) },
        signal_types: if signal_types.is_empty() { None } else { Some(signal_types) },
        page: Some(1),
        page_size: Some(config.poll.page_size.min(100)),
    })
}

fn join_unique<I>(iter: I) -> String
where
    I: IntoIterator<Item = String>,
{
    let set: BTreeSet<String> = iter
        .into_iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect();
    set.into_iter().collect::<Vec<_>>().join(",")
}

fn emit_poll_err(event_tx: &mpsc::Sender<PollerEvent>, error: String) {
    let event = PollerEvent::PollFailed { error };
    if let Err(err) = event_tx.send(event) {
        tracing::error!("send poll failed event error: {}", err);
    }
}

fn emit_sync_err(event_tx: &mpsc::Sender<PollerEvent>, key: SignalKey, error: String) {
    let event = PollerEvent::SyncFailed { key, error };
    if let Err(err) = event_tx.send(event) {
        tracing::error!("send sync failed event error: {}", err);
    }
}
