//! Map: преобразование query, json, form перед отправкой в upstream.
//! Shared state (&AppConfig) доступен в map-функциях.

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Общий state приложения (доступен как &T в map)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AppConfig {
    api_key: String,
}

// ---------------------------------------------------------------------------
// Типы для query map
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ProductsQuery {
    page: Option<u32>,
    size: Option<u32>,
    q: Option<String>,
}

#[derive(Debug, Serialize)]
struct ProductsQueryService {
    offset: u32,
    limit: u32,
    query: Option<String>,
}

// ---------------------------------------------------------------------------
// Типы для json map
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct PublicBuyInput {
    sale_ids: Vec<Uuid>,
    coupon: Option<String>,
    use_bonus_points: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ServiceBuyInput {
    sale_ids: Vec<Uuid>,
    promo_code: Option<String>,
    payment_mode: String,
    source: &'static str,
}

// ---------------------------------------------------------------------------
// Типы для form map
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct LegacyFormPublic {
    title: String,
    category: String,
}

#[derive(Debug, Serialize)]
struct LegacyFormService {
    title: String,
    category_code: String,
}

// ---------------------------------------------------------------------------
// Хуки
// ---------------------------------------------------------------------------

/// Аутентификация (нужна для /buy)
#[apigate::hook]
async fn inject_user_headers(ctx: &mut apigate::PartsCtx) -> apigate::HookResult {
    let _token = ctx
        .header("authorization")
        .ok_or_else(|| apigate::ApigateError::unauthorized("missing authorization"))?;
    ctx.set_header("x-user-id", "11111111-1111-1111-1111-111111111111")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Map-функции
// ---------------------------------------------------------------------------

/// Преобразование query: page/size -> offset/limit
#[apigate::map]
async fn remap_products_query(input: ProductsQuery) -> apigate::MapResult<ProductsQueryService> {
    let page = input.page.unwrap_or(1).max(1);
    let size = input.size.unwrap_or(20).clamp(1, 100);
    Ok(ProductsQueryService {
        offset: (page - 1) * size,
        limit: size,
        query: input
            .q
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
    })
}

/// Преобразование JSON + доступ к shared state (&AppConfig) в map
#[apigate::map]
async fn remap_buy_json(
    input: PublicBuyInput,
    config: &AppConfig,
) -> apigate::MapResult<ServiceBuyInput> {
    Ok(ServiceBuyInput {
        sale_ids: input.sale_ids,
        promo_code: input
            .coupon
            .map(|v| v.trim().to_uppercase())
            .filter(|v| !v.is_empty()),
        payment_mode: if input.use_bonus_points.unwrap_or(false) {
            "bonus"
        } else {
            "money"
        }
        .to_string(),
        source: if config.api_key.is_empty() {
            "unknown"
        } else {
            "apigate-demo"
        },
    })
}

/// Преобразование form: category -> category_code
#[apigate::map]
async fn remap_legacy_form(input: LegacyFormPublic) -> apigate::MapResult<LegacyFormService> {
    Ok(LegacyFormService {
        title: input.title.trim().to_string(),
        category_code: match input.category.trim().to_lowercase().as_str() {
            "pets" => "P",
            "items" => "I",
            _ => "U",
        }
        .to_string(),
    })
}

// ---------------------------------------------------------------------------
// Сервис
// ---------------------------------------------------------------------------

#[apigate::service(name = "sales", prefix = "/sales")]
mod sales {
    use super::*;

    /// Преобразование query-параметров через map
    #[apigate::get("/products", query = ProductsQuery, map = remap_products_query)]
    async fn get_products() {}

    /// Преобразование JSON через map + shared state в map-функции
    #[apigate::post("/buy", json = PublicBuyInput, before = [inject_user_headers], map = remap_buy_json)]
    async fn buy() {}

    /// Преобразование form через map
    #[apigate::post("/legacy-create", form = LegacyFormPublic, map = remap_legacy_form)]
    async fn legacy_create() {}
}

// ---------------------------------------------------------------------------
// Точка входа
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    let app = apigate::App::builder()
        .backend("sales", ["http://127.0.0.1:8081"])
        .state(AppConfig {
            api_key: "secret-key".to_string(),
        })
        .mount(sales::routes())
        .build()
        .map_err(anyhow::Error::msg)?;

    print!("\
map — http://{listen}

Query map:   curl 'http://{listen}/sales/products?page=2&size=5&q=test'
Json map:    curl -X POST -H 'authorization: Bearer t' -H 'content-type: application/json' \
               -d '{{\"sale_ids\":[\"11111111-1111-1111-1111-111111111111\"],\"coupon\":\"sale10\"}}' http://{listen}/sales/buy
Form map:    curl -X POST -H 'content-type: application/x-www-form-urlencoded' \
               -d 'title=Demo&category=pets' http://{listen}/sales/legacy-create

Upstream:    caddy run --config apigate/examples/upstream/Caddyfile
");

    apigate::run(listen, app).await?;
    Ok(())
}
