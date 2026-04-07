//! Multipart: загрузка файлов через proxy.
//! Body проксируется как есть (passthrough), без чтения и буферизации.

use std::net::SocketAddr;

#[apigate::hook]
async fn inject_user_headers(ctx: &mut apigate::PartsCtx<'_>) -> apigate::HookResult {
    let _token = ctx
        .header("authorization")
        .ok_or_else(|| apigate::ApigateError::unauthorized("missing authorization"))?;
    ctx.set_header("x-user-id", "11111111-1111-1111-1111-111111111111")?;
    Ok(())
}

#[apigate::service(name = "files", prefix = "/files")]
mod files {
    use super::*;

    /// Multipart с аутентификацией: проверяет токен, проксирует файл
    #[apigate::post("/upload", multipart, before = [inject_user_headers])]
    async fn upload() {}

    /// Multipart без хуков: простой passthrough
    #[apigate::post("/upload-public", multipart)]
    async fn upload_public() {}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    let app = apigate::App::builder()
        .backend("files", ["http://127.0.0.1:8081"])
        .mount(files::routes())
        .build()
        .map_err(anyhow::Error::msg)?;

    print!("\
multipart — http://{listen}

С auth:      curl -X POST -H 'authorization: Bearer t' -F 'file=@README.md' http://{listen}/files/upload
Без auth:    curl -X POST -F 'file=@README.md' http://{listen}/files/upload-public
Нет токена:  curl -X POST -F 'file=@README.md' http://{listen}/files/upload

Upstream:    caddy run --config apigate/examples/upstream/Caddyfile
");

    apigate::run(listen, app).await?;
    Ok(())
}
