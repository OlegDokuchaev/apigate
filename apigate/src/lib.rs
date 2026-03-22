extern crate self as apigate;

pub mod __private {
    pub use axum;
    pub use http;
    pub use serde_json;
    pub use serde_urlencoded;
}

pub use apigate_macros::*;
pub use apigate_core::{
    run,
    App,
    AppBuilder,
    Routes,
    RouteDef,
    Method,
    PartsCtx,
    ApigateError,
    HookResult,
    MapResult,
    BeforeFn,
    BeforeFuture,
    MapFn,
    MapFuture,
};
