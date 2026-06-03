use crate::app_command::AppCommand;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::history_cell::PlainHistoryCell;
use crate::text_formatting::truncate_text;
use codex_protocol::AgentPath;
use codex_protocol::protocol::InterAgentCommunication;
use ratatui::style::Stylize;
use ratatui::text::Line;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::io::Read;
use std::io::Write;
use std::net::TcpStream;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::mpsc;
use std::time::Duration;
use url::Url;

const CUTEX_AGENT_BUS_URL_ENV_VAR: &str = "CUTEX_AGENT_BUS_URL";
const CUTEX_AGENT_BUS_TOKEN_ENV_VAR: &str = "CUTEX_AGENT_BUS_TOKEN";
const CUTEX_AGENT_ID_ENV_VAR: &str = "CUTEX_AGENT_ID";
const CUTEX_AGENT_NAME_ENV_VAR: &str = "CUTEX_AGENT_NAME";
const CODEX_LAUNCH_PROFILE_ENV_VAR: &str = "CODEX_LAUNCH_PROFILE";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RegisterRequest {
    id: String,
    name: String,
    base_name: String,
    path_key: String,
    profile: String,
    cwd: String,
    pid: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PollResponse {
    messages: Vec<CutexAgentMessage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CutexAgentMessage {
    id: String,
    from: String,
    content: String,
    #[serde(alias = "trigger_turn")]
    trigger_turn: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AckRequest {
    agent_id: String,
    message_ids: Vec<String>,
}

#[derive(Debug)]
struct AgentContextUpdate {
    thread_name: Option<String>,
    cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentIdentity {
    name: String,
    base_name: String,
    path_key: String,
}

static CONTEXT_UPDATE_TX: OnceLock<mpsc::Sender<AgentContextUpdate>> = OnceLock::new();

pub(crate) fn maybe_spawn(app_event_tx: AppEventSender, cwd: &Path) {
    let Ok(base_url) = std::env::var(CUTEX_AGENT_BUS_URL_ENV_VAR) else {
        return;
    };
    let Ok(agent_id) = std::env::var(CUTEX_AGENT_ID_ENV_VAR) else {
        return;
    };
    if base_url.trim().is_empty() || agent_id.trim().is_empty() {
        return;
    }
    let token = std::env::var(CUTEX_AGENT_BUS_TOKEN_ENV_VAR)
        .ok()
        .filter(|value| !value.trim().is_empty());
    let fallback_name = std::env::var(CUTEX_AGENT_NAME_ENV_VAR)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| agent_id.clone());
    let profile = std::env::var(CODEX_LAUNCH_PROFILE_ENV_VAR)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "-".to_string());
    let cwd = cwd.display().to_string();
    let (context_update_tx, context_update_rx) = mpsc::channel();
    let _ = CONTEXT_UPDATE_TX.set(context_update_tx);

    std::thread::spawn(move || {
        let client = CutexAgentClient {
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
            agent_id,
            fallback_name,
            profile,
            cwd,
            thread_name: None,
        };
        client.run(app_event_tx, context_update_rx);
    });
}

pub(crate) fn update_thread_context(thread_name: Option<&str>, cwd: Option<&Path>) {
    let Some(tx) = CONTEXT_UPDATE_TX.get() else {
        return;
    };
    let _ = tx.send(AgentContextUpdate {
        thread_name: thread_name.map(str::to_string),
        cwd: cwd.map(|path| path.display().to_string()),
    });
}

struct CutexAgentClient {
    base_url: String,
    token: Option<String>,
    agent_id: String,
    fallback_name: String,
    profile: String,
    cwd: String,
    thread_name: Option<String>,
}

impl CutexAgentClient {
    fn run(
        mut self,
        app_event_tx: AppEventSender,
        context_update_rx: mpsc::Receiver<AgentContextUpdate>,
    ) {
        let mut registered_identity: Option<AgentIdentity> = None;
        let mut delivered_ids = DeliveredMessageIds::default();
        loop {
            let mut context_changed = false;
            while let Ok(update) = context_update_rx.try_recv() {
                self.thread_name = update.thread_name;
                if let Some(cwd) = update.cwd {
                    self.cwd = cwd;
                }
                context_changed = true;
            }

            let identity = self.identity();
            if context_changed || registered_identity.as_ref() != Some(&identity) {
                registered_identity = self.register(&identity).ok().map(|_| identity.clone());
            }
            match self.poll() {
                Ok(messages) => {
                    let mut ack_ids = Vec::new();
                    for message in messages {
                        let message_id = message.id.clone();
                        if delivered_ids.remember(message_id.clone()) {
                            deliver_message(&app_event_tx, message);
                        }
                        ack_ids.push(message_id);
                    }
                    if !ack_ids.is_empty() {
                        if let Err(err) = self.ack(&ack_ids) {
                            tracing::warn!("cutex agent bus ack failed: {err}");
                        }
                    }
                    std::thread::sleep(Duration::from_secs(2));
                }
                Err(err) => {
                    tracing::warn!("cutex agent bus poll failed: {err}");
                    registered_identity = None;
                    std::thread::sleep(Duration::from_secs(5));
                }
            }
        }
    }

    fn identity(&self) -> AgentIdentity {
        let raw_base = self
            .thread_name
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(self.fallback_name.as_str());
        let base_name = sanitize_agent_label(raw_base, "agent");
        let path_key = cwd_path_key(&self.cwd);
        AgentIdentity {
            name: format!("{base_name}.{path_key}"),
            base_name,
            path_key,
        }
    }

    fn register(&self, identity: &AgentIdentity) -> Result<(), String> {
        let body = serde_json::to_vec(&RegisterRequest {
            id: self.agent_id.clone(),
            name: identity.name.clone(),
            base_name: identity.base_name.clone(),
            path_key: identity.path_key.clone(),
            profile: self.profile.clone(),
            cwd: self.cwd.clone(),
            pid: std::process::id(),
        })
        .map_err(|err| err.to_string())?;
        http_json(
            &self.base_url,
            "POST",
            "/api/agents/register",
            self.token.as_deref(),
            Some(&body),
        )
        .map(|_| ())
    }

    fn poll(&self) -> Result<Vec<CutexAgentMessage>, String> {
        let value = http_json(
            &self.base_url,
            "GET",
            &format!("/api/messages/poll?agent_id={}&ack=1", self.agent_id),
            self.token.as_deref(),
            None,
        )?;
        serde_json::from_value::<PollResponse>(value)
            .map(|response| response.messages)
            .map_err(|err| err.to_string())
    }

    fn ack(&self, message_ids: &[String]) -> Result<(), String> {
        let body = serde_json::to_vec(&AckRequest {
            agent_id: self.agent_id.clone(),
            message_ids: message_ids.to_vec(),
        })
        .map_err(|err| err.to_string())?;
        http_json(
            &self.base_url,
            "POST",
            "/api/messages/ack",
            self.token.as_deref(),
            Some(&body),
        )
        .map(|_| ())
    }
}

#[derive(Default)]
struct DeliveredMessageIds {
    set: HashSet<String>,
    order: VecDeque<String>,
}

impl DeliveredMessageIds {
    fn remember(&mut self, id: String) -> bool {
        if !self.set.insert(id.clone()) {
            return false;
        }
        self.order.push_back(id);
        while self.order.len() > 1024 {
            if let Some(old) = self.order.pop_front() {
                self.set.remove(&old);
            }
        }
        true
    }
}

fn sanitize_agent_label(input: &str, fallback: &str) -> String {
    let mut sanitized = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        let next = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else if matches!(ch, '.' | '-' | '_') {
            ch
        } else {
            '-'
        };
        if next == '-' && last_dash {
            continue;
        }
        sanitized.push(next);
        last_dash = next == '-';
        if sanitized.len() >= 48 {
            break;
        }
    }
    let trimmed = sanitized.trim_matches(|ch: char| matches!(ch, '.' | '-' | '_'));
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn cwd_path_key(cwd: &str) -> String {
    let hash = fnv1a_hex(cwd);
    hash[..7].to_string()
}

fn fnv1a_hex(input: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn deliver_message(app_event_tx: &AppEventSender, message: CutexAgentMessage) {
    if message.content.trim().is_empty() {
        return;
    }
    app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(
        visible_agent_message_cell(&message),
    )));
    let communication = InterAgentCommunication::new(
        agent_path_for_label(&message.from),
        AgentPath::root(),
        Vec::new(),
        message.content,
        message.trigger_turn,
    );
    app_event_tx.send(AppEvent::CodexOp(AppCommand::inter_agent_communication(
        communication,
    )));
}

fn visible_agent_message_cell(message: &CutexAgentMessage) -> PlainHistoryCell {
    let reply_command = format!("cutex agent send {} \"message\"", message.from);
    let mut lines = vec![
        Line::from(vec![
            "CUTEX AGENT MESSAGE".magenta().bold(),
            " from ".dim(),
            message.from.clone().cyan().bold(),
        ]),
        Line::from(vec!["reply: ".dim(), reply_command.yellow()]),
    ];
    let preview = truncate_text(&message.content, 2000);
    for line in preview.lines() {
        lines.push(Line::from(format!("  {line}")));
    }
    PlainHistoryCell::new(lines)
}

fn agent_path_for_label(label: &str) -> AgentPath {
    let mut segment = String::new();
    let mut last_underscore = false;
    for ch in label.chars() {
        let next = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '_'
        };
        if next == '_' && last_underscore {
            continue;
        }
        segment.push(next);
        last_underscore = next == '_';
        if segment.len() >= 48 {
            break;
        }
    }
    let segment = segment.trim_matches('_');
    let segment = if segment.is_empty() || segment == "root" {
        "external_agent"
    } else {
        segment
    };
    AgentPath::root().join(segment).unwrap_or_else(|_| {
        AgentPath::root()
            .join("external_agent")
            .expect("valid path")
    })
}

