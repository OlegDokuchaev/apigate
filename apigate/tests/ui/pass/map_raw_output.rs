use apigate::{Bytes, MapResult, PartsCtx, RawBody};

// A schema-less `RawBody` map can forward bytes without allocating: return the
// `RawBody` itself, a zero-copy `Bytes` slice, or the owned `Bytes`. All compile
// because the generated pipeline accepts any `impl Into<Body>` as the new body.

// Forward the exact request bytes straight through (zero-copy move).
#[apigate::map]
async fn passthrough(raw: RawBody, ctx: &mut PartsCtx<'_>) -> MapResult<RawBody> {
    ctx.set_header("x-raw-len", raw.len().to_string())?;
    Ok(raw)
}

// Forward a zero-copy sub-slice: a reference-counted view over the same buffer.
#[apigate::map]
async fn forward_slice(raw: RawBody) -> MapResult<Bytes> {
    let end = raw.len().min(16);
    Ok(raw.slice(0..end))
}

// Move the underlying `Bytes` out without copying.
#[apigate::map]
async fn forward_bytes(raw: RawBody) -> MapResult<Bytes> {
    Ok(raw.into_bytes())
}

// Owned outputs still work unchanged (backward compatible).
#[apigate::map]
async fn forward_vec(raw: RawBody) -> MapResult<Vec<u8>> {
    Ok(raw.as_bytes().to_vec())
}

#[apigate::service(name = "echo", prefix = "/echo")]
mod echo {
    use super::*;

    #[apigate::post("/passthrough", map = passthrough)]
    async fn passthrough_route() {}

    #[apigate::post("/slice", map = forward_slice)]
    async fn slice_route() {}

    #[apigate::post("/bytes", map = forward_bytes)]
    async fn bytes_route() {}

    #[apigate::post("/vec", map = forward_vec)]
    async fn vec_route() {}
}

fn main() {}
