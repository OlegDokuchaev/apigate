mod app;
mod backend;
pub mod balancing;
mod error;
mod observability;
mod parts_ctx;
mod pipeline;
pub mod policy;
mod proxy;
mod route;
pub mod routing;

pub use app::{App, AppBuilder, run, run_router};
pub use error::{
    ApigateBuildError, ApigateCoreError, ApigateError, ApigateFrameworkError, ApigatePipelineError,
    BaseUriParseError, default_error_renderer,
};
pub use observability::{
    RuntimeEvent, RuntimeEventKind, RuntimeObserver, default_tracing_observer,
};
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
