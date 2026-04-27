struct AppState;

#[apigate::hook]
async fn hook(
    scope: &mut apigate::RequestScope<'_>,
    state: &AppState,
) -> apigate::HookResult {
    let _ = scope;
    let _ = state;
    Ok(())
}

fn main() {}
