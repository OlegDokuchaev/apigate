//! Hooks: authentication, shared state, header injection, query rewrite,
//! hook chains, and passing per-request data through `RequestScope`.

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Shared application state. Hooks can request it as `&T`.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AppConfig {
    api_key: String,
}

// ---------------------------------------------------------------------------
// Per-request data passed between hooks through `RequestScope`.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct RequestMeta {
    request_id: String,
}

// ---------------------------------------------------------------------------
// Typed query data available to hooks through `RequestScope`.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct ProductsQuery {
    page: Option<u32>,
    size: Option<u32>,
    q: Option<String>,
}

#[derive(Debug, Serialize)]
struct ProductsQueryService<'a> {
    offset: u32,
    limit: u32,
    query: Option<&'a str>,
}

// ---------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------

/// Checks an API key using shared state (`&AppConfig` from `.state(...)`).
#[apigate::hook]
async fn require_api_key(ctx: &mut apigate::PartsCtx, config: &AppConfig) -> apigate::HookResult {
    let key = ctx
        .header("x-api-key")
        .ok_or_else(|| apigate::ApigateError::forbidden("missing x-api-key"))?;
    if key != config.api_key {
        return Err(apigate::ApigateError::forbidden("invalid x-api-key"));
    }
    Ok(())
}

/// Checks authorization and injects user headers for the upstream request.
#[apigate::hook]
async fn inject_user_headers(ctx: &mut apigate::PartsCtx) -> apigate::HookResult {
    let _token = ctx
        .header("authorization")
        .ok_or_else(|| apigate::ApigateError::unauthorized("missing authorization"))?;
    ctx.set_header("x-user-id", "11111111-1111-1111-1111-111111111111")?;
    ctx.set_header("x-user-role", "demo-user")?;
    Ok(())
}

/// Generates a request id and stores `RequestMeta` for later hooks.
#[apigate::hook]
async fn set_request_id(
    ctx: &mut apigate::PartsCtx,
    scope: &mut apigate::RequestScope,
) -> apigate::HookResult {
    let id = Uuid::new_v4().to_string();
    ctx.set_header("x-request-id", &id)?;
    scope.insert(RequestMeta { request_id: id });
    Ok(())
}

/// Takes `RequestMeta` from scope through an owned parameter.
#[apigate::hook]
async fn log_request_meta(meta: RequestMeta) -> apigate::HookResult {
    println!("[hook] request_id={}", meta.request_id);
    Ok(())
}

/// Rewrites public product query parameters for the upstream service.
#[apigate::hook]
async fn remap_products_query(
    input: &ProductsQuery,
    ctx: &mut apigate::PartsCtx,
) -> apigate::HookResult {
    let page = input.page.unwrap_or(1).max(1);
    let size = input.size.unwrap_or(20).clamp(1, 100);
    let query = input.q.as_deref().map(str::trim).filter(|v| !v.is_empty());

    ctx.set_query(&ProductsQueryService {
        offset: (page - 1) * size,
        limit: size,
        query,
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

#[apigate::service(name = "sales", prefix = "/sales")]
mod sales {
    use super::*;

    /// Shared state in a hook: `require_api_key` receives `&AppConfig`.
    #[apigate::get("/admin/stats", before = [require_api_key])]
    async fn admin_stats() {}

    /// Authorization hook: validates token and injects upstream headers.
    #[apigate::get("/user", before = [inject_user_headers])]
    async fn user_profile() {}

    /// Four-hook chain with per-request data passed through scope:
    /// require_api_key -> inject_user_headers -> set_request_id -> log_request_meta.
    #[apigate::get(
        "/secure-user",
        before = [require_api_key, inject_user_headers, set_request_id, log_request_meta]
    )]
    async fn secure_user_profile() {}

    /// Typed query data in a hook: validates the public query, then rewrites it.
    #[apigate::get("/products", query = ProductsQuery, before = [remap_products_query])]
    async fn get_products() {}
}

// ---------------------------------------------------------------------------
// Entrypoint
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    let app = apigate::App::builder()
        .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
        .state(AppConfig {
            api_key: "secret-key".to_string(),
        })
        .build()?;

    print!("\
hooks - http://{listen}

Shared state:    curl -H 'x-api-key: secret-key' http://{listen}/sales/admin/stats
Wrong key:       curl -H 'x-api-key: wrong' http://{listen}/sales/admin/stats
Auth:            curl -H 'authorization: Bearer t' http://{listen}/sales/user
No token:        curl http://{listen}/sales/user
Hook chain:      curl -H 'x-api-key: secret-key' -H 'authorization: Bearer t' http://{listen}/sales/secure-user
Query rewrite:   curl 'http://{listen}/sales/products?page=2&size=5&q=test'

Upstream:        caddy run --config apigate/examples/upstream/Caddyfile
");

    apigate::run(listen, app).await?;
    Ok(())
}
