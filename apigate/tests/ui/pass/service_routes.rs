use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize)]
struct PathParams {
    id: String,
}

#[derive(Clone, Deserialize)]
struct QueryInput {
    q: String,
}

#[derive(Serialize)]
struct QueryOutput {
    q: String,
}

#[apigate::hook]
async fn auth(ctx: &mut apigate::PartsCtx<'_>) -> apigate::HookResult {
    ctx.set_header("x-auth", "ok")?;
    Ok(())
}

#[apigate::hook]
async fn remap_query(
    input: &QueryInput,
    path: &PathParams,
    ctx: &mut apigate::PartsCtx<'_>,
) -> apigate::HookResult {
    ctx.set_query(&QueryOutput {
        q: format!("{}:{}", path.id, input.q),
    })?;
    Ok(())
}

#[apigate::service(name = "sales", prefix = "/api/sales", policy = "default")]
mod sales {
    use super::*;

    #[apigate::get("/{id}", path = PathParams, before = [auth])]
    async fn get_sale() {}

    #[apigate::get("/{id}/search", path = PathParams, query = QueryInput, before = [remap_query])]
    async fn search() {}

    #[apigate::post("/upload", multipart)]
    async fn upload() {}

    #[apigate::put("/static", to = "/internal/static")]
    async fn static_rewrite() {}
}

fn main() {
    let routes = sales::routes();
    assert_eq!(routes.service, "sales");
}
