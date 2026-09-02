//! Transitional runtime heartbeat for foreground and Windows-managed TUI sessions.

use reqwest::StatusCode;
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

const CUTEX_AGENT_ID_ENV_VAR: &str = "CUTEX_AGENT_ID";
const CUTEX_AGENT_HOST_ID_ENV_VAR: &str = "CUTEX_AGENT_HOST_ID";
const CODEX_LAUNCH_PROFILE_ENV_VAR: &str = "CODEX_LAUNCH_PROFILE";
const CUTEX_RUNTIME_HEARTBEAT_URL_ENV_VAR: &str = "CUTEX_RUNTIME_HEARTBEAT_URL";
const CUTEX_RUNTIME_HEARTBEAT_TOKEN_ENV_VAR: &str = "CUTEX_RUNTIME_HEARTBEAT_TOKEN";
const CUTEX_RUNTIME_LAUNCH_ID_ENV_VAR: &str = "CUTEX_RUNTIME_LAUNCH_ID";
const RUNTIME_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const RUNTIME_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(5);

static RUNTIME_HEARTBEAT_STATE: OnceLock<Arc<Mutex<Option<RuntimeHeartbeatSnapshot>>>> =
    OnceLock::new();
static RUNTIME_HEARTBEAT_THREAD: OnceLock<()> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeHeartbeatSnapshot {
    codex_session_id: String,
    thread_name: Option<String>,
    cwd: Option<String>,
    profile: Option<String>,
    host_id: Option<String>,
    runtime_agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeHeartbeatPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    launch_id: Option<String>,
    codex_session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_name: Option<String>,
    pid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    host_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime_agent_id: Option<String>,
    source: &'static str,
}

pub(crate) fn update(
    session_id: Option<&str>,
    thread_name: Option<&str>,
    cwd: Option<&Path>,
    source: &'static str,
) {
    let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    if runtime_heartbeat_url().is_none() {
        return;
    }

    let snapshot = RuntimeHeartbeatSnapshot {
        codex_session_id: session_id.to_string(),
        thread_name: thread_name.and_then(trimmed_value),
        cwd: cwd.map(|path| path.display().to_string()),
        profile: env_string(CODEX_LAUNCH_PROFILE_ENV_VAR),
        host_id: env_string(CUTEX_AGENT_HOST_ID_ENV_VAR),
        runtime_agent_id: env_string(CUTEX_AGENT_ID_ENV_VAR),
    };
    let state = runtime_heartbeat_state();
    match state.lock() {
        Ok(mut guard) => *guard = Some(snapshot.clone()),
        Err(err) => {
            tracing::warn!(error = %err, "failed to lock cutex runtime heartbeat state");
        }
    }

    ensure_runtime_heartbeat_thread();
    post_runtime_heartbeat_snapshot(snapshot, source);
}

fn runtime_heartbeat_state() -> Arc<Mutex<Option<RuntimeHeartbeatSnapshot>>> {
    RUNTIME_HEARTBEAT_STATE
        .get_or_init(|| Arc::new(Mutex::new(None)))
        .clone()
}

fn ensure_runtime_heartbeat_thread() {
    let state = runtime_heartbeat_state();
    RUNTIME_HEARTBEAT_THREAD.get_or_init(|| {
        let spawn_result = thread::Builder::new()
            .name("cutex-runtime-heartbeat".to_string())
            .spawn(move || {
                loop {
                    thread::sleep(RUNTIME_HEARTBEAT_INTERVAL);
                    let snapshot = match state.lock() {
                        Ok(guard) => guard.clone(),
                        Err(err) => {
                            tracing::warn!(
                                error = %err,
                                "failed to lock cutex runtime heartbeat state"
                            );
                            None
                        }
                    };
                    if let Some(snapshot) = snapshot {
                        post_runtime_heartbeat_snapshot(snapshot, "heartbeat");
                    }
                }
            });
        if let Err(err) = spawn_result {
            tracing::warn!(error = %err, "failed to spawn cutex runtime heartbeat thread");
        }
    });
}

fn post_runtime_heartbeat_snapshot(snapshot: RuntimeHeartbeatSnapshot, source: &'static str) {
    let Some(url) = runtime_heartbeat_url() else {
        return;
    };
    let token = env_string(CUTEX_RUNTIME_HEARTBEAT_TOKEN_ENV_VAR);
    let payload = build_payload(
        snapshot,
        env_string(CUTEX_RUNTIME_LAUNCH_ID_ENV_VAR),
        std::process::id(),
        source,
    );
    let spawn_result = thread::Builder::new()
        .name("cutex-runtime-heartbeat-post".to_string())
        .spawn(
            move || match post_runtime_heartbeat_once(&url, token.as_deref(), &payload) {
                Ok(status) if !status.is_success() => {
                    tracing::warn!(
                        %status,
                        url = %url,
                        "cutex runtime heartbeat POST returned non-success"
                    );
                }
                Err(err) => {
                    tracing::warn!(error = %err, url = %url, "cutex runtime heartbeat POST failed");
                }
                Ok(_) => {}
            },
        );
    if let Err(err) = spawn_result {
        tracing::warn!(
            error = %err,
            "failed to spawn cutex runtime heartbeat POST thread"
        );
    }
}

