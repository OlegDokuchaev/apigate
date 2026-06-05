use apigate::{Bytes, MapResult, PartsCtx, RawBody};

// A schema-less `RawBody` map turns its output into the new body by type: return
// the `RawBody` itself, a zero-copy `Bytes` slice/owned `Bytes`, a `Vec<u8>`/
// `String`, or `()` to keep the original body unchanged (validate-only).

// Forward the exact request bytes straight through (zero-copy move).
#[apigate::map]
async fn passthrough(raw: RawBody, ctx: &mut PartsCtx<'_>) -> MapResult<RawBody> {
    ctx.set_header("x-raw-len", raw.len().to_string())?;
    Ok(raw)
}

// validate-only: returns `()`, so the original request bytes are forwarded.
#[apigate::map]
async fn inspect_raw(raw: RawBody, ctx: &mut PartsCtx<'_>) -> MapResult<()> {
    if raw.is_empty() {
        return Err(apigate::ApigateError::bad_request("empty body"));
    }
    ctx.set_header("x-raw-len", raw.len().to_string())?;
    Ok(())
}

// `String` output is moved into the body without copying.
#[apigate::map]
async fn forward_string(raw: RawBody) -> MapResult<String> {
    Ok(String::from_utf8_lossy(raw.as_bytes()).into_owned())
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

    #[apigate::post("/inspect", map = inspect_raw)]
    async fn inspect_route() {}

    #[apigate::post("/string", map = forward_string)]
    async fn string_route() {}

    #[apigate::post("/slice", map = forward_slice)]
    async fn slice_route() {}

    #[apigate::post("/bytes", map = forward_bytes)]
    async fn bytes_route() {}

    #[apigate::post("/vec", map = forward_vec)]
    async fn vec_route() {}
}

fn main() {}
