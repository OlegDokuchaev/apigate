//! Multipart uploads through the proxy.
//! The body is forwarded as passthrough without reading or buffering it.

use std::net::SocketAddr;

#[apigate::hook]
async fn inject_user_headers(ctx: &mut apigate::PartsCtx) -> apigate::HookResult {
    let _token = ctx
        .header("authorization")
        .ok_or_else(|| apigate::ApigateError::unauthorized("missing authorization"))?;
    ctx.set_header("x-user-id", "11111111-1111-1111-1111-111111111111")?;
    Ok(())
}

#[apigate::service(name = "files", prefix = "/files")]
mod files {
    use super::*;

    /// Multipart with auth: validates token and proxies the file body.
    #[apigate::post("/upload", multipart, before = [inject_user_headers])]
    async fn upload() {}

    /// Multipart without hooks: plain passthrough.
    #[apigate::post("/upload-public", multipart)]
    async fn upload_public() {}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    let app = apigate::App::builder()
        .mount_service(files::routes(), ["http://127.0.0.1:8081"])
        .build()?;

    print!("\
multipart - http://{listen}

With auth:   curl -X POST -H 'authorization: Bearer t' -F 'file=@README.md' http://{listen}/files/upload
No auth:     curl -X POST -F 'file=@README.md' http://{listen}/files/upload-public
Missing auth: curl -X POST -F 'file=@README.md' http://{listen}/files/upload

Upstream:    caddy run --config apigate/examples/upstream/Caddyfile
");

    apigate::run(listen, app).await?;
    Ok(())
}
