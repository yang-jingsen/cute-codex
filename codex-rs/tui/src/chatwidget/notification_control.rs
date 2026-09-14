//! Thin UI adapter. Cutex owns persistence and label configuration.
use super::*;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::{self};

#[derive(Default)]
pub(super) struct NotificationControl {
    thread: Option<String>,
    pub(super) label: Option<String>,
    queued_cycles: usize,
    response: Option<Receiver<Result<String, String>>>,
    next_read: Option<Instant>,
}

impl ChatWidget {
    pub(super) fn refresh_notification_control(&mut self, cycle: bool) {
        let thread = self.thread_id().map(|id| id.to_string());
        if self.notification_control.thread != thread {
            self.notification_control = NotificationControl {
                thread: thread.clone(),
                ..Default::default()
            };
        }
        let Some(thread) = thread else { return };
        if cycle {
            self.notification_control.queued_cycles =
                (self.notification_control.queued_cycles + 1) % 3;
        }
        if let Some(response) = self.notification_control.response.as_ref() {
            match response.try_recv() {
                Ok(result) => {
                    self.notification_control.label = Some(result.unwrap_or_else(|error| {
                        tracing::warn!(%error, "Cutex notification preference unavailable");
                        "NOTIFY?".into()
                    }));
                    self.notification_control.response = None;
                    self.notification_control.next_read =
                        Some(Instant::now() + Duration::from_secs(5));
                    self.refresh_status_line();
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.notification_control.response = None;
                    self.notification_control.label = Some("NOTIFY?".into());
                    self.notification_control.next_read =
                        Some(Instant::now() + Duration::from_secs(5));
                    self.refresh_status_line();
                }
            }
        }
        let cycle = self.notification_control.queued_cycles > 0;
        if !cycle {
            if let Some(due) = self.notification_control.next_read
                && due > Instant::now()
            {
                self.frame_requester
                    .schedule_frame_in(due.saturating_duration_since(Instant::now()));
                return;
            }
            if !self
                .config
                .tui_status_line
                .as_ref()
                .is_some_and(|items| items.iter().any(|item| item == "notification"))
            {
                return;
            }
        }
        if cycle {
            self.notification_control.queued_cycles -= 1;
        }
        let (tx, rx) = mpsc::channel();
        self.notification_control.response = Some(rx);
        let frame = self.frame_requester.clone();
        tokio::spawn(async move {
            let result = query(&thread, cycle).await;
            let _ = tx.send(result);
            frame.schedule_frame();
        });
    }
}

async fn query(thread: &str, cycle: bool) -> Result<String, String> {
    let helper = std::env::var_os("CUTEX_NOTIFICATION_CONTROL").unwrap_or_else(|| "cutex".into());
    let mut command = tokio::process::Command::new(helper);
    command
        .args(["notify", "session", thread, "--json"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    if cycle {
        command.arg("--cycle");
    }
    let output = tokio::time::timeout(Duration::from_secs(3), command.output())
        .await
        .map_err(|_| "preference helper timed out".to_string())?
        .map_err(|error| error.to_string())?;
    if !output.status.success() || output.stdout.len() > 8192 {
        return Err("preference helper failed".into());
    }
    parse_response(thread, &output.stdout)
}

fn parse_response(thread: &str, bytes: &[u8]) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Response {
        thread_id: String,
        label: String,
    }
    let response: Response = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if response.thread_id != thread
        || response.label.trim().is_empty()
        || response.label.chars().count() > 32
        || response.label.chars().any(char::is_control)
    {
        return Err("invalid notification preference response".into());
    }
    Ok(response.label)
}

#[cfg(test)]
#[path = "notification_control_tests.rs"]
mod tests;
