#[apigate::hook]
async fn bad(raw: apigate::RawBody) -> apigate::HookResult {
    let _ = raw;
    Ok(())
}

fn main() {}
