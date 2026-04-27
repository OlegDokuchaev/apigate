#[apigate::hook]
async fn hook(ctx: &apigate::PartsCtx<'_>) -> apigate::HookResult {
    let _ = ctx;
    Ok(())
}

fn main() {}
