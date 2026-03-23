use std::future::Future;
use std::pin::Pin;

use crate::PartsCtx;
use crate::error::ApigateError;
use axum::body::Body;

pub type MapFuture<'a> = Pin<Box<dyn Future<Output = MapBodyResult> + Send + 'a>>;
pub type MapFn = for<'a> fn(PartsCtx<'a>, Body, usize) -> MapFuture<'a>;

pub type MapRequestResult = Result<http::Request<Body>, ApigateError>;
pub type MapResult<T> = Result<T, ApigateError>;
pub type MapBodyResult = Result<Body, ApigateError>;
