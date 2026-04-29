# ApiGate

[![CI](https://github.com/OlegDokuchaev/apigate/actions/workflows/ci.yml/badge.svg)](https://github.com/OlegDokuchaev/apigate/actions/workflows/ci.yml)
[![Coverage](https://github.com/OlegDokuchaev/apigate/actions/workflows/coverage.yml/badge.svg)](https://github.com/OlegDokuchaev/apigate/actions/workflows/coverage.yml)
[![Crates.io](https://img.shields.io/crates/v/apigate.svg)](https://crates.io/crates/apigate)
[![Docs.rs](https://docs.rs/apigate/badge.svg)](https://docs.rs/apigate)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

ApiGate is a macro-driven API gateway for Rust services.

It lets you declare reverse-proxy routes as Rust modules, validate typed request data, run pre-proxy hooks, transform requests before forwarding, choose upstream backends with routing/balancing policies, and customize errors and observability without exposing axum details in your application code.

Under the hood ApiGate is built on `axum`, `hyper-util`, `tower`, and `tracing`.

## Contents

- [What It Provides](#what-it-provides)
- [Benchmarks](#benchmarks)
- [Installation](#installation)
- [Supported Rust](#supported-rust)
- [Quick Start](#quick-start)
- [Core Concepts](#core-concepts)
- [API Reference](#api-reference)
- [Performance Notes](#performance-notes)
- [Examples](#examples)

## What It Provides

- Declarative service and route macros: `#[apigate::service]`, `#[apigate::get]`, `#[apigate::post]`, etc.
- Reverse proxying with streaming passthrough when a route does not need to read the body.
- Typed validation for `path`, `query`, `json`, and `form` inputs.
- `before` hooks for auth, headers, request metadata, and per-request state.
- `map` functions for typed request transformation before the upstream call.
- Multipart passthrough without buffering file bodies.
- Built-in policies: round-robin, consistent hash, header/path sticky, least-request, least-time.
- Custom routing strategies and custom balancers.
- Custom error rendering, including JSON envelopes and fully custom hook/map responses.
- Optional runtime observability through `tracing` or a custom runtime observer.
- External `tower`/`axum` middleware composition through the underlying router.

## Benchmarks

ApiGate is benchmarked against Kong, Apache APISIX, and a tuned Python ASGI
gateway in a reproducible load-test repo:
[OlegDokuchaev/apigate-benchmark](https://github.com/OlegDokuchaev/apigate-benchmark).

The latest run uses the same Go auth/data backends for every gateway and runs
one gateway at a time on a 4 vCPU / 10 GiB Linux host.

| Profile | Result |
|---|---|
| Steady, 2500 RPS | ApiGate p99 latency is 33-144% faster than APISIX, 52-391% faster than Kong, and 317-3857% faster than Python depending on route. |
| Ramp, 0 -> 20000 RPS | ApiGate delivers 6-31% more average RPS than APISIX, 1-41% more than Kong, and 115-184% more than Python before the p99 abort threshold. |
| Stress, 9000 RPS | ApiGate p99 latency is 7-70% faster than APISIX, 32-159% faster than Kong, and much faster than Python on saturated routes. |

See [Performance Notes](#performance-notes) for steady-state latency numbers
and links to the full benchmark results.

## Installation

Add the facade crate to your application:

```toml
[dependencies]
apigate = "0.2.6"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde = { version = "1", features = ["derive"] }
anyhow = "1"
```

Optional dependencies used in examples:

```toml
axum = "0.8"
http = "1"
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
tower-http = { version = "0.6", features = ["trace"] }
uuid = { version = "1", features = ["v4", "serde"] }
```

## Supported Rust

ApiGate declares Rust 1.88 as its package `rust-version`. Rust 1.88 stabilizes `let` chains in the Rust 2024 edition, which ApiGate uses in its implementation. CI checks that the library crates compile on Rust 1.88 and runs the full test suite on the latest stable toolchain.

## Quick Start

```rust
use std::net::SocketAddr;

#[apigate::service(prefix = "/sales")]
mod sales {
    #[apigate::get("/ping")]
    async fn ping() {}

    #[apigate::get("/public", to = "/internal")]
    async fn public_alias() {}

    #[apigate::get("/item/{id}/review", to = "/api/v2/reviews/{id}")]
    async fn item_review() {}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    let app = apigate::App::builder()
        .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
        .request_timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(3))
        .pool_idle_timeout(std::time::Duration::from_secs(60))
        .build()?;

    apigate::run(listen, app).await?;
    Ok(())
}
```

Run a local upstream and the example gateway:

```sh
caddy run --config apigate/examples/upstream/Caddyfile
cargo run --example basic
```

Then call:

```sh
curl http://127.0.0.1:8080/sales/ping
curl http://127.0.0.1:8080/sales/public
curl http://127.0.0.1:8080/sales/item/abc-123/review
```

## Core Concepts

ApiGate has three layers:

| Layer | Purpose |
|---|---|
| Service | A macro-generated collection of routes with an optional prefix and default policy. |
| Route | An HTTP method/path declaration with optional validation, hooks, mapping, rewrite, and policy override. |
| App | Runtime configuration: mounted services, upstream backends, shared state, timeouts, policies, errors, and observability. |

The normal flow is:

1. A request matches an axum route generated by ApiGate.
2. ApiGate optionally extracts typed path parameters.
3. `before` hooks run in order.
4. The route optionally validates or maps `query`, `json`, or `form` data.
5. A routing strategy selects candidate backends.
6. A balancer picks one backend.
7. ApiGate rewrites the URI and proxies the request to the upstream.
8. Runtime events are emitted only if a runtime observer is configured.

## API Reference

The detailed API guide lives in [docs/reference.md][api-reference]. It keeps
the README focused on installation, the first gateway, core concepts, and
performance.

Reference sections:

| Topic | Link |
|---|---|
| Services and route declarations | [Services][ref-services], [Routes][ref-routes] |
| Typed inputs and rewrites | [Typed Inputs][ref-typed-inputs], [Path Rewrites][ref-path-rewrites] |
| Hooks and maps | [Hooks][ref-hooks], [Maps][ref-maps], [Hook and Map Parameters][ref-hook-map-parameters] |
| Shared and per-request state | [Shared and Per-Request State][ref-state] |
| Errors | [Error Handling][ref-errors] |
| Observability and tower layers | [Runtime Observability and Tracing][ref-observability], [External Tower Layers][ref-tower] |
| Policies and balancing | [Policies, Routing, and Balancing][ref-policies] |
| Runtime tuning and serving helpers | [App Builder Reference][ref-app-builder] |

[api-reference]: https://github.com/OlegDokuchaev/apigate/blob/master/docs/reference.md
[ref-services]: https://github.com/OlegDokuchaev/apigate/blob/master/docs/reference.md#services
[ref-routes]: https://github.com/OlegDokuchaev/apigate/blob/master/docs/reference.md#routes
[ref-typed-inputs]: https://github.com/OlegDokuchaev/apigate/blob/master/docs/reference.md#typed-inputs
[ref-path-rewrites]: https://github.com/OlegDokuchaev/apigate/blob/master/docs/reference.md#path-rewrites
[ref-hooks]: https://github.com/OlegDokuchaev/apigate/blob/master/docs/reference.md#hooks
[ref-maps]: https://github.com/OlegDokuchaev/apigate/blob/master/docs/reference.md#maps
[ref-hook-map-parameters]: https://github.com/OlegDokuchaev/apigate/blob/master/docs/reference.md#hook-and-map-parameters
[ref-state]: https://github.com/OlegDokuchaev/apigate/blob/master/docs/reference.md#shared-and-per-request-state
[ref-errors]: https://github.com/OlegDokuchaev/apigate/blob/master/docs/reference.md#error-handling
[ref-observability]: https://github.com/OlegDokuchaev/apigate/blob/master/docs/reference.md#runtime-observability-and-tracing
[ref-tower]: https://github.com/OlegDokuchaev/apigate/blob/master/docs/reference.md#external-tower-layers
[ref-policies]: https://github.com/OlegDokuchaev/apigate/blob/master/docs/reference.md#policies-routing-and-balancing
[ref-app-builder]: https://github.com/OlegDokuchaev/apigate/blob/master/docs/reference.md#app-builder-reference

## Performance Notes

ApiGate is designed to avoid unnecessary work on routes that do not need it:

- Routes without `path`, `before`, `query`, `json`, `form`, or `map` have no generated pipeline and proxy the body as streaming passthrough.
- Multipart routes stream the request body without reading or buffering it.
- `json` and `form` validation read the body only when the route declares typed validation or mapping.
- `query` validation does not read the body.
- Shared app state is accessed by reference through `Extensions`; read-only `&T` access does not clone per request.
- Per-request `RequestScope` local storage allocates only when values are inserted.
- Route metadata is stored in a table and request routing carries a small route index.
- The upstream client uses keep-alive pooling, `TCP_NODELAY`, configurable connect timeout, configurable idle timeout, and exposes hyper-util client/connector tuning hooks.
- Built-in balancers are lock-free and use atomics.
- Runtime observer is disabled by default; when disabled, the hot path only performs an `Option` check.

Routes with `json`, `form`, or mapped bodies intentionally allocate for parsed/serialized payloads. Keep those routes for boundaries where validation or transformation is worth the cost.

### Benchmark Results

The benchmark suite compares ApiGate with Kong, Apache APISIX, and a tuned
Python ASGI gateway over the same Go auth/data backends. The latest run used a
4 vCPU / 10 GiB Linux host and tested plain proxying, auth hook + header
injection, JSON validation, and typed request mapping/rewrite.

Steady-state p99 latency at 2500 RPS:

| Route | ApiGate | APISIX | Kong | Python |
|---|---:|---:|---:|---:|
| `GET /items` | 1.76 ms | 2.40 ms | 2.84 ms | 7.33 ms |
| `GET /my-items` | 3.34 ms | 8.15 ms | 16.38 ms | 132.1 ms |
| `POST /items/search` | 1.91 ms | 2.53 ms | 2.94 ms | 20.52 ms |
| `POST /items/lookup` | 1.86 ms | 2.75 ms | 2.83 ms | 22.05 ms |

For comparative throughput and latency numbers, see the reproducible
[ApiGate benchmark suite](https://github.com/OlegDokuchaev/apigate-benchmark)
and its
[latest results](https://github.com/OlegDokuchaev/apigate-benchmark/blob/main/load-tests/RESULTS.md).

## Examples

Run the mock upstream first:

```sh
caddy run --config apigate/examples/upstream/Caddyfile
```

Then run any example:

```sh
cargo run --example basic
cargo run --example hooks
cargo run --example errors
cargo run --example logging
cargo run --example tower_logging
cargo run --example runtime_tuning
cargo run --example path
cargo run --example map
cargo run --example policy
cargo run --example multipart
```

Example guide:

| Example | Shows |
|---|---|
| `basic` | Passthrough proxying, static rewrite, rewrite templates. |
| `hooks` | Shared state, auth, header injection, hook chains, per-request scope data. |
| `errors` | Global JSON error renderer, user/debug message separation, custom JSON from hooks. |
| `logging` | Built-in tracing observer and custom runtime observer. |
| `tower_logging` | External `tower_http::TraceLayer` with `.with_router(...)`. |
| `runtime_tuning` | Listener socket tuning plus upstream hyper-util client/connector settings. |
| `path` | Typed path validation, path data in hooks, path data in maps. |
| `map` | Query, JSON, and form transformations. |
| `policy` | Header/path sticky routing, consistent hash, least-request, least-time, round-robin. |
| `multipart` | Multipart upload passthrough with and without auth. |

Each example prints ready-to-run `curl` commands.
