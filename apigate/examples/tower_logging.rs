//! Tower/axum middleware on top of ApiGate:
//! adds `tower_http::TraceLayer` through `.with_router(...)`.

use std::net::SocketAddr;

use tower_http::trace::TraceLayer;
use tracing_subscriber::{EnvFilter, fmt};

#[apigate::service(prefix = "/sales")]
mod sales {
    #[apigate::get("/ping")]
    async fn ping() {}
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,apigate=debug,tower_http=debug"));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .compact()
        .init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    let app = apigate::App::builder()
        .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
        .build()?
        .with_router(|router| router.layer(TraceLayer::new_for_http()));

    print!(
        "\
tower_logging - http://{listen}

RUST_LOG=debug,apigate=debug,tower_http=debug cargo run --example tower_logging

Ping:
  curl http://{listen}/sales/ping

Upstream: caddy run --config apigate/examples/upstream/Caddyfile
"
    );

    apigate::run(listen, app).await?;
    Ok(())
}
