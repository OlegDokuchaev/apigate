//! Базовый пример: passthrough, rewrite, rewrite-шаблон.
//! Без хуков, валидации и map — только проксирование.

use std::net::SocketAddr;

#[apigate::service(prefix = "/sales")]
mod sales {
    /// Passthrough: проксирует /ping как есть
    #[apigate::get("/ping")]
    async fn ping() {}

    /// Статический rewrite: /public -> /internal
    #[apigate::get("/public", to = "/internal")]
    async fn public_alias() {}

    /// Rewrite-шаблон: /item/{id}/review -> /api/v2/reviews/{id}
    #[apigate::get("/item/{id}/review", to = "/api/v2/reviews/{id}")]
    async fn item_review() {}

    /// Фоллбек
    #[apigate::get("/anything")]
    async fn anything() {}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    let app = apigate::App::builder()
        .backend("sales", ["http://127.0.0.1:8081"])
        .mount(sales::routes())
        .build()?;

    print!(
        "\
basic — http://{listen}

Passthrough:   curl http://{listen}/sales/ping
Rewrite:       curl http://{listen}/sales/public
Rewrite tpl:   curl http://{listen}/sales/item/abc-123/review
Fallback:      curl http://{listen}/sales/anything

Upstream:      caddy run --config apigate/examples/upstream/Caddyfile
"
    );

    apigate::run(listen, app).await?;
    Ok(())
}
