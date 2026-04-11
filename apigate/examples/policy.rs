//! Политики: routing-стратегии и балансировщики.
//! Демонстрирует HeaderSticky, PathSticky, ConsistentHash, LeastRequest, LeastTime.

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

    /// HeaderSticky: запросы с одинаковым x-user-id идут на один backend
    #[apigate::get("/user", before = [inject_user_id])]
    async fn user_profile() {}

    /// PathSticky: affinity по path-параметру {id}
    #[apigate::get("/{id}", policy = "path_sticky")]
    async fn by_id() {}

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
        .mount_service(sales::routes(), ["http://127.0.0.1:8081"])
        // HeaderSticky: affinity по x-user-id + consistent hash
        .policy("sticky", apigate::Policy::header_sticky("x-user-id"))
        // PathSticky: affinity по path-параметру {id} + consistent hash
        .policy("path_sticky", apigate::Policy::path_sticky("id"))
        // LeastRequest: наименьшее число in-flight запросов
        .policy("least_req", apigate::Policy::least_request())
        // LeastTime: наименьшая EWMA-латентность
        .policy("least_time", apigate::Policy::least_time())
        // RoundRobin: циклический перебор
        .policy("round_robin", apigate::Policy::round_robin())
        .build()?;

    print!(
        "\
policy — http://{listen}

HeaderSticky:  curl -H 'x-user-id: user-1' http://{listen}/sales/user
PathSticky:    curl http://{listen}/sales/abc-123
LeastRequest:  curl http://{listen}/sales/fast
LeastTime:     curl http://{listen}/sales/optimized
RoundRobin:    curl http://{listen}/sales/ping

С несколькими backend'ами балансировка распределяет запросы между ними.
С одним backend'ом все стратегии ведут себя одинаково.

Upstream:      caddy run --config apigate/examples/upstream/Caddyfile
"
    );

    apigate::run(listen, app).await?;
    Ok(())
}
