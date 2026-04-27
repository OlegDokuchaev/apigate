use apigate::{HookResult, MapResult, PartsCtx, RequestScope};

struct AppState;

#[derive(Clone)]
struct LocalValue;

struct Input {
    value: String,
}

struct Output {
    value: String,
}

#[apigate::hook]
async fn hook_with_bare_ctx(ctx: &mut PartsCtx<'_>) -> HookResult {
    let _ = ctx.method();
    Ok(())
}

#[apigate::hook]
async fn hook_with_bare_scope(scope: &mut RequestScope<'_>) -> HookResult {
    scope.insert(LocalValue);
    Ok(())
}

#[apigate::map]
async fn map_with_bare_ctx_and_scope_values(
    input: Input,
    ctx: &mut PartsCtx<'_>,
    state: &AppState,
    local: &LocalValue,
) -> MapResult<Output> {
    let _ = ctx.method();
    let _ = state;
    let _ = local;
    Ok(Output { value: input.value })
}

fn main() {}
