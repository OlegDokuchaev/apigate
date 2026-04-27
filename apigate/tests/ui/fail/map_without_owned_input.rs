struct AppState;
struct Output;

#[apigate::map]
async fn map_without_input(
    state: &AppState,
    ctx: &mut apigate::PartsCtx<'_>,
) -> apigate::MapResult<Output> {
    let _ = state;
    let _ = ctx;
    Ok(Output)
}

fn main() {}
