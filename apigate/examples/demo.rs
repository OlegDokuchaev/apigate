use std::net::SocketAddr;

#[apigate::service(name = "sales", prefix = "/sales")]
mod sales {
    // 1) to не указан => проксируем в тот же path
    #[apigate::get("/ping")]
    async fn ping() {}

    // 2) Пример alias: внешний путь отличается от внутреннего
    #[apigate::get("/public", to = "/internal")]
    async fn public_alias() {}

    // 3) Маршрут с параметром (в минимальной версии apigate он просто прокинется как путь)
    #[apigate::get("/{id}")]
    async fn get_by_id() {}

    // 4) Фоллбек для всего остального внутри /sales/*
    #[apigate::get("/anything")]
    async fn anything() {}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listen: SocketAddr = "127.0.0.1:8080".parse()?;

    // Минимальный upstream (один адрес) для сервиса "sales"
    let app = apigate::App::builder()
        .backend("sales", ["http://127.0.0.1:8081"])
        .mount(sales::routes())
        .build()
        .map_err(anyhow::Error::msg)?;

    println!("apigate demo listening on http://{listen}");
    println!("try: curl -i http://{listen}/sales/ping");

    apigate::run(listen, app).await?;
    Ok(())
}
