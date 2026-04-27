use std::net::SocketAddr;

use axum::Router;
use axum::serve::ListenerExt;
use socket2::{Domain, Protocol, Socket, Type};

use super::App;

/// Listener-side options for [`run_with`] and [`run_router_with`].
///
/// Defaults match [`run`] and [`run_router`]. Use this when the gateway owns
/// the listener and you need to tune socket-level settings.
#[derive(Debug, Clone)]
pub struct ServeConfig {
    pub(super) backlog: Option<u32>,
    pub(super) reuse_address: Option<bool>,
    #[cfg(all(
        unix,
        not(any(target_os = "solaris", target_os = "illumos", target_os = "cygwin"))
    ))]
    pub(super) reuse_port: Option<bool>,
    pub(super) ipv6_only: Option<bool>,
    pub(super) recv_buffer_size: Option<usize>,
    pub(super) send_buffer_size: Option<usize>,
    pub(super) tcp_nodelay: Option<bool>,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl ServeConfig {
    /// Creates a config that lets the OS choose the listen backlog.
    #[must_use]
    pub fn new() -> Self {
        Self {
            backlog: None,
            reuse_address: None,
            #[cfg(all(
                unix,
                not(any(target_os = "solaris", target_os = "illumos", target_os = "cygwin"))
            ))]
            reuse_port: None,
            ipv6_only: None,
            recv_buffer_size: None,
            send_buffer_size: None,
            tcp_nodelay: None,
        }
    }

    /// Sets the `listen(2)` backlog for the server socket.
    ///
    /// Larger values can absorb short connection bursts when the accept loop is
    /// briefly behind. Kernels may clamp this value to their configured maximum.
    #[must_use]
    pub fn backlog(mut self, backlog: u32) -> Self {
        self.backlog = Some(backlog);
        self
    }

    /// Sets `SO_REUSEADDR` before binding the listener socket.
    ///
    /// Leaving this unset preserves the platform defaults used by [`run`].
    #[must_use]
    pub fn reuse_address(mut self, reuse: bool) -> Self {
        self.reuse_address = Some(reuse);
        self
    }

    /// Sets `SO_REUSEPORT` before binding the listener socket.
    ///
    /// This is useful for multi-process accept/load distribution on platforms
    /// that support it.
    #[cfg(all(
        unix,
        not(any(target_os = "solaris", target_os = "illumos", target_os = "cygwin"))
    ))]
    #[must_use]
    pub fn reuse_port(mut self, reuse: bool) -> Self {
        self.reuse_port = Some(reuse);
        self
    }

    /// Sets `IPV6_V6ONLY` before binding an IPv6 listener socket.
    ///
    /// This option is ignored for IPv4 listen addresses.
    #[must_use]
    pub fn ipv6_only(mut self, only_v6: bool) -> Self {
        self.ipv6_only = Some(only_v6);
        self
    }

    /// Sets `SO_RCVBUF` on the listener socket before binding.
    #[must_use]
    pub fn recv_buffer_size(mut self, bytes: usize) -> Self {
        self.recv_buffer_size = Some(bytes);
        self
    }

    /// Sets `SO_SNDBUF` on the listener socket before binding.
    #[must_use]
    pub fn send_buffer_size(mut self, bytes: usize) -> Self {
        self.send_buffer_size = Some(bytes);
        self
    }

    /// Sets `TCP_NODELAY` on every accepted inbound TCP stream.
    #[must_use]
    pub fn tcp_nodelay(mut self, nodelay: bool) -> Self {
        self.tcp_nodelay = Some(nodelay);
        self
    }
}

/// Serves an [`App`] on the provided socket address.
pub async fn run(addr: SocketAddr, app: App) -> std::io::Result<()> {
    run_router_with(addr, app.router, ServeConfig::default()).await
}

/// Serves an [`App`] with custom listener configuration.
///
/// See [`ServeConfig`] for the available knobs.
pub async fn run_with(addr: SocketAddr, app: App, config: ServeConfig) -> std::io::Result<()> {
    run_router_with(addr, app.router, config).await
}

/// Runs a pre-built axum router.
///
/// Useful when you need full control over outer tower/axum middleware stack.
pub async fn run_router(addr: SocketAddr, router: Router) -> std::io::Result<()> {
    run_router_with(addr, router, ServeConfig::default()).await
}

/// Runs a pre-built axum router with custom listener configuration.
pub async fn run_router_with(
    addr: SocketAddr,
    router: Router,
    config: ServeConfig,
) -> std::io::Result<()> {
    let tcp_nodelay = config.tcp_nodelay;
    let listener = bind_listener(addr, &config).await?;

    if let Some(nodelay) = tcp_nodelay {
        return axum::serve(
            listener.tap_io(move |stream| {
                if let Err(err) = stream.set_nodelay(nodelay) {
                    tracing::trace!("failed to set TCP_NODELAY on accepted connection: {err}");
                }
            }),
            router,
        )
        .await;
    }

    axum::serve(listener, router).await
}

