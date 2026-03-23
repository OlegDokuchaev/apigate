use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct SaleIdPath {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ProductsQueryPublic {
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

#[derive(Debug, Deserialize)]
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
struct LegacyFormPublic {
    title: String,
    category: String,
}

#[derive(Debug, Serialize)]
struct LegacyFormService {
    title: String,
    category_code: String,
}

#[apigate::hook]
async fn require_api_key(ctx: &mut apigate::PartsCtx<'_>) -> apigate::HookResult {
    let api_key = ctx
        .header("x-api-key")
        .ok_or_else(|| apigate::ApigateError::forbidden("missing x-api-key"))?;

    if api_key != "secret-key" {
        return Err(apigate::ApigateError::forbidden("invalid x-api-key"));
    }

    Ok(())
}

#[apigate::hook]
async fn inject_user_headers(ctx: &mut apigate::PartsCtx<'_>) -> apigate::HookResult {
    let _auth = ctx
        .header("authorization")
        .ok_or_else(|| apigate::ApigateError::unauthorized("missing authorization"))?;

    ctx.set_header("x-user-id", "11111111-1111-1111-1111-111111111111")?;
    ctx.set_header("x-user-role", "demo-user")?;
    ctx.set_header_if_absent("x-request-id", "demo-request-id")?;

    Ok(())
}

#[apigate::hook]
async fn mark_demo_request(ctx: &mut apigate::PartsCtx<'_>) -> apigate::HookResult {
    ctx.set_header("x-apigate-demo", "1")?;
    Ok(())
}

#[apigate::map]
async fn remap_products_query(
    input: ProductsQueryPublic,
    _ctx: &mut apigate::PartsCtx<'_>,
) -> apigate::MapResult<ProductsQueryService> {
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

#[apigate::map]
async fn remap_buy_json(
    input: PublicBuyInput,
    _ctx: &mut apigate::PartsCtx<'_>,
) -> apigate::MapResult<ServiceBuyInput> {
    let promo_code = input
        .coupon
        .map(|v| v.trim().to_uppercase())
        .filter(|v| !v.is_empty());

    let payment_mode = if input.use_bonus_points.unwrap_or(false) {
        "bonus".to_string()
    } else {
        "money".to_string()
    };

    Ok(ServiceBuyInput {
        sale_ids: input.sale_ids,
        promo_code,
        payment_mode,
        source: "apigate-demo",
    })
}

#[apigate::map]
async fn remap_legacy_form(
    input: LegacyFormPublic,
    _ctx: &mut apigate::PartsCtx<'_>,
) -> apigate::MapResult<LegacyFormService> {
    let category_code = match input.category.trim().to_lowercase().as_str() {
        "pets" => "P",
        "items" => "I",
        _ => "U",
    };

    Ok(LegacyFormService {
        title: input.title.trim().to_string(),
        category_code: category_code.to_string(),
    })
}

#[apigate::service(name = "sales", prefix = "/sales")]
mod sales {
    use super::*;

    // Обычный passthrough без хуков
    #[apigate::get("/ping")]
    async fn ping() {}

    // Alias + before
    #[apigate::get("/public", to = "/internal", before = [mark_demo_request])]
    async fn public_alias() {}

    // Защищённый маршрут: нужен x-api-key
    #[apigate::get("/admin/stats", before = [require_api_key])]
    async fn admin_stats() {}

    // Хук добавляет заголовки, которые уйдут в upstream
    #[apigate::get("/user", before = [inject_user_headers])]
    async fn user_profile() {}

    // Можно комбинировать несколько before-хуков
    #[apigate::get(
        "/secure-user",
        before = [require_api_key, inject_user_headers, mark_demo_request]
    )]
    async fn secure_user_profile() {}

    // Маршрут с параметром
    #[apigate::get(
        "/{id}",
        path = SaleIdPath,
        before = [mark_demo_request]
    )]
    async fn get_by_id() {}

    // Изменение query
    #[apigate::get(
        "/products",
        query = ProductsQueryPublic,
        map = remap_products_query
    )]
    async fn get_products() {}

    // Изменение json
    #[apigate::post(
        "/buy",
        json = PublicBuyInput,
        before = [inject_user_headers],
        map = remap_buy_json
    )]
    async fn buy() {}

    // Изменение form
    #[apigate::post(
        "/legacy-create",
        form = LegacyFormPublic,
        before = [require_api_key],
        map = remap_legacy_form
    )]
    async fn legacy_create() {}

    // Фоллбек для всего остального внутри /sales/*
    #[apigate::get("/anything")]
    async fn anything() {}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    // Минимальный upstream (один адрес) для сервиса "sales"
    let app = apigate::App::builder()
        .backend("sales", ["http://127.0.0.1:8081"])
        .mount(sales::routes())
        .build()
        .map_err(anyhow::Error::msg)?;

    println!("apigate demo listening on http://{listen}");
    println!();
    println!("Try:");
    println!("  curl -i http://127.0.0.1:8080/sales/ping");
    println!("  curl -i http://127.0.0.1:8080/sales/public");
    println!("  curl -i -H 'x-api-key: secret-key' http://127.0.0.1:8080/sales/admin/stats");
    println!("  curl -i -H 'authorization: Bearer test' http://127.0.0.1:8080/sales/user");
    println!(
        "  curl -i -H 'x-api-key: secret-key' -H 'authorization: Bearer test' http://127.0.0.1:8080/sales/secure-user"
    );
    println!("  curl -i 'http://127.0.0.1:8080/sales/products?page=2&size=5&q=  test  '");
    println!("  curl -i -X POST http://127.0.0.1:8080/sales/buy \\");
    println!("    -H 'authorization: Bearer test' -H 'content-type: application/json' \\");
    println!(
        "    -d '{{\"sale_ids\":[\"11111111-1111-1111-1111-111111111111\"],\"coupon\":\"  sale10  \",\"use_bonus_points\":true}}'"
    );
    println!("  curl -i -X POST http://127.0.0.1:8080/sales/legacy-create \\");
    println!(
        "    -H 'x-api-key: secret-key' -H 'content-type: application/x-www-form-urlencoded' \\"
    );
    println!("    --data 'title=  Demo+Title  &category=pets'");
    println!();
    println!("Run Caddy on 127.0.0.1:8081 to inspect rewritten query/body/headers.");

    apigate::run(listen, app).await?;
    Ok(())
}
