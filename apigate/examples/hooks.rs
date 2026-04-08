//! Хуки: аутентификация, shared state, инъекция заголовков,
//! цепочка хуков, передача данных между хуками через scope.

use std::net::SocketAddr;

use uuid::Uuid;

// ---------------------------------------------------------------------------
// Общий state приложения (доступен как &T в хуках)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AppConfig {
    api_key: String,
}

// ---------------------------------------------------------------------------
// Per-request данные (передаются между хуками через scope)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct RequestMeta {
    request_id: String,
}

// ---------------------------------------------------------------------------
// Хуки
// ---------------------------------------------------------------------------

/// Проверяет api key через shared state (&AppConfig из .state())
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

/// Проверяет токен авторизации, ставит заголовки для upstream
#[apigate::hook]
async fn inject_user_headers(ctx: &mut apigate::PartsCtx) -> apigate::HookResult {
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
    ctx: &mut apigate::PartsCtx,
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

// ---------------------------------------------------------------------------
// Сервис
// ---------------------------------------------------------------------------

#[apigate::service(name = "sales", prefix = "/sales")]
mod sales {
    use super::*;

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
hooks — http://{listen}

Shared state:    curl -H 'x-api-key: secret-key' http://{listen}/sales/admin/stats
Wrong key:       curl -H 'x-api-key: wrong' http://{listen}/sales/admin/stats
Auth:            curl -H 'authorization: Bearer t' http://{listen}/sales/user
No token:        curl http://{listen}/sales/user
Hook chain:      curl -H 'x-api-key: secret-key' -H 'authorization: Bearer t' http://{listen}/sales/secure-user

Upstream:        caddy run --config apigate/examples/upstream/Caddyfile
");

    apigate::run(listen, app).await?;
    Ok(())
}