async fn bind_listener(
    addr: SocketAddr,
    config: &ServeConfig,
) -> std::io::Result<tokio::net::TcpListener> {
    if !needs_custom_bind(config) {
        return tokio::net::TcpListener::bind(addr).await;
    }

    let domain = match addr {
        SocketAddr::V4(_) => Domain::IPV4,
        SocketAddr::V6(_) => Domain::IPV6,
    };
    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    if let SocketAddr::V6(_) = addr
        && let Some(only_v6) = config.ipv6_only
    {
        socket.set_only_v6(only_v6)?;
    }
    socket.set_reuse_address(config.reuse_address.unwrap_or_else(default_reuse_address))?;
    #[cfg(all(
        unix,
        not(any(target_os = "solaris", target_os = "illumos", target_os = "cygwin"))
    ))]
    if let Some(reuse_port) = config.reuse_port {
        socket.set_reuse_port(reuse_port)?;
    }
    if let Some(bytes) = config.recv_buffer_size {
        socket.set_recv_buffer_size(bytes)?;
    }
    if let Some(bytes) = config.send_buffer_size {
        socket.set_send_buffer_size(bytes)?;
    }
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(listen_backlog(config.backlog))?;

    tokio::net::TcpListener::from_std(socket.into())
}

fn needs_custom_bind(config: &ServeConfig) -> bool {
    config.backlog.is_some()
        || config.reuse_address.is_some()
        || config.ipv6_only.is_some()
        || config.recv_buffer_size.is_some()
        || config.send_buffer_size.is_some()
        || {
            #[cfg(all(
                unix,
                not(any(target_os = "solaris", target_os = "illumos", target_os = "cygwin"))
            ))]
            {
                config.reuse_port.is_some()
            }
            #[cfg(not(all(
                unix,
                not(any(target_os = "solaris", target_os = "illumos", target_os = "cygwin"))
            )))]
            {
                false
            }
        }
}

fn listen_backlog(backlog: Option<u32>) -> i32 {
    match backlog {
        Some(backlog) => backlog.try_into().unwrap_or(i32::MAX),
        None => default_listen_backlog(),
    }
}

fn default_reuse_address() -> bool {
    !cfg!(windows)
}

#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_vendor = "apple"
))]
fn default_listen_backlog() -> i32 {
    -1
}

#[cfg(any(
    target_os = "windows",
    target_os = "redox",
    target_os = "espidf",
    target_os = "horizon"
))]
fn default_listen_backlog() -> i32 {
    128
}

#[cfg(target_os = "hermit")]
fn default_listen_backlog() -> i32 {
    1024
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_vendor = "apple",
    target_os = "windows",
    target_os = "redox",
    target_os = "espidf",
    target_os = "horizon",
    target_os = "hermit"
)))]
fn default_listen_backlog() -> i32 {
    i32::MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serve_config_defaults_to_os_backlog() {
        let cfg = ServeConfig::default();
        assert!(cfg.backlog.is_none());
        assert!(cfg.tcp_nodelay.is_none());
    }

    #[test]
    fn serve_config_records_explicit_backlog() {
        let cfg = ServeConfig::new().backlog(1024);
        assert_eq!(cfg.backlog, Some(1024));
    }

    #[test]
    fn serve_config_records_socket_options() {
        let cfg = ServeConfig::new()
            .reuse_address(true)
            .ipv6_only(true)
            .recv_buffer_size(256 * 1024)
            .send_buffer_size(256 * 1024)
            .tcp_nodelay(true);

        assert_eq!(cfg.reuse_address, Some(true));
        assert_eq!(cfg.ipv6_only, Some(true));
        assert_eq!(cfg.recv_buffer_size, Some(256 * 1024));
        assert_eq!(cfg.send_buffer_size, Some(256 * 1024));
        assert_eq!(cfg.tcp_nodelay, Some(true));
    }

    #[cfg(all(
        unix,
        not(any(target_os = "solaris", target_os = "illumos", target_os = "cygwin"))
    ))]
    #[test]
    fn serve_config_records_reuse_port() {
        let cfg = ServeConfig::new().reuse_port(true);
        assert_eq!(cfg.reuse_port, Some(true));
    }

    #[test]
    fn serve_config_saturates_backlog_to_i32_max_for_socket2() {
        let cfg = ServeConfig::new().backlog(u32::MAX);
        let backlog = cfg.backlog.unwrap().try_into().unwrap_or(i32::MAX);
        assert_eq!(backlog, i32::MAX);
    }

    #[tokio::test]
    async fn bind_listener_with_backlog_succeeds_on_loopback() {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = bind_listener(addr, &ServeConfig::new().backlog(1024))
            .await
            .expect("listener bind");
        let local = listener.local_addr().expect("local_addr");
        assert!(local.port() != 0);
    }

    #[tokio::test]
    async fn bind_listener_without_backlog_uses_tokio_bind_path() {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = bind_listener(addr, &ServeConfig::new())
            .await
            .expect("listener bind");
        let local = listener.local_addr().expect("local_addr");
        assert!(local.port() != 0);
    }

    #[tokio::test]
    async fn bind_listener_with_socket_options_succeeds_on_loopback() {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = bind_listener(
            addr,
            &ServeConfig::new()
                .reuse_address(true)
                .recv_buffer_size(128 * 1024)
                .send_buffer_size(128 * 1024),
        )
        .await
        .expect("listener bind");
        let local = listener.local_addr().expect("local_addr");
        assert!(local.port() != 0);
    }
}
