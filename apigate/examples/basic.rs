//! Basic example: passthrough proxying, static rewrite, and rewrite templates.
//! No hooks, validation, or maps are used here.

use std::net::SocketAddr;

#[apigate::service(prefix = "/sales")]
mod sales {
    /// Passthrough: forwards `/ping` as-is after stripping the service prefix.
    #[apigate::get("/ping")]
    async fn ping() {}

    /// Static rewrite: `/public` -> `/internal`.
    #[apigate::get("/public", to = "/internal")]
    async fn public_alias() {}

    /// Rewrite template: `/item/{id}/review` -> `/api/v2/reviews/{id}`.
    #[apigate::get("/item/{id}/review", to = "/api/v2/reviews/{id}")]
    async fn item_review() {}

    /// Plain fallback-style route used by the example curl output.
    #[apigate::get("/anything")]
    async fn anything() {}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    let app = apigate::App::builder()
        .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
        .build()?;

    print!(
        "\
basic - http://{listen}

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
