struct QueryInput;
struct Output;

#[apigate::map]
async fn remap(input: QueryInput) -> apigate::MapResult<Output> {
    let _ = input;
    Ok(Output)
}

#[apigate::service(name = "sales", prefix = "/sales")]
mod sales {
    use super::*;

    #[apigate::get("/items", query = QueryInput, map = remap)]
    async fn items() {}
}

fn main() {}
