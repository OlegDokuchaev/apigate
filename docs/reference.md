# ApiGate Reference

Detailed API reference for ApiGate. Start with the [README](../README.md) for installation, quick start, core concepts, benchmarks, and examples.

## Services

A service is an inline Rust module annotated with `#[apigate::service]`.

```rust
#[apigate::service(name = "sales", prefix = "/sales", policy = "sales_default")]
mod sales {
    use super::*;

    #[apigate::get("/ping")]
    async fn ping() {}

    #[apigate::post("/buy", json = BuyInput, before = [auth], map = remap_buy)]
    async fn buy() {}
}
```

Service arguments:

| Argument | Description |
|---|---|
| `name = "sales"` | Logical service name. Defaults to the module name. This name is used for backend registration. |
| `prefix = "/sales"` | Public URL prefix for all routes in the service. Defaults to `""`. |
| `policy = "name"` | Default named policy for all routes in the service. |

The service macro injects a `routes()` function. Mount it with backends:

```rust
let app = apigate::App::builder()
    .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
    .build()?;
```

If you want to register backends separately, use `.backend(...).mount(...)`:

```rust
let app = apigate::App::builder()
    .backend("sales", ["http://127.0.0.1:8081"])
    .mount(sales::routes())
    .build()?;
```

## Routes

Supported method attributes:

```rust
#[apigate::get(...)]
#[apigate::post(...)]
#[apigate::put(...)]
#[apigate::delete(...)]
#[apigate::patch(...)]
#[apigate::head(...)]
#[apigate::options(...)]
```

Full route shape:

```rust
#[apigate::post(
    "/path/{id}",
    to = "/upstream/{id}",
    path = PathParams,
    query = QueryInput,
    before = [auth, inject_headers],
    map = remap,
    policy = "sticky_by_id",
)]
async fn route_name() {}
```

Route arguments:

| Argument | Description |
|---|---|
| `"/path"` | Public route path relative to the service prefix. Supports `{param}` segments. |
| `to = "/path"` | Upstream path rewrite. Without `to`, ApiGate strips the service prefix and forwards the remaining path. Supports `{param}` template captures. |
| `path = T` | Deserializes typed path parameters with axum. `T` should be `Deserialize + Clone + Send + Sync + 'static`. |
| `query = T` | Validates query string as `T`. With `map`, serializes mapped output back into the query string. |
| `json = T` | Validates JSON body as `T`. With `map`, serializes mapped output as a new JSON body. |
| `form = T` | Validates `application/x-www-form-urlencoded` data as `T`. With `map`, serializes mapped output back as form data or query data for GET/HEAD. |
| `multipart` | Enables multipart passthrough. The body is not read or buffered. |
| `before = [...]` | Hooks executed before proxying. They run in the listed order. |
| `map = fn_name` | Typed request transformation for `query`, `json`, or `form`. Not supported with `multipart`. |
| `policy = "name"` | Route-level policy override. |

Only one body/data mode can be used per route: `query`, `json`, `form`, or `multipart`.

### Path Rewrites

No `to` means strip the service prefix:

```rust
#[apigate::service(prefix = "/sales")]
mod sales {
    #[apigate::get("/ping")]
    async fn ping() {}
}
```

`GET /sales/ping` is forwarded to `/ping` on the upstream.

Static rewrite:

```rust
#[apigate::get("/public", to = "/internal")]
async fn public_alias() {}
```

Template rewrite:

```rust
#[apigate::get("/item/{id}/review", to = "/api/v2/reviews/{id}")]
async fn item_review() {}
```

## Typed Inputs

### Path Parameters

```rust
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
struct SalePath {
    id: Uuid,
}

#[apigate::service(prefix = "/sales")]
mod sales {
    use super::*;

    #[apigate::get("/{id}", path = SalePath)]
    async fn get_sale() {}
}
```

Path values are extracted before hooks and inserted into `RequestScope`. Hooks and maps can request `&SalePath` or owned `SalePath` as parameters.

### Query, JSON, and Form

```rust
#[apigate::get("/search", query = SearchQuery)]
async fn search() {}

#[apigate::post("/buy", json = BuyInput)]
async fn buy() {}

#[apigate::post("/legacy", form = LegacyForm)]
async fn legacy() {}
```

