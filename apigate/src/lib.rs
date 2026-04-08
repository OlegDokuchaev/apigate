extern crate self as apigate;

pub mod __private {
    pub use axum;
    pub use http;
    pub use serde_json;
    pub use serde_urlencoded;
}

pub use apigate_core::balancing;
pub use apigate_core::policy::Policy;
pub use apigate_core::routing;
pub use apigate_core::{
    ApigateError, App, AppBuilder, DstChunk, HookResult, MapResult, Method, PartsCtx, PipelineFn,
    PipelineFuture, PipelineResult, RequestScope, RewriteSpec, RewriteTemplate, RouteDef, Routes,
    SrcSeg, run,
};
pub use apigate_macros::*;
