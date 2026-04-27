use std::time::Duration;

use axum::body::Body;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::{Builder as HyperClientBuilder, Client};
use hyper_util::rt::{TokioExecutor, TokioTimer};

type ClientBuilderConfigurer = Box<dyn FnOnce(&mut HyperClientBuilder) + Send + 'static>;
type ConnectorConfigurer = Box<dyn FnOnce(&mut HttpConnector) + Send + 'static>;

/// Upstream client and TCP socket configuration.
///
/// `AppBuilder` keeps common gateway settings at the top level. Use this
/// config for transport-oriented tuning and the hyper-util escape hatches.
pub struct UpstreamConfig {
    pub(super) connect_timeout: Duration,
    pub(super) pool_idle_timeout: Duration,
    pub(super) pool_max_idle_per_host: usize,
    tcp_nodelay: bool,
    client_configurers: Vec<ClientBuilderConfigurer>,
    connector_configurers: Vec<ConnectorConfigurer>,
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl UpstreamConfig {
    /// Creates an upstream config matching ApiGate defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            pool_idle_timeout: Duration::from_secs(90),
            pool_max_idle_per_host: usize::MAX,
            tcp_nodelay: true,
            client_configurers: Vec::new(),
            connector_configurers: Vec::new(),
        }
    }

    /// Sets the TCP connect timeout for upstream connections.
    #[must_use]
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Sets how long idle upstream connections are kept in the client pool.
    #[must_use]
    pub fn pool_idle_timeout(mut self, timeout: Duration) -> Self {
        self.pool_idle_timeout = timeout;
        self
    }

    /// Sets the maximum idle upstream connections kept per host.
    ///
    /// The default is `usize::MAX`, matching hyper-util's default.
    #[must_use]
    pub fn pool_max_idle_per_host(mut self, max_idle: usize) -> Self {
        self.pool_max_idle_per_host = max_idle;
        self
    }

    /// Sets `TCP_NODELAY` for upstream TCP connections.
    ///
    /// Enabled by default to avoid delayed ACK/Nagle latency on proxied calls.
    #[must_use]
    pub fn tcp_nodelay(mut self, nodelay: bool) -> Self {
        self.tcp_nodelay = nodelay;
        self
    }

    /// Applies custom configuration to hyper-util's upstream client builder.
    ///
    /// The closure runs after ApiGate's built-in client settings, so it can
    /// set less common hyper-util knobs such as HTTP/1 buffers, HTTP/2 windows,
    /// keep-alive pings, retry behavior, or host-header handling.
    #[must_use]
    pub fn configure_client<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(&mut HyperClientBuilder) + Send + 'static,
    {
        self.client_configurers.push(Box::new(configure));
        self
    }

    /// Applies custom configuration to hyper-util's upstream `HttpConnector`.
    ///
    /// The closure runs after ApiGate's built-in connector settings, so it can
    /// set platform-specific socket options exposed by hyper-util.
    #[must_use]
    pub fn configure_connector<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(&mut HttpConnector) + Send + 'static,
    {
        self.connector_configurers.push(Box::new(configure));
        self
    }

    pub(super) fn build_client(self) -> Client<HttpConnector, Body> {
        let mut connector = HttpConnector::new();
        connector.set_nodelay(self.tcp_nodelay);
        connector.set_connect_timeout(Some(self.connect_timeout));
        connector.set_keepalive(Some(self.pool_idle_timeout));
        for configure in self.connector_configurers {
            configure(&mut connector);
        }

        let mut client_builder = Client::builder(TokioExecutor::new());
        client_builder
            .pool_timer(TokioTimer::new())
            .pool_idle_timeout(self.pool_idle_timeout)
            .pool_max_idle_per_host(self.pool_max_idle_per_host);
        for configure in self.client_configurers {
            configure(&mut client_builder);
        }

        client_builder.build(connector)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn upstream_config_records_socket_options() {
        let cfg = UpstreamConfig::new()
            .pool_max_idle_per_host(128)
            .tcp_nodelay(false);

        assert_eq!(cfg.pool_max_idle_per_host, 128);
        assert!(!cfg.tcp_nodelay);
    }

    #[test]
    fn upstream_config_runs_configurers_while_building_client() {
        let client_configured = Arc::new(AtomicBool::new(false));
        let connector_configured = Arc::new(AtomicBool::new(false));

        let client_flag = Arc::clone(&client_configured);
        let connector_flag = Arc::clone(&connector_configured);

        let _client = UpstreamConfig::new()
            .configure_client(move |client| {
                client.pool_max_idle_per_host(8);
                client_configured.store(true, Ordering::SeqCst);
            })
            .configure_connector(move |connector| {
                connector.set_nodelay(true);
                connector_configured.store(true, Ordering::SeqCst);
            })
            .build_client();

        assert!(client_flag.load(Ordering::SeqCst));
        assert!(connector_flag.load(Ordering::SeqCst));
    }
}