Without `map`, ApiGate validates the input and forwards the original body/query data. For `json` and `form` bodies, validation requires reading the body up to `map_body_limit`.

With `map`, ApiGate validates the input, calls your mapper, and forwards the mapped output.

### Multipart

```rust
#[apigate::post("/upload", multipart, before = [auth])]
async fn upload() {}
```

Multipart bodies are proxied as streaming passthrough. ApiGate does not read or buffer the file body. `map` is intentionally not supported for multipart routes.

## Hooks

Hooks run before the upstream request is sent. Use them for authentication, authorization, request IDs, header injection, request mutation, and per-request metadata.

```rust
#[apigate::hook]
async fn auth(ctx: &mut apigate::PartsCtx) -> apigate::HookResult {
    let has_token = ctx
        .header("authorization")
        .map(|token| !token.is_empty())
        .ok_or_else(|| apigate::ApigateError::unauthorized("missing authorization"))?;

    ctx.set_header("x-user-id", "11111111-1111-1111-1111-111111111111")?;
    ctx.set_header("x-auth-token-seen", has_token.to_string())?;
    Ok(())
}

#[apigate::get("/protected", before = [auth])]
async fn protected() {}
```

`PartsCtx` exposes the request head:

| Method | Purpose |
|---|---|
| `service()` | Current logical service name. |
| `route_path()` | Route path relative to the service prefix. |
| `method()` | Current HTTP method. |
| `uri()` / `uri_mut()` | Read or mutate the request URI. |
| `headers()` / `headers_mut()` | Read or mutate headers. |
| `header(name)` | Read a UTF-8 header as `Option<&str>`. |
| `set_header(name, value)` | Insert or replace a header. |
| `set_header_if_absent(name, value)` | Insert a header only when absent. |
| `remove_header(name)` | Remove a header. |
| `extensions()` / `extensions_mut()` | Access request extensions. |

## Maps

Maps transform typed `query`, `json`, or `form` inputs before proxying.

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct PublicBuy {
    sale_ids: Vec<String>,
    coupon: Option<String>,
}

#[derive(Debug, Serialize)]
struct UpstreamBuy {
    ids: Vec<String>,
    promo_code: Option<String>,
    source: &'static str,
}

#[apigate::map]
async fn remap_buy(input: PublicBuy) -> apigate::MapResult<UpstreamBuy> {
    Ok(UpstreamBuy {
        ids: input.sale_ids,
        promo_code: input
            .coupon
            .map(|v| v.trim().to_uppercase())
            .filter(|v| !v.is_empty()),
        source: "gateway",
    })
}

#[apigate::post("/buy", json = PublicBuy, map = remap_buy)]
async fn buy() {}
```

Mapping behavior:

| Route data | Map output handling |
|---|---|
| `query = T` | Serialized with `serde_urlencoded` and written into the URI query string. |
| `json = T` | Serialized with `serde_json` and sent as a new JSON body. |
| `form = T` | Serialized with `serde_urlencoded`; sent as a form body for non-GET/HEAD and as query string for GET/HEAD. |

## Hook and Map Parameters

`#[apigate::hook]` and `#[apigate::map]` rewrite your function signature into an efficient internal form. You declare only the values you need.

| Parameter | Source | Example |
|---|---|---|
| `&mut PartsCtx` | Request head context. | `ctx: &mut apigate::PartsCtx` |
| `&mut RequestScope` | Direct access to per-request scope. | `scope: &mut apigate::RequestScope` |
| `&T` | Local per-request value first, then shared app state. | `config: &AuthConfig` |
| `&mut T` | Local per-request value only. | `counter: &mut RequestCounter` |
| `T` in a hook | `scope.take::<T>()`; falls back to cloning shared state. | `path: SalePath` |
| First owned `T` in a map | Typed input from `query`, `json`, or `form`. | `input: PublicBuy` |
| Additional owned `T` in a map | `scope.take::<T>()`; falls back to cloning shared state. | `path: SalePath` |

Rules enforced by the macros:

