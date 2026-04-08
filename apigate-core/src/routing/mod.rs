mod header_sticky;
mod no_route_key;
mod path_sticky;

use std::borrow::Cow;

use crate::backend::BackendPool;

pub use header_sticky::HeaderSticky;
pub use no_route_key::NoRouteKey;
pub use path_sticky::PathSticky;

pub struct RouteCtx<'a> {
    pub service: &'a str,
    pub prefix: &'a str,
    pub route_path: &'a str,
    pub method: &'a http::Method,
    pub uri: &'a http::Uri,
    pub headers: &'a http::HeaderMap,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct AffinityKey<'a>(Cow<'a, str>);

impl<'a> AffinityKey<'a> {
    pub fn borrowed(value: &'a str) -> Self {
        Self(Cow::Borrowed(value))
    }

    pub fn owned(value: impl Into<String>) -> Self {
        Self(Cow::Owned(value.into()))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    pub fn into_owned(self) -> String {
        self.0.into_owned()
    }
}

#[derive(Clone, Debug)]
pub enum CandidateSet<'a> {
    All,
    Indices(&'a [usize]),
}

#[derive(Clone, Debug)]
pub struct RoutingDecision<'a> {
    pub affinity: Option<AffinityKey<'a>>,
    pub candidates: CandidateSet<'a>,
}

pub trait RouteStrategy: Send + Sync + 'static {
    fn route<'a>(&self, ctx: &RouteCtx<'a>, pool: &'a BackendPool) -> RoutingDecision<'a>;
}