fn build_payload(
    snapshot: RuntimeHeartbeatSnapshot,
    launch_id: Option<String>,
    pid: u32,
    source: &'static str,
) -> RuntimeHeartbeatPayload {
    RuntimeHeartbeatPayload {
        launch_id,
        codex_session_id: snapshot.codex_session_id,
        thread_name: snapshot.thread_name,
        pid,
        cwd: snapshot.cwd,
        profile: snapshot.profile,
        host_id: snapshot.host_id,
        runtime_agent_id: snapshot.runtime_agent_id,
        source,
    }
}

fn post_runtime_heartbeat_once(
    url: &str,
    token: Option<&str>,
    payload: &RuntimeHeartbeatPayload,
) -> Result<StatusCode, reqwest::Error> {
    let client = reqwest::blocking::Client::builder()
        .timeout(RUNTIME_HEARTBEAT_TIMEOUT)
        .build()?;
    let mut request = client.post(url).json(payload);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    Ok(request.send()?.status())
}

fn runtime_heartbeat_url() -> Option<String> {
    env_string(CUTEX_RUNTIME_HEARTBEAT_URL_ENV_VAR)
}

fn env_string(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .and_then(|value| trimmed_value(&value))
}

fn trimmed_value(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    fn full_snapshot() -> RuntimeHeartbeatSnapshot {
        RuntimeHeartbeatSnapshot {
            codex_session_id: "session-1".to_string(),
            thread_name: Some("release review".to_string()),
            cwd: Some("/tmp/cutex-heartbeat".to_string()),
            profile: Some("managed".to_string()),
            host_id: Some("host-1".to_string()),
            runtime_agent_id: Some("runtime-1".to_string()),
        }
    }

    #[test]
    fn payload_uses_camel_case_and_omits_absent_metadata() {
        let mut snapshot = full_snapshot();
        snapshot.thread_name = None;
        snapshot.runtime_agent_id = None;
        let payload = build_payload(
            snapshot,
            Some("launch-1".to_string()),
            42,
            "session_configured",
        );
        let value = serde_json::to_value(payload).expect("serialize heartbeat payload");

        assert_eq!(value["launchId"], "launch-1");
        assert_eq!(value["codexSessionId"], "session-1");
        assert_eq!(value["pid"], 42);
        assert_eq!(value["cwd"], "/tmp/cutex-heartbeat");
        assert_eq!(value["profile"], "managed");
        assert_eq!(value["hostId"], "host-1");
        assert_eq!(value["source"], "session_configured");
        assert!(value.get("threadName").is_none());
        assert!(value.get("runtimeAgentId").is_none());
    }

    #[test]
    fn trimmed_value_rejects_empty_and_normalizes_nonempty_values() {
        assert_eq!(trimmed_value(" \n\t "), None);
        assert_eq!(trimmed_value("  agent-1 \n"), Some("agent-1".to_string()));
    }

    #[tokio::test]
    async fn post_once_sends_bearer_and_complete_json_payload() {
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
            let headers = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
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
            let body =
                serde_json::from_slice::<Value>(&buffer[header_end..header_end + content_length])
                    .expect("parse request body");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .await
                .expect("write response");
            (headers, body)
        });

        let payload = build_payload(
            full_snapshot(),
            Some("launch-1".to_string()),
            42,
            "session_configured",
        );
        let url = format!("http://{address}/api/runtime/heartbeat");
        let post_task = tokio::task::spawn_blocking(move || {
            post_runtime_heartbeat_once(&url, Some("sample-token"), &payload)
        });
        let status = tokio::time::timeout(Duration::from_secs(2), post_task)
            .await
            .expect("heartbeat POST completed")
            .expect("blocking task joined")
            .expect("heartbeat POST succeeded");
        assert_eq!(status, StatusCode::OK);

        let (headers, body) = tokio::time::timeout(Duration::from_secs(2), request_task)
            .await
            .expect("request received")
            .expect("request task joined");
        let headers = headers.to_ascii_lowercase();
        assert!(headers.starts_with("post /api/runtime/heartbeat "));
        assert!(headers.contains("authorization: bearer sample-token"));
        assert_eq!(body["launchId"], "launch-1");
        assert_eq!(body["codexSessionId"], "session-1");
        assert_eq!(body["threadName"], "release review");
        assert_eq!(body["hostId"], "host-1");
        assert_eq!(body["runtimeAgentId"], "runtime-1");
        assert_eq!(body["source"], "session_configured");
    }
}
