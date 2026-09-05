//! Narrow TLS backend fallback for delegated requests that select their route per destination.
//!
//! Native TLS remains the default. A recognized connection-time protocol negotiation failure can
//! select rustls for one HTTPS origin and outbound route without changing other destinations.

use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;

use crate::HttpClient;
use crate::OutboundProxyRoute;

const MAX_CACHED_RUSTLS_DESTINATIONS: usize = 16;
// Schannel maps TLS alert 70 (protocol_version) to SEC_E_UNSUPPORTED_FUNCTION.
const SCHANNEL_PROTOCOL_VERSION_ERROR: i32 = 0x8009_0302_u32 as i32;
const CERTIFICATE_ERROR_MARKERS: [&str; 9] = [
    "certificate",
    "unknown issuer",
    "unknown ca",
    "untrusted",
    "self signed",
    "self-signed",
    "hostname",
    "expired",
    "revoked",
];

#[derive(Clone, Default)]
pub(crate) struct RustlsClientCache {
    state: Arc<Mutex<RustlsClientCacheState>>,
}

#[derive(Default)]
struct RustlsClientCacheState {
    destinations: HashSet<DestinationRoute>,
    clients: HashMap<OutboundProxyRoute, HttpClient>,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct DestinationRoute {
    host: String,
    port: u16,
    route: OutboundProxyRoute,
}

impl RustlsClientCache {
    pub(crate) fn requires_rustls(&self, url: &reqwest::Url, route: &OutboundProxyRoute) -> bool {
        let Some(destination) = DestinationRoute::new(url, route) else {
            return false;
        };
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .destinations
            .contains(&destination)
    }

    pub(crate) fn client_for_route(&self, route: &OutboundProxyRoute) -> Option<HttpClient> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clients
            .get(route)
            .cloned()
    }

    pub(crate) fn remember(
        &self,
        url: &reqwest::Url,
        route: &OutboundProxyRoute,
        client: HttpClient,
    ) {
        let Some(destination) = DestinationRoute::new(url, route) else {
            return;
        };
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.destinations.contains(&destination) {
            return;
        }
        if state.destinations.len() >= MAX_CACHED_RUSTLS_DESTINATIONS
            && let Some(destination_to_evict) = state.destinations.iter().next().cloned()
        {
            state.destinations.remove(&destination_to_evict);
            if !state
                .destinations
                .iter()
                .any(|destination| destination.route == destination_to_evict.route)
            {
                state.clients.remove(&destination_to_evict.route);
            }
        }
        state.clients.entry(route.clone()).or_insert(client);
        state.destinations.insert(destination);
    }
}

impl DestinationRoute {
    fn new(url: &reqwest::Url, route: &OutboundProxyRoute) -> Option<Self> {
        if url.scheme() != "https" {
            return None;
        }
        Some(Self {
            host: url.host_str()?.to_ascii_lowercase(),
            port: url.port_or_known_default()?,
            route: route.clone(),
        })
    }
}

pub(crate) fn should_retry_with_rustls(error: &reqwest::Error) -> bool {
    error.is_connect() && !error.is_timeout() && error.source().is_some_and(has_retryable_tls_error)
}

fn has_retryable_tls_error(error: &(dyn Error + 'static)) -> bool {
    let mut source = Some(error);
    let mut recognized_negotiation_failure = false;

    while let Some(error) = source {
        let message = error.to_string().to_ascii_lowercase();
        if contains_certificate_error(&message) {
            return false;
        }

        if is_protocol_version_error(error, &message) {
            recognized_negotiation_failure = true;
        }
        source = error.source();
    }

    recognized_negotiation_failure
}

pub(crate) fn is_tls_error(error: &(dyn Error + 'static)) -> bool {
    if error.downcast_ref::<rustls::Error>().is_some()
        || error.downcast_ref::<native_tls::Error>().is_some()
    {
        return true;
    }

    let Some(error) = error.downcast_ref::<std::io::Error>() else {
        return false;
    };
    let message = error.to_string().to_ascii_lowercase();
    contains_certificate_error(&message) || is_protocol_version_error(error, &message)
}

fn contains_certificate_error(message: &str) -> bool {
    CERTIFICATE_ERROR_MARKERS
        .iter()
        .any(|marker| message.contains(marker))
}

fn is_protocol_version_error(error: &(dyn Error + 'static), message: &str) -> bool {
    // macOS Secure Transport reports the protocol alert as "bad protocol version".
    let is_macos_protocol_version_error = message.contains("bad protocol version");
    // Linux OpenSSL reports the peer's "tlsv1 alert protocol version". Rustls can be hidden inside
    // an opaque `std::io::Error` and expose only the peer alert in its display text.
    let is_linux_protocol_version_error = message.contains("tlsv1 alert protocol version")
        || message.contains("received fatal alert: protocolversion");
    // Windows Schannel may expose the protocol alert as a raw or formatted OS error.
    let is_schannel_protocol_version_error = error
        .downcast_ref::<std::io::Error>()
        .and_then(std::io::Error::raw_os_error)
        == Some(SCHANNEL_PROTOCOL_VERSION_ERROR)
        || message.contains("(os error -2146893054)")
        || message.contains("0x80090302");

    is_macos_protocol_version_error
        || is_linux_protocol_version_error
        || is_schannel_protocol_version_error
}

#[cfg(test)]
#[path = "tls_backend_fallback_tests.rs"]
mod tests;
