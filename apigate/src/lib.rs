extern crate self as apigate;

#[doc(hidden)]
pub mod __private {
    use std::marker::PhantomData;

    pub use axum;
    pub use http;
    pub use serde_json;
    pub use serde_urlencoded;

    use apigate_core::{ApigateError, ApigatePipelineError, MapResult};
    use axum::body::Body;

    /// Whether a `#[apigate::map]` keeps the original request body or replaces it.
    pub enum BodyOutcome<T> {
        /// Forward the original body unchanged.
        Keep,
        /// Replace the body with this payload.
        Replace(T),
    }

    impl<T> From<Option<T>> for BodyOutcome<T> {
        #[inline]
        fn from(value: Option<T>) -> Self {
            match value {
                Some(value) => Self::Replace(value),
                None => Self::Keep,
            }
        }
    }

    /// Projects the payload type `T` out of a map's declared `MapResult<T>` return type.
    #[diagnostic::on_unimplemented(
        message = "#[apigate::map] must return `apigate::MapResult<T>`",
        label = "expected `apigate::MapResult<T>`, found `{Self}`"
    )]
    pub trait MapOutput {
        /// The payload `T` carried inside `MapResult<T>`.
        type Output;
    }

    impl<T> MapOutput for MapResult<T> {
        type Output = T;
    }

    /// The wire format a `#[apigate::map]` output is finished into.
    pub trait MapFormat {
        /// The payload produced by [`Finish::apigate_finish`].
        type Out;
    }

    /// Converts a map output `O` into the format `F`'s payload, dispatched by the
    /// receiver type of `Self`.
    ///
    /// The method returns `F::Out`: the projection goes through
    /// `F`, which the generated wrapper names in its where-clause, so it
    /// normalizes even while the wrapper is generic over `F`.
    ///
    /// `None` means "keep the original body".
    pub trait Finish<F, O>
    where
        F: MapFormat,
    {
        /// Serializes or moves `output` into format `F`'s payload.
        fn apigate_finish(self, output: O) -> MapResult<Option<F::Out>>;
    }

    /// Zero-sized dispatch marker used for autoref specialization.
    pub struct Finisher<F, O>(PhantomData<(F, O)>);

    impl<F, O> Finisher<F, O> {
        /// Binds `O` to the concrete type of `output` before method lookup.
        #[inline]
        pub fn for_output(_: &O) -> Self {
            Self(PhantomData)
        }
    }

    /// By-value keep impl: a `()` output (validate-only) preserves the original
    /// body for *every* format.
    impl<F: MapFormat> Finish<F, ()> for Finisher<F, ()> {
        #[inline]
        fn apigate_finish(self, _: ()) -> MapResult<Option<F::Out>> {
            Ok(None)
        }
    }

    /// JSON format
    pub struct Json;

    impl MapFormat for Json {
        type Out = String;
    }

    impl<O: serde::Serialize> Finish<Json, O> for &Finisher<Json, O> {
        #[inline]
        fn apigate_finish(self, output: O) -> MapResult<Option<String>> {
            serde_json::to_string(&output).map(Some).map_err(|err| {
                ApigateError::from(ApigatePipelineError::FailedSerializeMappedJson(
                    err.to_string(),
                ))
            })
        }
    }

    /// URL-encoded form format
    pub struct Form;

    impl MapFormat for Form {
        type Out = String;
    }

    impl<O: serde::Serialize> Finish<Form, O> for &Finisher<Form, O> {
        #[inline]
        fn apigate_finish(self, output: O) -> MapResult<Option<String>> {
            serde_urlencoded::to_string(&output)
                .map(Some)
                .map_err(|err| {
                    ApigateError::from(ApigatePipelineError::FailedSerializeMappedForm(
                        err.to_string(),
                    ))
                })
        }
    }

    /// Raw format
    pub struct Raw;

    impl MapFormat for Raw {
        type Out = Body;
    }

    impl<O: Into<Body>> Finish<Raw, O> for &Finisher<Raw, O> {
        #[inline]
        fn apigate_finish(self, output: O) -> MapResult<Option<Body>> {
            Ok(Some(output.into()))
        }
    }
}

pub use apigate_core::balancing;
pub use apigate_core::policy::Policy;
pub use apigate_core::routing;
pub use apigate_core::{
    ApigateBuildError, ApigateCoreError, ApigateError, ApigateFrameworkError, ApigatePipelineError,
    App, AppBuilder, Backend, BackendPool, BaseUriParseError, Bytes, DstChunk, HookResult,
    MapResult, Method, PartsCtx, PipelineFn, PipelineFuture, PipelineResult, RawBody, RequestScope,
    RewriteSpec, RewriteTemplate, RouteDef, Routes, RuntimeEvent, RuntimeEventKind,
    RuntimeObserver, ServeConfig, SrcSeg, UpstreamConfig, default_error_renderer,
    default_tracing_observer, run, run_router, run_router_with, run_with,
};
pub use apigate_macros::*;
