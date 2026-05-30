struct Input;

#[apigate::map]
async fn bad(input: Input, raw: &apigate::RawBody) -> apigate::MapResult<()> {
    let _ = input;
    let _ = raw;
    Ok(())
}

fn main() {}
