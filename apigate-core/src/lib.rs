mod app;
mod backend;
mod balancing;
mod hook;
mod policy;
mod proxy;
mod routing;
mod map;
mod error;

pub use app::{App, AppBuilder, run};
pub use hook::{BeforeFn, BeforeFuture, HookResult, PartsCtx};
pub use map::{MapFn, MapFuture, MapRequestResult, MapResult};
pub use error::ApigateError;

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
    pub map: Option<MapFn>,
}

#[derive(Clone, Copy, Debug)]
pub struct Routes {
    pub service: &'static str,
    pub prefix: &'static str,
    pub policy: Option<&'static str>,
    pub routes: &'static [RouteDef],
}
