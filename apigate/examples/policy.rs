//! Политики: routing-стратегии и балансировщики.
//! Демонстрирует HeaderSticky + ConsistentHash, LeastRequest, LeastTime.

use std::net::SocketAddr;

#[apigate::hook]
async fn inject_user_id(ctx: &mut apigate::PartsCtx<'_>) -> apigate::HookResult {
    let user_id = ctx.header("x-user-id").unwrap_or("anonymous").to_owned();
    ctx.set_header("x-user-id", &user_id)?;
    Ok(())
}

#[apigate::service(name = "sales", prefix = "/sales", policy = "sticky")]
mod sales {
    use super::*;

    /// Sticky sessions: запросы с одинаковым x-user-id идут на один backend
    #[apigate::get("/user", before = [inject_user_id])]
    async fn user_profile() {}

    /// LeastRequest: выбирает backend с наименьшим числом in-flight запросов
    #[apigate::get("/fast", policy = "least_req")]
    async fn fast() {}

    /// LeastTime: выбирает backend с наименьшей EWMA-латентностью
    #[apigate::get("/optimized", policy = "least_time")]
    async fn optimized() {}

    /// RoundRobin (дефолтная при отсутствии политики)
    #[apigate::get("/ping", policy = "round_robin")]
    async fn ping() {}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    let app = apigate::App::builder()
        // Несколько backend'ов — балансировка видна в ответах
        .backend("sales", ["http://127.0.0.1:8081"])
        // HeaderSticky: affinity по x-user-id + consistent hash
        .policy(
            "sticky",
            apigate::Policy::new()
                .router(apigate::routing::HeaderSticky::new("x-user-id"))
                .balancer(apigate::balancing::ConsistentHash::new()),
        )
        // LeastRequest: наименьшее число in-flight запросов
        .policy(
            "least_req",
            apigate::Policy::new()
                .balancer(apigate::balancing::LeastRequest::new()),
        )
        // LeastTime: наименьшая EWMA-латентность
        .policy(
            "least_time",
            apigate::Policy::new()
                .balancer(apigate::balancing::LeastTime::new()),
        )
        // RoundRobin: циклический перебор
        .policy(
            "round_robin",
            apigate::Policy::new()
                .balancer(apigate::balancing::RoundRobin::new()),
        )
        .mount(sales::routes())
        .build()
        .map_err(anyhow::Error::msg)?;

    print!("\
policy — http://{listen}

Sticky:        curl -H 'x-user-id: user-1' http://{listen}/sales/user
LeastRequest:  curl http://{listen}/sales/fast
LeastTime:     curl http://{listen}/sales/optimized
RoundRobin:    curl http://{listen}/sales/ping

С несколькими backend'ами балансировка распределяет запросы между ними.
С одним backend'ом все стратегии ведут себя одинаково.

Upstream:      caddy run --config apigate/examples/upstream/Caddyfile
");

    apigate::run(listen, app).await?;
    Ok(())
}
