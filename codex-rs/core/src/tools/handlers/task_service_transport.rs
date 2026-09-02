use std::ffi::OsString;
use std::net::IpAddr;
use std::time::Duration;

use futures::StreamExt;
use reqwest::Client;
use reqwest::header::AUTHORIZATION;
use reqwest::header::CONTENT_TYPE;
use reqwest::header::HeaderValue;
use serde_json::Value;
use url::Host;
use url::Url;

const AGENT_BUS_URL_ENV: &str = "CUTEX_AGENT_BUS_URL";
const AGENT_BUS_TOKEN_ENV: &str = "CUTEX_AGENT_BUS_TOKEN";
const AGENT_ID_ENV: &str = "CUTEX_AGENT_ID";
const TASK_ACTION_PATH: &str = "/api/task/v2/actions";
const TASK_WORKER_PREPARE_PATH: &str = "/api/task/v2/worker-prepare";
const TASK_DIRECTOR_ACTION_PATH: &str = "/api/task/v2/director-action";
const MAX_PROVIDER_RESPONSE_BYTES: usize = 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) struct TaskServiceTransport {
    client: Client,
    integration: IntegrationState,
}

enum IntegrationState {
    Ready(Box<AuthenticatedIntegration>),
    Unavailable(&'static str),
}

struct AuthenticatedIntegration {
    action_endpoint: Url,
    prepare_endpoint: Url,
    director_action_endpoint: Url,
    authorization: HeaderValue,
    agent_id: HeaderValue,
}

#[derive(Clone, Copy)]
pub(super) enum TransportError {
    Unavailable(&'static str),
    ResponseUncertain,
    Rejected,
    InvalidResponse,
}

impl TaskServiceTransport {
    pub(super) fn from_environment() -> Option<Self> {
        let bus_url = std::env::var_os(AGENT_BUS_URL_ENV);
        let token = std::env::var_os(AGENT_BUS_TOKEN_ENV);
        let agent_id = std::env::var_os(AGENT_ID_ENV);
        if bus_url.is_none() && token.is_none() && agent_id.is_none() {
            return None;
        }
        Some(Self::new(bus_url, token, agent_id, REQUEST_TIMEOUT))
    }

    pub(super) fn new(
        bus_url: Option<OsString>,
        token: Option<OsString>,
        agent_id: Option<OsString>,
        timeout: Duration,
    ) -> Self {
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(timeout)
            .timeout(timeout)
            .build();
        let integration = match client.as_ref() {
            Ok(_) => resolve_integration(bus_url, token, agent_id),
            Err(_) => Err("integration_unavailable"),
        };
        Self {
            client: client.unwrap_or_default(),
            integration: match integration {
                Ok(integration) => IntegrationState::Ready(Box::new(integration)),
                Err(code) => IntegrationState::Unavailable(code),
            },
        }
    }

    pub(super) async fn post_action(&self, request_body: Vec<u8>) -> Result<Value, TransportError> {
        let IntegrationState::Ready(integration) = &self.integration else {
            let IntegrationState::Unavailable(code) = &self.integration else {
                unreachable!();
            };
            return Err(TransportError::Unavailable(code));
        };
        self.post(integration.action_endpoint.clone(), request_body)
            .await
    }

    pub(super) async fn post_prepare(
        &self,
        request_body: Vec<u8>,
    ) -> Result<Value, TransportError> {
        let IntegrationState::Ready(integration) = &self.integration else {
            let IntegrationState::Unavailable(code) = &self.integration else {
                unreachable!();
            };
            return Err(TransportError::Unavailable(code));
        };
        self.post(integration.prepare_endpoint.clone(), request_body)
            .await
    }

    pub(super) async fn post_director_action(
        &self,
        request_body: Vec<u8>,
    ) -> Result<Value, TransportError> {
        let IntegrationState::Ready(integration) = &self.integration else {
            let IntegrationState::Unavailable(code) = &self.integration else {
                unreachable!();
            };
            return Err(TransportError::Unavailable(code));
        };
        self.post(integration.director_action_endpoint.clone(), request_body)
            .await
    }

