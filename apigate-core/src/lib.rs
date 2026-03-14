mod app;
mod backend;
mod balancing;
mod hook;
mod policy;
mod proxy;
mod routing;

pub use app::{App, AppBuilder, run};
pub use hook::{BeforeFn, BeforeFuture, HookError, HookResult, PartsCtx};

#[derive(Clone, Copy, Debug)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

#[derive(Clone, Copy, Debug)]
pub struct RouteDef {
    pub method: Method,
    pub path: &'static str,
    pub to: Option<&'static str>,
    pub policy: Option<&'static str>,
    pub before: Option<BeforeFn>,
}

#[derive(Clone, Copy, Debug)]
pub struct Routes {
    pub service: &'static str,
    pub prefix: &'static str,
    pub policy: Option<&'static str>,
    pub routes: &'static [RouteDef],
}
