use apigate::MapResult;

#[derive(serde::Deserialize)]
struct Input {
    name: String,
}

// `NotSerialize` is neither `()` (keep) nor `Serialize` (replace), so the map
// output cannot be turned into a body: the `Json` format's `Finish` impl
// requires `Serialize`, which is unsatisfied -> compile error.
struct NotSerialize {
    name: String,
}

#[apigate::map]
async fn bad(input: Input) -> MapResult<NotSerialize> {
    Ok(NotSerialize { name: input.name })
}

#[apigate::service(name = "svc", prefix = "/svc")]
mod svc {
    use super::*;

    #[apigate::post("/bad", json = Input, map = bad)]
    async fn b() {}
}

fn main() {}
