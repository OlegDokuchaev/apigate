#[apigate::service(name = "sales")]
mod sales {
    #[apigate::get("/items", unknown = Input)]
    async fn items() {}
}

fn main() {}
