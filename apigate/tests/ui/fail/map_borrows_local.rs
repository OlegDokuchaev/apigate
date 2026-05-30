use apigate::MapResult;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Input {
    name: String,
}

#[derive(Serialize)]
struct Output<'a> {
    name: &'a str,
}

// The output borrows a value computed *inside* the body. That borrow would
// dangle once the body finishes, so this must not compile. Owned locals should
// be moved into the output instead of borrowed.
#[apigate::map]
async fn remap(input: Input) -> MapResult<Output<'_>> {
    let computed = input.name.to_lowercase();
    Ok(Output { name: &computed })
}

fn main() {}
