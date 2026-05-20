use std::time::Duration;

use reqwest::Client;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{self, Instant};

use crate::event::ErrorReport;

pub async fn post_reports(
    client: &Client,
    collector_endpoint: &str,
    reports: &[ErrorReport],
) -> Result<(), reqwest::Error> {
    for report in reports {
        client
            .post(collector_endpoint)
            .json(report)
            .send()
            .await?
            .error_for_status()?;
    }
    Ok(())
}

struct QueuedReport {
    report: ErrorReport,
    urgent: bool,
}

enum SenderCommand {
    Send(Box<QueuedReport>),
    Flush { ack: oneshot::Sender<()> },
}

/// Async batch sender with explicit flush (Sentry-style guard support).
#[derive(Clone)]
pub struct SenderHandle {
    tx: mpsc::UnboundedSender<SenderCommand>,
}

impl SenderHandle {
    pub fn spawn(
        collector_endpoint: String,
        batch_max: usize,
        batch_interval: Duration,
    ) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("hawk reqwest client");

        tokio::spawn(async move {
            let mut buffer: Vec<ErrorReport> = Vec::new();
            let mut flush_deadline = Instant::now() + batch_interval;

            loop {
                let sleep = time::sleep_until(flush_deadline);
                tokio::pin!(sleep);

                tokio::select! {
                    cmd = rx.recv() => {
                        let Some(cmd) = cmd else {
                            flush_buffer(&client, &collector_endpoint, &mut buffer).await;
                            break;
                        };
                        match cmd {
                            SenderCommand::Send(item) => {
                                buffer.push(item.report);
                                if item.urgent || buffer.len() >= batch_max {
                                    flush_buffer(&client, &collector_endpoint, &mut buffer).await;
                                    flush_deadline = Instant::now() + batch_interval;
                                }
                            }
                            SenderCommand::Flush { ack } => {
                                flush_buffer(&client, &collector_endpoint, &mut buffer).await;
                                flush_deadline = Instant::now() + batch_interval;
                                let _ = ack.send(());
                            }
                        }
                    }
                    _ = &mut sleep => {
                        if !buffer.is_empty() {
                            flush_buffer(&client, &collector_endpoint, &mut buffer).await;
                        }
                        flush_deadline = Instant::now() + batch_interval;
                    }
                }
            }
        });

        Self { tx }
    }

    pub fn send(&self, report: ErrorReport, urgent: bool) {
        let _ = self
            .tx
            .send(SenderCommand::Send(Box::new(QueuedReport { report, urgent })));
    }

    pub async fn flush(&self, timeout: Duration) {
        let (ack_tx, ack_rx) = oneshot::channel();
        if self.tx.send(SenderCommand::Flush { ack: ack_tx }).is_err() {
            return;
        }
        let _ = time::timeout(timeout, ack_rx).await;
    }
}

async fn flush_buffer(client: &Client, endpoint: &str, buffer: &mut Vec<ErrorReport>) {
    if buffer.is_empty() {
        return;
    }
    let batch = std::mem::take(buffer);
    if let Err(e) = post_reports(client, endpoint, &batch).await {
        tracing::warn!(error = %e, count = batch.len(), "hawk: failed to send events");
    }
}

/// Flush pending events on shutdown (called from `HawkGuard::drop`).
pub struct HawkGuard {
    sender: SenderHandle,
    timeout: Duration,
}

impl HawkGuard {
    pub fn new(sender: SenderHandle, timeout: Duration) -> Self {
        Self { sender, timeout }
    }

    pub async fn flush(&self) {
        self.sender.flush(self.timeout).await;
    }

    /// Best-effort flush on shutdown. Never blocks inside a Tokio runtime.
    pub fn flush_on_shutdown(&self) {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let sender = self.sender.clone();
            let timeout = self.timeout;
            handle.spawn(async move {
                sender.flush(timeout).await;
            });
        } else if let Ok(rt) = tokio::runtime::Runtime::new() {
            rt.block_on(self.sender.flush(self.timeout));
        }
    }
}

impl Drop for HawkGuard {
    fn drop(&mut self) {
        self.flush_on_shutdown();
    }
}