- At most one `&mut PartsCtx` parameter.
- At most one `&mut RequestScope` parameter.
- At most one extracted `&mut T` parameter.
- `&mut RequestScope` cannot be combined with extracted `&T` or `&mut T` parameters.
- Extracted `&mut T` cannot be combined with extracted `&T` parameters.
- Hook and map functions must be `async`.

Example using shared state and per-request path data:

```rust
#[derive(Clone)]
struct AuthConfig {
    api_key: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SalePath {
    id: uuid::Uuid,
}

#[apigate::hook]
async fn require_key(ctx: &mut apigate::PartsCtx, config: &AuthConfig) -> apigate::HookResult {
    if ctx.header("x-api-key") != Some(config.api_key.as_str()) {
        return Err(apigate::ApigateError::forbidden("invalid api key"));
    }
    Ok(())
}

#[apigate::hook]
async fn add_sale_header(path: &SalePath, ctx: &mut apigate::PartsCtx) -> apigate::HookResult {
    ctx.set_header("x-sale-id", path.id.to_string())?;
    Ok(())
}

#[apigate::service(prefix = "/sales")]
mod sales {
    use super::*;

    #[apigate::get("/{id}", path = SalePath, before = [require_key, add_sale_header])]
    async fn get_sale() {}
}
```

## Shared and Per-Request State

Register app state with `.state(...)`:

```rust
let app = apigate::App::builder()
    .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
    .state(AuthConfig {
        api_key: "secret-key".to_string(),
    })
    .build()?;
```

State is stored in `Extensions` and exposed to hooks/maps by reference. Read-only access through `&T` does not clone state per request.

For per-request data, insert into `RequestScope` from a hook:

```rust
#[derive(Clone)]
struct RequestMeta {
    request_id: String,
}

#[apigate::hook]
async fn set_request_id(
    ctx: &mut apigate::PartsCtx,
    scope: &mut apigate::RequestScope<'_>,
) -> apigate::HookResult {
    let request_id = uuid::Uuid::new_v4().to_string();
    ctx.set_header("x-request-id", &request_id)?;
    scope.insert(RequestMeta { request_id });
    Ok(())
}

#[apigate::hook]
async fn log_request(meta: RequestMeta) -> apigate::HookResult {
    println!("request_id={}", meta.request_id);
    Ok(())
}
```

`RequestScope` methods:

| Method | Purpose |
|---|---|
| `get::<T>()` | Read local value first, then shared app state. |
| `get_mut::<T>()` | Mutably read local per-request value only. |
| `insert(value)` | Insert local per-request value. |
| `take::<T>()` | Remove local value, or clone from shared app state if absent. |
| `take_body()` | Take request body ownership. Used by generated pipelines. |
| `body_limit()` | Current generated pipeline body limit. |

## Error Handling

ApiGate separates two use cases:

| Use case | API |
|---|---|
| Framework-rendered errors | Return `ApigateError::bad_request(...)`, `unauthorized(...)`, `forbidden(...)`, etc. These go through the global error renderer. |
| Fully custom responses | Return `ApigateError::from_response(...)` or `ApigateError::json(...)`. These bypass the global renderer. |

### Default Behavior

By default, framework errors are returned as `text/plain` with the error status code and a user-facing message.

Build-time configuration errors are returned from `.build()` as `ApigateBuildError`.

Runtime framework errors are normalized as `ApigateFrameworkError` before rendering:

```rust
pub enum ApigateFrameworkError {
    Core(ApigateCoreError),
    Pipeline(ApigatePipelineError),
    Http { status: StatusCode, message: Cow<'static, str> },
}
```

Useful methods:

| Method | Purpose |
|---|---|
| `status_code()` | Default HTTP status for the error. |
| `code()` | Stable machine-readable code. |
| `user_message()` | Message safe to return to clients. |
| `debug_details()` | Internal diagnostic details intended for logs. |

### Global JSON Error Renderer

Use `.error_renderer(...)` when you want one JSON format for framework errors:

