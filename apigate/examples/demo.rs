use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Общий state приложения (регистрируется через .state(), доступен как &T)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AppConfig {
    api_key: String,
}

// ---------------------------------------------------------------------------
// Per-request данные (передаются между хуками через scope.insert / scope.take)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct RequestMeta {
    request_id: String,
}

// ---------------------------------------------------------------------------
// Path-параметры (Deserialize + Clone)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct SaleIdPath {
    id: Uuid,
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

/// Проверяет api key через shared state (&AppConfig из .state())
#[apigate::hook]
async fn require_api_key(
    ctx: &mut apigate::PartsCtx<'_>,
    config: &AppConfig,
) -> apigate::HookResult {
    let key = ctx
        .header("x-api-key")
        .ok_or_else(|| apigate::ApigateError::forbidden("missing x-api-key"))?;
    if key != config.api_key {
        return Err(apigate::ApigateError::forbidden("invalid x-api-key"));
    }
    Ok(())
}

/// Проверяет токен авторизации, ставит заголовки для upstream
#[apigate::hook]
async fn inject_user_headers(ctx: &mut apigate::PartsCtx<'_>) -> apigate::HookResult {
    let _token = ctx
        .header("authorization")
        .ok_or_else(|| apigate::ApigateError::unauthorized("missing authorization"))?;
    ctx.set_header("x-user-id", "11111111-1111-1111-1111-111111111111")?;
    ctx.set_header("x-user-role", "demo-user")?;
    Ok(())
}

/// Генерирует request-id и сохраняет RequestMeta в scope для следующих хуков
#[apigate::hook]
async fn set_request_id(
    ctx: &mut apigate::PartsCtx<'_>,
    scope: &mut apigate::RequestScope,
) -> apigate::HookResult {
    let id = Uuid::new_v4().to_string();
    ctx.set_header("x-request-id", &id)?;
    scope.insert(RequestMeta { request_id: id });
    Ok(())
}

/// Забирает RequestMeta из scope (вставленный set_request_id) через owned-параметр
#[apigate::hook]
async fn log_request_meta(meta: RequestMeta) -> apigate::HookResult {
    println!("[hook] request_id={}", meta.request_id);
    Ok(())
}

/// Читает path-параметры через &T (без удаления из scope)
#[apigate::hook]
async fn log_sale_access(
    path: &SaleIdPath,
    ctx: &mut apigate::PartsCtx<'_>,
) -> apigate::HookResult {
    println!("[hook] sale id={}", path.id);
    ctx.set_header("x-sale-id", &path.id.to_string())?;
    Ok(())
}

/// Добавляет маркер-заголовок
#[apigate::hook]
async fn mark_demo(ctx: &mut apigate::PartsCtx<'_>) -> apigate::HookResult {
    ctx.set_header("x-apigate-demo", "1")?;
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

/// Преобразование JSON + доступ к path-параметрам (&SaleIdPath) в map
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

#[apigate::service(name = "sales", prefix = "/sales", policy = "sales_sticky")]
mod sales {
    use super::*;

    /// Простой passthrough без хуков и валидации
    #[apigate::get("/ping")]
    async fn ping() {}

    /// Переписывает путь: /public -> /internal
    #[apigate::get("/public", to = "/internal", before = [mark_demo])]
    async fn public_alias() {}

    /// Shared state в хуке: require_api_key получает &AppConfig
    #[apigate::get("/admin/stats", before = [require_api_key])]
    async fn admin_stats() {}

    /// Аутентификация: проверяет токен, инжектирует заголовки в upstream
    #[apigate::get("/user", before = [inject_user_headers])]
    async fn user_profile() {}

    /// Цепочка из 4 хуков + передача данных между ними через scope:
    /// require_api_key -> inject_user_headers -> set_request_id -> log_request_meta
    #[apigate::get(
        "/secure-user",
        before = [require_api_key, inject_user_headers, set_request_id, log_request_meta]
    )]
    async fn secure_user_profile() {}

    /// Валидация path (id = UUID) + доступ к path в хуке через &SaleIdPath
    #[apigate::get("/{id}", path = SaleIdPath, before = [log_sale_access, mark_demo])]
    async fn get_by_id() {}

    /// Path + json + map: map-функция получает &SaleIdPath из scope
    #[apigate::post(
        "/{id}/update",
        path = SaleIdPath,
        json = UpdateSaleInput,
        before = [inject_user_headers],
        map = remap_update_sale
    )]
    async fn update_sale() {}

    /// Rewrite-шаблон: /item/{id}/review -> /api/v2/reviews/{id}
    #[apigate::get("/item/{id}/review", to = "/api/v2/reviews/{id}")]
    async fn item_review() {}

    /// Преобразование query-параметров через map
    #[apigate::get("/products", query = ProductsQuery, map = remap_products_query)]
    async fn get_products() {}

    /// Преобразование JSON через map + shared state в map-функции
    #[apigate::post(
        "/buy",
        json = PublicBuyInput,
        before = [inject_user_headers],
        map = remap_buy_json
    )]
    async fn buy() {}

    /// Преобразование form через map
    #[apigate::post(
        "/legacy-create",
        form = LegacyFormPublic,
        before = [require_api_key],
        map = remap_legacy_form
    )]
    async fn legacy_create() {}

    /// Multipart: passthrough без чтения тела
    #[apigate::post("/upload", multipart, before = [inject_user_headers])]
    async fn upload() {}

    /// Фоллбек
    #[apigate::get("/anything")]
    async fn anything() {}
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
        .request_timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(3))
        .pool_idle_timeout(std::time::Duration::from_secs(60))

        // Sticky sessions: запросы с одинаковым x-user-id идут на один backend
        .policy(
            "sales_sticky",
            apigate::Policy::new()
                .router(apigate::routing::HeaderSticky::new("x-user-id"))
                .balancer(apigate::balancing::ConsistentHash::new()),
        )
        .mount(sales::routes())
        .build()
        .map_err(anyhow::Error::msg)?;

    print!("\
apigate demo — http://{listen}

Passthrough:     curl http://{listen}/sales/ping
Rewrite:         curl http://{listen}/sales/public
Shared state:    curl -H 'x-api-key: secret-key' http://{listen}/sales/admin/stats
Auth:            curl -H 'authorization: Bearer t' http://{listen}/sales/user
Hook chain:      curl -H 'x-api-key: secret-key' -H 'authorization: Bearer t' http://{listen}/sales/secure-user
Path valid:      curl http://{listen}/sales/11111111-1111-1111-1111-111111111111
Path invalid:    curl http://{listen}/sales/not-a-uuid
Path in map:     curl -X POST -H 'authorization: Bearer t' -H 'content-type: application/json' \
                   -d '{{\"title\":\"New\"}}' http://{listen}/sales/11111111-1111-1111-1111-111111111111/update
Rewrite tpl:     curl http://{listen}/sales/item/abc/review
Query map:       curl 'http://{listen}/sales/products?page=2&size=5&q=test'
Json map:        curl -X POST -H 'authorization: Bearer t' -H 'content-type: application/json' \
                   -d '{{\"sale_ids\":[\"11111111-1111-1111-1111-111111111111\"],\"coupon\":\"sale10\"}}' http://{listen}/sales/buy
Form map:        curl -X POST -H 'x-api-key: secret-key' -H 'content-type: application/x-www-form-urlencoded' \
                   -d 'title=Demo&category=pets' http://{listen}/sales/legacy-create

Start upstream:  caddy run --config apigate/examples/upstream/Caddyfile
");

    apigate::run(listen, app).await?;
    Ok(())
}
