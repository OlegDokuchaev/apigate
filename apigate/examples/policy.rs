//! Policies: routing strategies and load balancers.
//! Demonstrates HeaderSticky, PathSticky, ConsistentHash, LeastRequest, and LeastTime.

use std::net::SocketAddr;

#[apigate::hook]
async fn inject_user_id(ctx: &mut apigate::PartsCtx) -> apigate::HookResult {
    let user_id = ctx.header("x-user-id").unwrap_or("anonymous").to_owned();
    ctx.set_header("x-user-id", &user_id)?;
    Ok(())
}

#[apigate::service(name = "sales", prefix = "/sales", policy = "sticky")]
mod sales {
    use super::*;

    /// HeaderSticky: requests with the same `x-user-id` use the same backend.
    #[apigate::get("/user", before = [inject_user_id])]
    async fn user_profile() {}

    /// PathSticky: affinity key comes from path parameter `{id}`.
    #[apigate::get("/{id}", policy = "path_sticky")]
    async fn by_id() {}

    /// LeastRequest: chooses the backend with the fewest in-flight requests.
    #[apigate::get("/fast", policy = "least_req")]
    async fn fast() {}

    /// LeastTime: chooses the backend with the lowest EWMA latency.
    #[apigate::get("/optimized", policy = "least_time")]
    async fn optimized() {}

    /// RoundRobin: cycles through backends.
    #[apigate::get("/ping", policy = "round_robin")]
    async fn ping() {}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    let app = apigate::App::builder()
        // Add more backend URLs here to make load-balancing visible in responses.
        .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
        // HeaderSticky: x-user-id affinity + consistent hash.
        .policy("sticky", apigate::Policy::header_sticky("x-user-id"))
        // PathSticky: path parameter `{id}` affinity + consistent hash.
        .policy("path_sticky", apigate::Policy::path_sticky("id"))
        // LeastRequest: fewest in-flight requests.
        .policy("least_req", apigate::Policy::least_request())
        // LeastTime: lowest EWMA latency.
        .policy("least_time", apigate::Policy::least_time())
        // RoundRobin: cyclic selection.
        .policy("round_robin", apigate::Policy::round_robin())
        .build()?;

    print!(
        "\
policy - http://{listen}

HeaderSticky:  curl -H 'x-user-id: user-1' http://{listen}/sales/user
PathSticky:    curl http://{listen}/sales/abc-123
LeastRequest:  curl http://{listen}/sales/fast
LeastTime:     curl http://{listen}/sales/optimized
RoundRobin:    curl http://{listen}/sales/ping

With multiple backends, load balancing distributes requests across them.
With one backend, all strategies resolve to the same backend.

Upstream:      caddy run --config apigate/examples/upstream/Caddyfile
"
    );

    apigate::run(listen, app).await?;
    Ok(())
}
