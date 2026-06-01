use apigate::{MapResult, RawBody};

#[apigate::service(name = "svc", prefix = "/svc")]
mod svc {
    use super::*;

    // A `multipart` route may attach a map, provided the map takes the body raw:
    // the unparsed multipart bytes are handed to it as `RawBody`.
    #[apigate::map]
    async fn passthrough(raw: RawBody) -> MapResult<RawBody> {
        Ok(raw)
    }

    #[apigate::post("/upload", multipart, map = passthrough)]
    async fn upload() {}
}

fn main() {}
