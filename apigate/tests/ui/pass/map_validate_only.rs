use apigate::{MapResult, PartsCtx};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Input {
    name: String,
}

#[derive(Deserialize, Serialize)]
struct Replacement {
    name: String,
}

// validate-only: returns `()`, so the macro keeps the original request body.
// The map can still inspect the parsed input and mutate headers via `ctx`.
#[apigate::map]
async fn validate_json(input: Input, ctx: &mut PartsCtx<'_>) -> MapResult<()> {
    if input.name.is_empty() {
        return Err(apigate::ApigateError::bad_request("empty name"));
    }
    ctx.set_header("x-validated", "1")?;
    Ok(())
}

// validate-only on a form route.
#[apigate::map]
async fn validate_form(input: Input) -> MapResult<()> {
    let _ = input;
    Ok(())
}

// Replacing map still works: returning a value serializes it (backward compatible).
#[apigate::map]
async fn replace_json(input: Input) -> MapResult<Replacement> {
    Ok(Replacement { name: input.name })
}

// `Option<T>` is a normal serialized value, NOT "keep": Some -> value, None -> null.
#[apigate::map]
async fn optional_value(input: Input) -> MapResult<Option<Replacement>> {
    if input.name.is_empty() {
        Ok(None) // serialized as JSON null, body IS replaced
    } else {
        Ok(Some(Replacement { name: input.name }))
    }
}

#[apigate::service(name = "svc", prefix = "/svc")]
mod svc {
    use super::*;

    #[apigate::post("/validate-json", json = Input, map = validate_json)]
    async fn vj() {}

    #[apigate::post("/validate-form", form = Input, map = validate_form)]
    async fn vf() {}

    #[apigate::post("/replace", json = Input, map = replace_json)]
    async fn rj() {}

    #[apigate::post("/optional", json = Input, map = optional_value)]
    async fn ov() {}
}

fn main() {}
