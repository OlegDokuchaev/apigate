use std::future::Future;
use std::pin::Pin;

use crate::error::ApigateError;
use crate::PartsCtx;

pub type BeforeFuture<'a> = Pin<Box<dyn Future<Output = HookResult> + Send + 'a>>;
pub type BeforeFn = for<'a> fn(PartsCtx<'a>) -> BeforeFuture<'a>;
pub type HookResult = Result<(), ApigateError>;
