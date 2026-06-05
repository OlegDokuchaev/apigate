// A raw map cannot return a borrow of its `RawBody` input: the borrow cannot
// outlive the map function. Return an owned, zero-copy `Bytes` via `raw.slice(..)`
// or `raw.into_bytes()` instead of a `&[u8]`.
#[apigate::map]
async fn bad(raw: apigate::RawBody) -> apigate::MapResult<&'static [u8]> {
    Ok(&raw.as_bytes()[..])
}

fn main() {}
