//! Ошибки: глобальный JSON renderer, разделение user/debug сообщений
//! и кастомный JSON-ответ из hook.

use std::net::SocketAddr;

use apigate::{ApigateCoreError, ApigateFrameworkError, ApigatePipelineError};
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct ErrBody {
    code: &'static str,
    message: String,
}

#[derive(Debug, Deserialize)]
struct BuyInput {
    sale_id: String,
}

#[derive(Debug, Serialize)]
struct BuyInputUpstream {
    id: String,
    source: &'static str,
}

fn render_error(err: ApigateFrameworkError) -> Response {
    match &err {
        // Пример точечного override:
        // 1) получаем конкретный enum-вариант
        // 2) логируем внутренние детали
        // 3) возвращаем полностью другой HTTP-результат
        ApigateFrameworkError::Pipeline(ApigatePipelineError::InvalidJsonBody(details)) => {
            eprintln!("[apigate][invalid_json_body] details={details}");
            let body = serde_json::json!({
                "error": {
                    "code": "invalid_json_payload",
                    "message": "invalid json payload",
                }
            });
            return (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(body)).into_response();
        }
        ApigateFrameworkError::Core(ApigateCoreError::UpstreamRequestTimedOut) => {
            eprintln!("[apigate][upstream_timeout] code={}", err.code());
            let body = serde_json::json!({
                "error": {
                    "code": "upstream_timeout",
                    "message": "upstream timeout, please retry",
                }
            });
            return (StatusCode::GATEWAY_TIMEOUT, axum::Json(body)).into_response();
        }
        _ => {
            if let Some(details) = err.debug_details() {
                eprintln!("[apigate][debug] code={} details={details}", err.code());
            }
        }
    }

    let body = serde_json::json!({
        "error": {
            "code": err.code(),
            "message": err.user_message(),
        }
    });
    (err.status_code(), axum::Json(body)).into_response()
}

#[apigate::hook]
async fn require_auth(ctx: &mut apigate::PartsCtx) -> apigate::HookResult {
    if ctx.header("authorization").is_none() {
        return Err(apigate::ApigateError::unauthorized_json(&ErrBody {
            code: "auth_missing_token",
            message: "missing authorization header".to_string(),
        }));
    }
    Ok(())
}

#[apigate::map]
async fn remap_buy(input: BuyInput) -> apigate::MapResult<BuyInputUpstream> {
    Ok(BuyInputUpstream {
        id: input.sale_id,
        source: "apigate-errors-example",
    })
}

#[apigate::service(name = "sales", prefix = "/sales")]
mod sales {
    use super::*;

    #[apigate::post("/buy", json = BuyInput, before = [require_auth], map = remap_buy)]
    async fn buy() {}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    let app = apigate::App::builder()
        .backend("sales", ["http://127.0.0.1:8081"])
        .error_renderer(render_error)
        .mount(sales::routes())
        .build()?;

    print!(
        "\
errors — http://{listen}

Hook custom JSON error (no auth):
  curl -X POST -H 'content-type: application/json' \
    -d '{{\"sale_id\":\"111\"}}' http://{listen}/sales/buy

Framework parse error (invalid json):
  curl -X POST -H 'authorization: Bearer t' -H 'content-type: application/json' \
    -d '{{\"sale_id\":' http://{listen}/sales/buy

Success:
  curl -X POST -H 'authorization: Bearer t' -H 'content-type: application/json' \
    -d '{{\"sale_id\":\"111\"}}' http://{listen}/sales/buy

Upstream: caddy run --config apigate/examples/upstream/Caddyfile
"
    );

    apigate::run(listen, app).await?;
    Ok(())
}
