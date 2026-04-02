mod app;
mod backend;
mod balancing;
mod error;
mod parts_ctx;
mod pipeline;
mod policy;
mod proxy;
mod route;
mod routing;

pub use app::{App, AppBuilder, run};
pub use error::ApigateError;
pub use parts_ctx::PartsCtx;
pub use pipeline::{
    HookResult, MapResult, PipelineFn, PipelineFuture, PipelineResult, RequestScope,
};
pub use route::{DstChunk, RewriteSpec, RewriteTemplate, SrcSeg};

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
    pub rewrite: RewriteSpec,
    pub policy: Option<&'static str>,
    pub pipeline: Option<PipelineFn>,
}

#[derive(Clone, Copy, Debug)]
pub struct Routes {
    pub service: &'static str,
    pub prefix: &'static str,
    pub policy: Option<&'static str>,
    pub routes: &'static [RouteDef],
}
