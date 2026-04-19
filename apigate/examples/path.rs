//! Path parameters: validation, hook access through `&T`, and map access
//! through `&T` from `RequestScope`.

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Path parameters (`Deserialize + Clone`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct SaleIdPath {
    id: Uuid,
}

// ---------------------------------------------------------------------------
// Map types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct UpdateSaleInput {
    title: String,
}

#[derive(Debug, Serialize)]
struct UpdateSaleService {
    sale_id: String,
    title: String,
}

// ---------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------

/// Reads path parameters through `&T` without removing them from scope.
#[apigate::hook]
async fn log_sale_access(path: &SaleIdPath, ctx: &mut apigate::PartsCtx) -> apigate::HookResult {
    println!("[hook] sale id={}", path.id);
    ctx.set_header("x-sale-id", path.id.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Map functions
// ---------------------------------------------------------------------------

/// Reads path parameters (`&SaleIdPath`) inside a map function.
#[apigate::map]
async fn remap_update_sale(
    input: UpdateSaleInput,
    path: &SaleIdPath,
) -> apigate::MapResult<UpdateSaleService> {
    Ok(UpdateSaleService {
        sale_id: path.id.to_string(),
        title: input.title.trim().to_string(),
    })
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

#[apigate::service(name = "sales", prefix = "/sales")]
mod sales {
    use super::*;

    /// Validates `id` as UUID and exposes path data to the hook.
    #[apigate::get("/{id}", path = SaleIdPath, before = [log_sale_access])]
    async fn get_by_id() {}

    /// Path + JSON + map: the map receives `&SaleIdPath` from scope.
    #[apigate::post("/{id}/update", path = SaleIdPath, json = UpdateSaleInput, map = remap_update_sale)]
    async fn update_sale() {}
}

// ---------------------------------------------------------------------------
// Entrypoint
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    let app = apigate::App::builder()
        .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
        .build()?;

    print!("\
path - http://{listen}

Valid UUID:      curl http://{listen}/sales/11111111-1111-1111-1111-111111111111
Invalid:         curl http://{listen}/sales/not-a-uuid
Path in map:     curl -X POST -H 'content-type: application/json' \
                   -d '{{\"title\":\"New\"}}' http://{listen}/sales/11111111-1111-1111-1111-111111111111/update

Upstream:        caddy run --config apigate/examples/upstream/Caddyfile
");

    apigate::run(listen, app).await?;
    Ok(())
}
