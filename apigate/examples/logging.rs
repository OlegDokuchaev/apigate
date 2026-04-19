//! Runtime observability: built-in tracing adapter
//! + кастомный observer для событий apigate.

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Debug, Deserialize, Serialize)]
struct BuyInput {
    sale_id: String,
}

#[apigate::hook]
async fn require_auth(ctx: &mut apigate::PartsCtx) -> apigate::HookResult {
    if ctx.header("authorization").is_none() {
        return Err(apigate::ApigateError::unauthorized("missing authorization"));
    }
    Ok(())
}

#[apigate::map]
async fn passthrough_buy(input: BuyInput) -> apigate::MapResult<BuyInput> {
    Ok(input)
}

#[apigate::service(name = "sales", prefix = "/sales")]
mod sales {
    use super::*;

    #[apigate::get("/ping")]
    async fn ping() {}

    #[apigate::post("/buy", json = BuyInput, before = [require_auth], map = passthrough_buy)]
    async fn buy() {}
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,apigate=debug,apigate::proxy=trace"));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .compact()
        .init();
}

fn observe(event: apigate::RuntimeEvent<'_>) {
    // Сохраняем дефолтный tracing-лог от apigate
    apigate::default_tracing_observer(event);

    // И добавляем свою доменную "надстройку" (например, аудит)
    if let apigate::RuntimeEventKind::UpstreamSucceeded {
        backend_index,
        status,
        upstream_latency,
    } = event.kind
    {
        tracing::info!(
            target: "app::audit",
            service = event.service,
            route = event.route_path,
            backend_index,
            status = status.as_u16(),
            latency = ?upstream_latency,
            "gateway request completed"
        );
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    let app = apigate::App::builder()
        .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
        .runtime_observer(observe)
        .build()?;

    print!(
        "\
logging — http://{listen}

RUST_LOG=debug,apigate=trace cargo run --example logging

Ping:
  curl http://{listen}/sales/ping

Pipeline error (no auth):
  curl -X POST -H 'content-type: application/json' \
    -d '{{\"sale_id\":\"111\"}}' http://{listen}/sales/buy

Success:
  curl -X POST -H 'authorization: Bearer t' -H 'content-type: application/json' \
    -d '{{\"sale_id\":\"111\"}}' http://{listen}/sales/buy

Upstream: caddy run --config apigate/examples/upstream/Caddyfile
"
    );

    apigate::run(listen, app).await?;
    Ok(())
}
