//! Maps: transform JSON and form data before forwarding upstream.

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Shared application state. Maps can request it as `&T`.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AppConfig {
    api_key: String,
}

// ---------------------------------------------------------------------------
// JSON map types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct PublicBuyInput {
    sale_ids: Vec<Uuid>,
    coupon: Option<String>,
    use_bonus_points: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ServiceBuyInput {
    sale_ids: Vec<Uuid>,
    promo_code: Option<String>,
    payment_mode: &'static str,
    source: &'static str,
}

// ---------------------------------------------------------------------------
// Form map types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct LegacyFormPublic {
    title: String,
    category: String,
}

#[derive(Debug, Serialize)]
struct LegacyFormService<'a> {
    title: &'a str,
    category_code: &'static str,
}

// ---------------------------------------------------------------------------
// Raw body map types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct WebhookEvent {
    id: String,
    kind: String,
}

#[derive(Debug, Serialize)]
struct UpstreamEvent {
    event_id: String,
    event_kind: String,
    verified: bool,
}

/// Dependency-free stand-in for an HMAC: FNV-1a over the raw bytes.
fn signature(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(2166136261u32, |h, &b| {
        (h ^ u32::from(b)).wrapping_mul(16777619)
    });
    format!("{hash:08x}")
}

// ---------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------

/// Authorization hook used by `/buy`.
#[apigate::hook]
async fn inject_user_headers(ctx: &mut apigate::PartsCtx) -> apigate::HookResult {
    let _token = ctx
        .header("authorization")
        .ok_or_else(|| apigate::ApigateError::unauthorized("missing authorization"))?;
    ctx.set_header("x-user-id", "11111111-1111-1111-1111-111111111111")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Map functions
// ---------------------------------------------------------------------------

/// JSON transformation with shared state access (`&AppConfig`).
#[apigate::map]
async fn remap_buy_json(
    input: PublicBuyInput,
    config: &AppConfig,
) -> apigate::MapResult<ServiceBuyInput> {
    Ok(ServiceBuyInput {
        sale_ids: input.sale_ids,
        promo_code: input
            .coupon
            .map(|v| v.trim().to_uppercase())
            .filter(|v| !v.is_empty()),
        payment_mode: if input.use_bonus_points.unwrap_or(false) {
            "bonus"
        } else {
            "money"
        },
        source: if config.api_key.is_empty() {
            "unknown"
        } else {
            "apigate-demo"
        },
    })
}

/// Form transformation: category -> category_code.
#[apigate::map]
async fn remap_legacy_form(input: LegacyFormPublic) -> apigate::MapResult<LegacyFormService<'_>> {
    Ok(LegacyFormService {
        title: input.title.trim(),
        category_code: match input.category.trim().to_lowercase().as_str() {
            "pets" => "P",
            "items" => "I",
            _ => "U",
        },
    })
}

/// Typed `json` input **plus** the exact raw bytes.
#[apigate::map]
async fn verify_and_remap(
    input: WebhookEvent,
    raw: apigate::RawBody,
    ctx: &mut apigate::PartsCtx,
) -> apigate::MapResult<UpstreamEvent> {
    let provided = ctx.header("x-signature").unwrap_or_default();
    if provided != signature(raw.as_bytes()) {
        return Err(apigate::ApigateError::unauthorized("invalid signature"));
    }

    Ok(UpstreamEvent {
        event_id: input.id,
        event_kind: input.kind,
        verified: true,
    })
}

/// No `json`/`form`: `RawBody` is the map input and the output is any `impl Into<Body>`.
#[apigate::map]
async fn forward_raw(
    raw: apigate::RawBody,
    ctx: &mut apigate::PartsCtx,
) -> apigate::MapResult<apigate::RawBody> {
    ctx.set_header("x-raw-len", raw.len().to_string())?;
    Ok(raw)
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

#[apigate::service(name = "sales", prefix = "/sales")]
mod sales {
    use super::*;

    /// JSON body transformed through a map function with shared state access.
    #[apigate::post("/buy", json = PublicBuyInput, before = [inject_user_headers], map = remap_buy_json)]
    async fn buy() {}

    /// Form body transformed through a map function.
    #[apigate::post("/legacy-create", form = LegacyFormPublic, map = remap_legacy_form)]
    async fn legacy_create() {}

    /// Typed body + raw-byte signature verification.
    #[apigate::post("/events", json = WebhookEvent, map = verify_and_remap)]
    async fn events() {}

    /// No schema: inspect and forward the exact body.
    #[apigate::post("/raw", map = forward_raw)]
    async fn raw() {}
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

    let webhook = r#"{"id":"evt_1","kind":"order.created"}"#;
    let sig = signature(webhook.as_bytes());

    print!(
        "\
map - http://{listen}

Json map:        curl -X POST -H 'authorization: Bearer t' -H 'content-type: application/json' \
                   -d '{{\"sale_ids\":[\"11111111-1111-1111-1111-111111111111\"],\"coupon\":\"sale10\"}}' http://{listen}/sales/buy
Form map:        curl -X POST -H 'content-type: application/x-www-form-urlencoded' \
                   -d 'title=Demo&category=pets' http://{listen}/sales/legacy-create
Raw + json map:  curl -X POST -H 'content-type: application/json' -H 'x-signature: {sig}' \
                   -d '{webhook}' http://{listen}/sales/events
Bad signature:   curl -X POST -H 'content-type: application/json' -H 'x-signature: deadbeef' \
                   -d '{webhook}' http://{listen}/sales/events
Raw map:         curl -X POST --data-binary 'any-raw-bytes-here' http://{listen}/sales/raw

Upstream:        caddy run --config apigate/examples/upstream/Caddyfile
"
    );

    apigate::run(listen, app).await?;
    Ok(())
}