fn http_json(
    base_url: &str,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<&[u8]>,
) -> Result<Value, String> {
    let url = Url::parse(&format!("{base_url}{path}")).map_err(|err| err.to_string())?;
    if url.scheme() != "http" {
        return Err("only http:// cutex agent bus URLs are supported".to_string());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "cutex agent bus URL has no host".to_string())?;
    let port = url.port_or_known_default().unwrap_or(80);
    let mut stream = TcpStream::connect(format!("{host}:{port}")).map_err(|err| err.to_string())?;
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();

    let mut request_path = url.path().to_string();
    if request_path.is_empty() {
        request_path.push('/');
    }
    if let Some(query) = url.query() {
        request_path.push('?');
        request_path.push_str(query);
    }
    let body = body.unwrap_or(b"");
    let auth = token
        .filter(|token| !token.is_empty())
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let content_type = if body.is_empty() {
        String::new()
    } else {
        "Content-Type: application/json\r\n".to_string()
    };
    let request = format!(
        "{method} {request_path} HTTP/1.1\r\nHost: {host}:{port}\r\n{auth}{content_type}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|err| err.to_string())?;
    stream.write_all(body).map_err(|err| err.to_string())?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|err| err.to_string())?;
    let text = String::from_utf8_lossy(&response);
    let (headers, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_ref(), ""));
    if !headers.starts_with("HTTP/1.1 2") {
        return Err(format!(
            "cutex agent bus returned non-success: {headers} {body}"
        ));
    }
    if body.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(body).map_err(|err| err.to_string())
}
