struct Input;
struct Output;

#[apigate::map]
async fn remap(input: Input) -> apigate::MapResult<Output> {
    let _ = input;
    Ok(Output)
}

#[apigate::service(name = "sales", prefix = "/sales")]
mod sales {
    use super::*;

    #[apigate::get("/items", map = remap)]
    async fn items() {}
}

fn main() {}