```rust
use apigate::{ApigateCoreError, ApigateFrameworkError, ApigatePipelineError};
use axum::response::{IntoResponse, Response};
use http::StatusCode;

fn render_error(err: ApigateFrameworkError) -> Response {
    match &err {
        ApigateFrameworkError::Pipeline(ApigatePipelineError::InvalidJsonBody(details)) => {
            tracing::warn!(details, "invalid json body");
            let body = serde_json::json!({
                "error": {
                    "code": "invalid_json_payload",
                    "message": "invalid json payload"
                }
            });
            return (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(body)).into_response();
        }
        ApigateFrameworkError::Core(ApigateCoreError::UpstreamRequestTimedOut) => {
            tracing::warn!(code = err.code(), "upstream timeout");
            let body = serde_json::json!({
                "error": {
                    "code": "upstream_timeout",
                    "message": "upstream timeout, please retry"
                }
            });
            return (StatusCode::GATEWAY_TIMEOUT, axum::Json(body)).into_response();
        }
        _ => {
            if let Some(details) = err.debug_details() {
                tracing::debug!(code = err.code(), details, "apigate framework error");
            }
        }
    }

    let body = serde_json::json!({
        "error": {
            "code": err.code(),
            "message": err.user_message()
        }
    });
    (err.status_code(), axum::Json(body)).into_response()
}

let app = apigate::App::builder()
    .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
    .error_renderer(render_error)
    .build()?;
```

### Custom Hook and Map Errors

Return framework-rendered errors:

```rust
#[apigate::hook]
async fn require_auth(ctx: &mut apigate::PartsCtx) -> apigate::HookResult {
    if ctx.header("authorization").is_none() {
        return Err(apigate::ApigateError::unauthorized("missing authorization header"));
    }
    Ok(())
}
```

Return a custom JSON response that bypasses the global renderer:

```rust
#[derive(serde::Serialize)]
struct ErrBody {
    code: &'static str,
    message: String,
}

#[apigate::hook]
async fn require_auth(ctx: &mut apigate::PartsCtx) -> apigate::HookResult {
    if ctx.header("authorization").is_none() {
        return Err(apigate::ApigateError::json(
            http::StatusCode::UNAUTHORIZED,
            ErrBody {
                code: "auth_missing",
                message: "missing token".to_string(),
            },
        ));
    }
    Ok(())
}
```

Convenience JSON constructors:

```rust
apigate::ApigateError::bad_request_json(body)
apigate::ApigateError::unauthorized_json(body)
apigate::ApigateError::forbidden_json(body)
```

Other common framework constructors:

```rust
apigate::ApigateError::new(status, message)
apigate::ApigateError::bad_request(message)
apigate::ApigateError::unauthorized(message)
apigate::ApigateError::forbidden(message)
apigate::ApigateError::payload_too_large(message)
apigate::ApigateError::unsupported_media_type(message)
apigate::ApigateError::bad_gateway(message)
apigate::ApigateError::gateway_timeout(message)
apigate::ApigateError::internal(message)
```

Full example:

```sh
cargo run --example errors
```

## Runtime Observability and Tracing

ApiGate does not install a global tracing subscriber. Your application owns tracing configuration.

By default, ApiGate runtime observer is disabled. This keeps the hot path low-overhead: request handling only checks whether an observer exists.

Enable the built-in tracing observer:

```rust
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,apigate=debug"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .compact()
        .init();
}

let app = apigate::App::builder()
    .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
    .enable_default_tracing()
    .build()?;
```

Or provide a custom runtime observer:

```rust
fn observe(event: apigate::RuntimeEvent<'_>) {
    apigate::default_tracing_observer(event);

    if let apigate::RuntimeEventKind::UpstreamSucceeded {
        backend_index,
        status,
        upstream_latency,
    } = event.kind
    {
        tracing::info!(
            target: "app::audit",
            service = event.service,
            route = event.route_path,
            backend_index,
            status = status.as_u16(),
            latency = ?upstream_latency,
            "gateway request completed"
        );
    }
}

let app = apigate::App::builder()
    .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
    .runtime_observer(observe)
    .build()?;
```

Runtime event kinds include request start, backend selection, pipeline failure, dispatch failure, upstream success, and upstream failure. Success-oriented events are debug-level in the default observer. Expected client errors are logged as `info`, and server/upstream failures as `warn`.

Disable observer explicitly after conditional configuration:

```rust
let app = apigate::App::builder()
    .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
    .disable_runtime_observer()
    .build()?;
```

