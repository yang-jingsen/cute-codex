//! Structured HTTP lifecycle notifications for the TUI.
//!
//! This boundary is deliberately independent from terminal/desktop
//! notifications. It preserves the deployed cute-codex payload contract while
//! letting the current 0.146 ChatWidget own lifecycle state and scheduling.

use chrono::Utc;
use codex_config::types::NotifyServiceEvent;
use codex_config::types::NotifyServiceSettings;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;
use std::time::Instant;

pub(crate) type IdleNotifyStatus = NotifyServiceEvent;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct IdleNotifyPayload {
    pub status: IdleNotifyStatus,
    pub project_name: String,
    pub agent_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    pub cwd: String,
    pub host_name: String,
    pub duration_seconds: u64,
    pub session_duration_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_duration_seconds: Option<u64>,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub idle_seconds: u64,
    pub completed_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_details: Option<Value>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_payload_with_details(
    status: IdleNotifyStatus,
    cwd: &Path,
    agent_name: &str,
    thread_name: Option<&str>,
    thread_id: Option<String>,
    session_started_at: Option<Instant>,
    turn_duration_seconds: Option<u64>,
    usage: &crate::token_usage::TokenUsage,
    idle_seconds: u64,
    event_details: Option<Value>,
) -> IdleNotifyPayload {
    let project_name = cwd
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_string();
    let host_name = gethostname::gethostname().to_string_lossy().to_string();
    let session_duration_seconds = session_started_at
        .map(|started_at| started_at.elapsed().as_secs())
        .unwrap_or(0);
    let duration_seconds = turn_duration_seconds.unwrap_or(session_duration_seconds);
    IdleNotifyPayload {
        status,
        project_name,
        agent_name: agent_name.to_string(),
        session_name: thread_name.map(str::to_string),
        thread_id,
        cwd: cwd.display().to_string(),
        host_name,
        duration_seconds,
        session_duration_seconds,
        turn_duration_seconds,
        total_tokens: usage.total_tokens,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cached_input_tokens: usage.cached_input_tokens,
        reasoning_output_tokens: usage.reasoning_output_tokens,
        idle_seconds,
        completed_at: Utc::now().to_rfc3339(),
        event_details,
    }
}

pub(crate) fn event_enabled(settings: &NotifyServiceSettings, status: IdleNotifyStatus) -> bool {
    settings.notify_service_events.contains(&status)
}

pub(crate) fn send_idle_notification(
    url: String,
    token: Option<String>,
    payload: IdleNotifyPayload,
) {
    tokio::spawn(async move {
        send_idle_notification_once(url, token, payload).await;
    });
}

pub(crate) async fn send_idle_notification_once(
    url: String,
    token: Option<String>,
    payload: IdleNotifyPayload,
) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            tracing::warn!(error = %err, "failed to build reqwest client for lifecycle notification");
            return;
        }
    };
    let mut request = client.post(&url).json(&payload);
    if let Some(token) = token.as_deref() {
        request = request.bearer_auth(token);
    }
    match request.send().await {
        Ok(response) if !response.status().is_success() => {
            tracing::warn!(
                status = %response.status(),
                url = %url,
                "lifecycle notification POST returned non-success"
            );
        }
        Err(err) => {
            tracing::warn!(error = %err, url = %url, "lifecycle notification POST failed");
        }
        Ok(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    #[test]
    fn payload_uses_turn_duration_and_preserves_optional_details() {
        let payload = build_payload_with_details(
            IdleNotifyStatus::TurnCompleted,
            Path::new("/tmp/cutex-notify"),
            "codex",
            Some("thread"),
            Some("thread-id".to_string()),
            Some(Instant::now() - Duration::from_secs(3)),
            Some(7),
            &crate::token_usage::TokenUsage::default(),
            0,
            Some(serde_json::json!({"turn": {"follow_up_started": false}})),
        );
        assert_eq!(payload.duration_seconds, 7);
        assert_eq!(payload.turn_duration_seconds, Some(7));
        assert_eq!(payload.thread_id.as_deref(), Some("thread-id"));
        assert_eq!(
            payload
                .event_details
                .as_ref()
                .and_then(|details| details.pointer("/turn/follow_up_started"))
                .and_then(Value::as_bool),
            Some(false)
        );
    }

    #[tokio::test]
    async fn awaited_sender_posts_json_and_bearer_token() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        let request_task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept request");
            let mut buffer = Vec::new();
            let header_end = loop {
                let mut chunk = [0_u8; 1024];
                let read = stream.read(&mut chunk).await.expect("read request");
                assert!(read > 0, "connection closed before headers");
                buffer.extend_from_slice(&chunk[..read]);
                if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                    break position + 4;
                }
            };
            let headers = String::from_utf8_lossy(&buffer[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length:")
                        .or_else(|| line.strip_prefix("Content-Length:"))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .expect("content length");
            while buffer.len() < header_end + content_length {
                let mut chunk = [0_u8; 1024];
                let read = stream.read(&mut chunk).await.expect("read body");
                assert!(read > 0, "connection closed before body");
                buffer.extend_from_slice(&chunk[..read]);
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .await
                .expect("write response");
            String::from_utf8_lossy(&buffer).into_owned()
        });

        let payload = build_payload_with_details(
            IdleNotifyStatus::SessionExit,
            Path::new("/tmp/cutex-notify"),
            "codex",
            None,
            None,
            None,
            None,
            &crate::token_usage::TokenUsage::default(),
            0,
            None,
        );
        send_idle_notification_once(
            format!("http://{address}/api/agent-notify/push"),
            Some("sample-token".to_string()),
            payload,
        )
        .await;

        let request = tokio::time::timeout(Duration::from_secs(1), request_task)
            .await
            .expect("request received")
            .expect("request task");
        assert!(request.starts_with("POST /api/agent-notify/push "));
        assert!(request.contains("authorization: Bearer sample-token"));
        assert!(request.contains("\"status\":\"session_exit\""));
    }
}
