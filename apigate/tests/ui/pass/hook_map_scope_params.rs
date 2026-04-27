use apigate::{HookResult, MapResult, PartsCtx, RequestScope};

#[derive(Clone)]
struct RequestMeta {
    id: u64,
}

struct Input;
struct Output;
struct AppState;

#[apigate::hook]
async fn set_meta(scope: &mut RequestScope<'_>) -> HookResult {
    scope.insert(RequestMeta { id: 42 });
    Ok(())
}

#[apigate::hook]
async fn take_meta(meta: RequestMeta, ctx: &mut PartsCtx<'_>) -> HookResult {
    ctx.set_header("x-request-id", meta.id.to_string())?;
    Ok(())
}

#[apigate::map]
async fn map_with_state(input: Input, state: &AppState) -> MapResult<Output> {
    let _ = input;
    let _ = state;
    Ok(Output)
}

fn main() {}
