//! Path-параметры: валидация, доступ в хуках (&T), доступ в map-функциях.

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Path-параметры (Deserialize + Clone)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct SaleIdPath {
    id: Uuid,
}

// ---------------------------------------------------------------------------
// Типы для map
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
// Хуки
// ---------------------------------------------------------------------------

/// Читает path-параметры через &T (без удаления из scope)
#[apigate::hook]
async fn log_sale_access(path: &SaleIdPath, ctx: &mut apigate::PartsCtx) -> apigate::HookResult {
    println!("[hook] sale id={}", path.id);
    ctx.set_header("x-sale-id", &path.id.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Map-функции
// ---------------------------------------------------------------------------

/// Доступ к path-параметрам (&SaleIdPath) в map-функции
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
// Сервис
// ---------------------------------------------------------------------------

#[apigate::service(name = "sales", prefix = "/sales")]
mod sales {
    use super::*;

    /// Валидация path (id = UUID) + доступ к path в хуке через &SaleIdPath
    #[apigate::get("/{id}", path = SaleIdPath, before = [log_sale_access])]
    async fn get_by_id() {}

    /// Path + json + map: map-функция получает &SaleIdPath из scope
    #[apigate::post("/{id}/update", path = SaleIdPath, json = UpdateSaleInput, map = remap_update_sale)]
    async fn update_sale() {}
}

// ---------------------------------------------------------------------------
// Точка входа
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    let app = apigate::App::builder()
        .backend("sales", ["http://127.0.0.1:8081"])
        .mount(sales::routes())
        .build()
        .map_err(anyhow::Error::msg)?;

    print!("\
path — http://{listen}

Valid UUID:      curl http://{listen}/sales/11111111-1111-1111-1111-111111111111
Invalid:         curl http://{listen}/sales/not-a-uuid
Path in map:     curl -X POST -H 'content-type: application/json' \
                   -d '{{\"title\":\"New\"}}' http://{listen}/sales/11111111-1111-1111-1111-111111111111/update

Upstream:        caddy run --config apigate/examples/upstream/Caddyfile
");

    apigate::run(listen, app).await?;
    Ok(())
}
