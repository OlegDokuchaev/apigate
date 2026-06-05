use apigate::MapResult;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct FormInput {
    title: String,
    category: String,
}

// The output borrows from `input`: `title` is a slice of the input and `code`
// is a `&'static str`. No `.to_string()` is required because the `#[apigate::map]`
// wrapper serializes the output while `input` is still alive.
#[derive(Serialize)]
struct FormOutput<'a> {
    title: &'a str,
    code: &'static str,
}

// One map reused for both JSON and form routes proves the wrapper serializes
// according to the body kind chosen by the route at runtime.
#[apigate::map]
async fn remap(input: FormInput) -> MapResult<FormOutput<'_>> {
    Ok(FormOutput {
        title: input.title.trim(),
        code: match input.category.as_str() {
            "pets" => "P",
            "items" => "I",
            _ => "U",
        },
    })
}

#[apigate::service(name = "sales", prefix = "/sales")]
mod sales {
    use super::*;

    #[apigate::post("/json", json = FormInput, map = remap)]
    async fn json_route() {}

    #[apigate::post("/form", form = FormInput, map = remap)]
    async fn form_route() {}
}

fn main() {}
