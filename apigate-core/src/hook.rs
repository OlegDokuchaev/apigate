use std::future::Future;
use std::pin::Pin;

use crate::PartsCtx;
use crate::error::ApigateError;

pub type BeforeFuture<'a> = Pin<Box<dyn Future<Output = HookResult> + Send + 'a>>;
pub type BeforeFn = for<'a> fn(PartsCtx<'a>) -> BeforeFuture<'a>;
pub type HookResult = Result<(), ApigateError>;
