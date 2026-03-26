extern crate self as apigate;

pub mod __private {
    pub use axum;
    pub use http;
    pub use serde_json;
    pub use serde_urlencoded;
}

pub use apigate_core::{
    ApigateError, App, AppBuilder, BeforeFn, BeforeFuture, DstChunk, HookResult, MapFn, MapFuture,
    MapResult, Method, PartsCtx, RewriteSpec, RewriteTemplate, RouteDef, Routes, SrcSeg, run,
};
pub use apigate_macros::*;