### External Tower Layers

Use `with_router` to add outer `tower`/`axum` middleware after building the app:

```rust
use tower_http::trace::TraceLayer;

let app = apigate::App::builder()
    .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
    .build()?
    .with_router(|router| router.layer(TraceLayer::new_for_http()));

apigate::run(listen, app).await?;
```

For full manual composition, take the underlying router:

```rust
let router = apigate::App::builder()
    .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
    .build()?
    .into_router();

let router = router.layer(TraceLayer::new_for_http());
apigate::run_router(listen, router).await?;
```

Useful examples:

```sh
RUST_LOG=debug,apigate=trace cargo run --example logging
RUST_LOG=debug,apigate=debug,tower_http=debug cargo run --example tower_logging
```

`hyper-util` also emits internal logs for transport, connection pooling, and connecting. Enable them only for diagnostics:

```sh
RUST_LOG=info,apigate=debug,hyper_util::client::legacy=debug cargo run --example logging
```

## Policies, Routing, and Balancing

A policy combines:

| Component | Purpose |
|---|---|
| Routing strategy | Selects candidate backends and optionally produces an affinity key. |
| Balancer | Picks the final backend from the candidate set. |

Default policy: `NoRouteKey + RoundRobin`.

Register named policies:

```rust
let app = apigate::App::builder()
    .mount_service(sales::routes(), [
        "http://127.0.0.1:8081",
        "http://127.0.0.1:8082",
    ])
    .policy("sticky_user", apigate::Policy::header_sticky("x-user-id"))
    .policy("sticky_id", apigate::Policy::path_sticky("id"))
    .policy("least_req", apigate::Policy::least_request())
    .policy("least_time", apigate::Policy::least_time())
    .build()?;
```

Use a service-level policy:

```rust
#[apigate::service(prefix = "/sales", policy = "sticky_user")]
mod sales {
    #[apigate::get("/user")]
    async fn user() {}
}
```

Override per route:

```rust
#[apigate::get("/{id}", policy = "sticky_id")]
async fn by_id() {}
```

Policy priority:

1. Route-level `policy = "name"`.
2. Service-level `policy = "name"`.
3. Builder default policy.

Built-in policy presets:

| Preset | Meaning |
|---|---|
| `Policy::round_robin()` | `NoRouteKey + RoundRobin`. |
| `Policy::consistent_hash()` | `NoRouteKey + ConsistentHash`; falls back to round-robin when no affinity key exists. |
| `Policy::header_sticky("x-user-id")` | `HeaderSticky + ConsistentHash`. |
| `Policy::path_sticky("id")` | `PathSticky + ConsistentHash`. |
| `Policy::least_request()` | `NoRouteKey + LeastRequest`. |
| `Policy::least_time()` | `NoRouteKey + LeastTime`. |

You can also build custom combinations:

```rust
let policy = apigate::Policy::new()
    .router(apigate::routing::HeaderSticky::new("x-tenant-id"))
    .balancer(apigate::balancing::ConsistentHash::new());
```

### Custom Routing Strategy

```rust
use apigate::routing::{AffinityKey, CandidateSet, RouteCtx, RouteStrategy, RoutingDecision};

struct CookieSticky(&'static str);

impl RouteStrategy for CookieSticky {
    fn route<'a>(
        &self,
        ctx: &RouteCtx<'a>,
        _pool: &'a apigate::BackendPool,
    ) -> RoutingDecision<'a> {
        let affinity = ctx
            .headers
            .get("cookie")
            .and_then(|v| v.to_str().ok())
            .and_then(|cookies| {
                cookies
                    .split(';')
                    .map(str::trim)
                    .find_map(|cookie| cookie.strip_prefix(self.0)?.strip_prefix('='))
            })
            .map(AffinityKey::borrowed);

        RoutingDecision {
            affinity,
            candidates: CandidateSet::All,
        }
    }
}
```

`RouteCtx` includes service, prefix, route path, method, URI, and headers. `RoutingDecision` returns an optional affinity key and either all backend candidates or explicit backend indices.

### Custom Balancer