    async fn post(&self, endpoint: Url, request_body: Vec<u8>) -> Result<Value, TransportError> {
        for attempt in 0..2 {
            match self.post_once(endpoint.clone(), request_body.clone()).await {
                Err(PostError::Retryable) if attempt == 0 => continue,
                Err(PostError::Retryable) => return Err(TransportError::ResponseUncertain),
                Err(PostError::Rejected) => return Err(TransportError::Rejected),
                Err(PostError::InvalidResponse) => return Err(TransportError::InvalidResponse),
                Ok(response) => return Ok(response),
            }
        }
        unreachable!()
    }

    async fn post_once(&self, endpoint: Url, request_body: Vec<u8>) -> Result<Value, PostError> {
        let IntegrationState::Ready(integration) = &self.integration else {
            return Err(PostError::Rejected);
        };
        let response = self
            .client
            .post(endpoint)
            .header(AUTHORIZATION, integration.authorization.clone())
            .header("x-cutex-agent-id", integration.agent_id.clone())
            .header(CONTENT_TYPE, "application/json")
            .body(request_body)
            .send()
            .await
            .map_err(|_| PostError::Retryable)?;
        if response.status().is_server_error() || response.status().as_u16() == 408 {
            return Err(PostError::Retryable);
        }
        if !response.status().is_success() {
            return Err(PostError::Rejected);
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_PROVIDER_RESPONSE_BYTES as u64)
        {
            return Err(PostError::InvalidResponse);
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| PostError::Retryable)?;
            if bytes.len().saturating_add(chunk.len()) > MAX_PROVIDER_RESPONSE_BYTES {
                return Err(PostError::InvalidResponse);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| PostError::InvalidResponse)
    }
}

#[derive(Clone, Copy)]
enum PostError {
    Retryable,
    Rejected,
    InvalidResponse,
}

fn resolve_integration(
    bus_url: Option<OsString>,
    token: Option<OsString>,
    agent_id: Option<OsString>,
) -> Result<AuthenticatedIntegration, &'static str> {
    let bus_url = unicode_env(bus_url)?;
    let token = unicode_env(token)?;
    let agent_id = unicode_env(agent_id)?;
    if token.is_empty() || token.len() > 4096 || agent_id.is_empty() || agent_id.len() > 256 {
        return Err("missing_authenticated_integration");
    }
    let mut endpoint = Url::parse(&bus_url).map_err(|_| "insecure_integration")?;
    if endpoint.scheme() != "http"
        || !matches!(endpoint.path(), "" | "/")
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || !host_is_loopback(endpoint.host())
    {
        return Err("insecure_integration");
    }
    let mut action_endpoint = endpoint.clone();
    action_endpoint.set_path(TASK_ACTION_PATH);
    let mut director_action_endpoint = endpoint.clone();
    director_action_endpoint.set_path(TASK_DIRECTOR_ACTION_PATH);
    endpoint.set_path(TASK_WORKER_PREPARE_PATH);
    let authorization = HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| "missing_authenticated_integration")?;
    let agent_id =
        HeaderValue::from_str(&agent_id).map_err(|_| "missing_authenticated_integration")?;
    Ok(AuthenticatedIntegration {
        action_endpoint,
        prepare_endpoint: endpoint,
        director_action_endpoint,
        authorization,
        agent_id,
    })
}

fn unicode_env(value: Option<OsString>) -> Result<String, &'static str> {
    value
        .ok_or("missing_authenticated_integration")?
        .into_string()
        .map_err(|_| "missing_authenticated_integration")
}

fn host_is_loopback(host: Option<Host<&str>>) -> bool {
    match host {
        Some(Host::Ipv4(address)) => IpAddr::V4(address).is_loopback(),
        Some(Host::Ipv6(address)) => IpAddr::V6(address).is_loopback(),
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        None => false,
    }
}
