use std::net::SocketAddr;

#[apigate::hook]
async fn require_api_key(ctx: &mut apigate::PartsCtx<'_>) -> apigate::HookResult {
    let api_key = ctx
        .header("x-api-key")
        .ok_or_else(|| apigate::HookError::forbidden("missing x-api-key"))?;

    if api_key != "secret-key" {
        return Err(apigate::HookError::forbidden("invalid x-api-key"));
    }

    Ok(())
}

#[apigate::hook]
async fn inject_user_headers(ctx: &mut apigate::PartsCtx<'_>) -> apigate::HookResult {
    let _auth = ctx
        .header("authorization")
        .ok_or_else(|| apigate::HookError::unauthorized("missing authorization"))?;

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
    #[apigate::get(
        "/admin/stats",
        before = [require_api_key]
    )]
    async fn admin_stats() {}

    // Хук добавляет заголовки, которые уйдут в upstream
    #[apigate::get(
        "/user",
        before = [inject_user_headers]
    )]
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
        before = [mark_demo_request]
    )]
    async fn get_by_id() {}

    // 4) Фоллбек для всего остального внутри /sales/*
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
    println!("  curl -i -H 'x-api-key: secret-key' -H 'authorization: Bearer test' http://127.0.0.1:8080/sales/secure-user");
    println!();
    println!("To inspect forwarded headers удобно поднять echo upstream на 127.0.0.1:8081.");

    apigate::run(listen, app).await?;
    Ok(())
}