```rust
use apigate::balancing::{BalanceCtx, Balancer, ResultEvent, StartEvent};

struct FirstCandidate;

impl Balancer for FirstCandidate {
    fn pick(&self, ctx: &BalanceCtx<'_>) -> Option<usize> {
        ctx.candidate_index(0)
    }

    fn on_start(&self, _event: &StartEvent<'_>) {}

    fn on_result(&self, _event: &ResultEvent<'_>) {}
}
```

`BalanceCtx` gives access to service, affinity, backend pool, candidate count, candidate indices, candidate backends, and candidate membership checks.

Built-in balancers use atomics and avoid locks on the request path.

## App Builder Reference

Common builder methods:

| Method | Description |
|---|---|
| `.mount_service(routes, urls)` | Register backend URLs for `routes.service` and mount the routes. |
| `.backend(service, urls)` | Register backend URLs by service name. |
| `.mount(routes)` | Mount macro-generated routes. Requires matching `.backend(...)`. |
| `.policy(name, policy)` | Register a named policy. |
| `.default_policy(policy)` | Set fallback policy for routes without route/service policy. |
| `.state(value)` | Insert shared application state available to hooks and maps. |
| `.request_timeout(duration)` | Total timeout for an upstream request. Default: 30s. |
| `.connect_timeout(duration)` | TCP connect timeout for upstream connections. Default: 5s. |
| `.pool_idle_timeout(duration)` | Idle connection lifetime in the upstream client pool. Default: 90s. |
| `.pool_max_idle_per_host(max)` | Maximum idle upstream connections per host. Default: unlimited. |
| `.upstream(config)` | Replace the upstream transport configuration. |
| `.map_body_limit(bytes)` | Max body size read by generated validation/map pipelines. Default: 2 MiB. |
| `.error_renderer(renderer)` | Configure framework error rendering. |
| `.enable_default_tracing()` | Emit built-in runtime events through `tracing`. |
| `.runtime_observer(observer)` | Configure a custom runtime observer. |
| `.disable_runtime_observer()` | Disable runtime observer events. |
| `.build()` | Build the gateway app. |

`UpstreamConfig` methods:

| Method | Description |
|---|---|
| `.connect_timeout(duration)` | TCP connect timeout for upstream connections. |
| `.pool_idle_timeout(duration)` | Idle connection lifetime in the upstream client pool. |
| `.pool_max_idle_per_host(max)` | Maximum idle upstream connections per host. |
| `.tcp_nodelay(bool)` | Toggle `TCP_NODELAY` for upstream TCP connections. |
| `.configure_client(|client| ...)` | Customize hyper-util's legacy client builder. |
| `.configure_connector(|connector| ...)` | Customize hyper-util's `HttpConnector`. |

For reusable or detailed transport settings, build a config value from defaults:

```rust
let upstream = apigate::UpstreamConfig::default()
    .connect_timeout(std::time::Duration::from_secs(3))
    .tcp_nodelay(true);

let app = apigate::App::builder()
    .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
    .upstream(upstream)
    .build()?;
```

Use the hyper-util escape hatches for less common client or connector knobs:

```rust
let upstream = apigate::UpstreamConfig::default()
    .configure_connector(|connector| {
        connector.set_keepalive(Some(std::time::Duration::from_secs(30)));
        connector.set_recv_buffer_size(Some(512 * 1024));
        connector.set_happy_eyeballs_timeout(Some(std::time::Duration::from_millis(200)));
    })
    .configure_client(|client| {
        client.http2_adaptive_window(true);
    });
```

`App` methods:

| Method | Description |
|---|---|
| `.with_router(|router| ...)` | Transform the internal axum router and keep an `App`. |
| `.into_router()` | Consume the app and return the axum `Router`. |

Serving helpers:

```rust
apigate::run(addr, app).await?;
apigate::run_router(addr, router).await?;
```

Tune the listener socket when ApiGate owns it:

```rust
let config = apigate::ServeConfig::new()
    .backlog(2048)
    .reuse_address(true)
    .tcp_nodelay(true);

apigate::run_with(addr, app, config).await?;
```

`ServeConfig` also supports listener buffer sizes, IPv6-only binding, and
`SO_REUSEPORT` on supported Unix platforms. Use `run_router_with` for the same
socket options with a manually composed router.

