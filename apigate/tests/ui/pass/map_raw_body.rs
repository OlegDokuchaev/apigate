use apigate::{MapResult, PartsCtx, RawBody, RequestScope};
use serde::{Deserialize, Serialize};

struct AppState;

#[derive(Deserialize)]
struct JsonInput;

#[derive(Clone, Deserialize)]
struct QueryInput;

#[derive(Serialize)]
struct Output;

// Typed `json` input plus a `RawBody` side parameter and borrowed shared state.
// `RawBody` is owned, so it composes with `&AppState` and `&mut PartsCtx`.
#[apigate::map]
async fn remap_json(
    input: JsonInput,
    raw: RawBody,
    state: &AppState,
    ctx: &mut PartsCtx<'_>,
) -> MapResult<Output> {
    let _ = input;
    let _ = raw.as_bytes();
    let _ = state;
    let _ = ctx;
    Ok(Output)
}

// No body data: `RawBody` is the map input, output is `impl Into<Body>`.
#[apigate::map]
async fn rewrap(raw: RawBody, scope: &mut RequestScope<'_>) -> MapResult<Vec<u8>> {
    let _ = scope;
    Ok(raw.as_bytes().to_vec())
}

// `query` is independent from body data, so it composes with a `RawBody` map:
// the parsed query comes from scope, the raw body is the map input.
#[apigate::map]
async fn rewrap_with_query(raw: RawBody, query: QueryInput) -> MapResult<Vec<u8>> {
    let _ = query;
    Ok(raw.as_bytes().to_vec())
}

#[apigate::service(name = "sales", prefix = "/sales")]
mod sales {
    use super::*;

    #[apigate::post("/json", json = JsonInput, map = remap_json)]
    async fn json_route() {}

    #[apigate::post("/raw", map = rewrap)]
    async fn raw_route() {}

    #[apigate::post("/raw-q", query = QueryInput, map = rewrap_with_query)]
    async fn raw_query_route() {}
}

fn main() {}
