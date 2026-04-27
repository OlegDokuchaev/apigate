//! Runtime tuning: listener socket options and upstream hyper-util client settings.

use std::net::SocketAddr;
use std::time::Duration;

#[apigate::service(prefix = "/sales")]
mod sales {
    #[apigate::get("/ping")]
    async fn ping() {}

    #[apigate::get("/anything")]
    async fn anything() {}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    let upstream = apigate::UpstreamConfig::default()
        .connect_timeout(Duration::from_secs(3))
        .pool_idle_timeout(Duration::from_secs(60))
        .pool_max_idle_per_host(256)
        .tcp_nodelay(true)
        .configure_client(|client| {
            client.http1_max_buf_size(1024 * 1024);
            client.http1_writev(true);
            client.http2_adaptive_window(true);
            client.http2_keep_alive_interval(Duration::from_secs(20));
            client.http2_keep_alive_timeout(Duration::from_secs(5));
            client.http2_keep_alive_while_idle(true);
            client.retry_canceled_requests(true);
        })
        .configure_connector(|connector| {
            connector.set_keepalive(Some(Duration::from_secs(30)));
            connector.set_keepalive_interval(Some(Duration::from_secs(10)));
            connector.set_keepalive_retries(Some(3));
            connector.set_recv_buffer_size(Some(512 * 1024));
            connector.set_send_buffer_size(Some(512 * 1024));
            connector.set_happy_eyeballs_timeout(Some(Duration::from_millis(200)));
            connector.set_reuse_address(true);
        });

    let app = apigate::App::builder()
        .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
        .request_timeout(Duration::from_secs(20))
        .upstream(upstream)
        .build()?;

    let serve = apigate::ServeConfig::new()
        .backlog(2048)
        .reuse_address(true)
        .recv_buffer_size(512 * 1024)
        .send_buffer_size(512 * 1024)
        .tcp_nodelay(true);

    print!(
        "\
runtime_tuning - http://{listen}

Ping:          curl http://{listen}/sales/ping
Passthrough:   curl http://{listen}/sales/anything

Upstream:      caddy run --config apigate/examples/upstream/Caddyfile
"
    );

    apigate::run_with(listen, app, serve).await?;
    Ok(())
}
