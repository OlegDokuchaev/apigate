use std::sync::Arc;

use http::uri::PathAndQuery;

use crate::policy::Policy;
use crate::{BeforeFn, MapFn};

#[derive(Clone)]
pub struct RouteMeta {
    pub service: &'static str,
    pub route_path: &'static str,
    pub prefix: &'static str,
    pub rewrite: Rewrite,
    pub policy: Arc<Policy>,
    pub before: Option<BeforeFn>,
    pub map: Option<MapFn>,
}

#[derive(Clone, Debug)]
pub enum Rewrite {
    StripPrefix,
    Fixed(FixedRewrite),
}

#[derive(Clone, Debug)]
pub struct FixedRewrite {
    raw: &'static str,
    no_query: PathAndQuery,
}

impl FixedRewrite {
    pub fn new(raw: &'static str) -> Self {
        Self {
            raw,
            no_query: PathAndQuery::from_static(raw),
        }
    }

    #[inline]
    pub fn raw(&self) -> &'static str {
        self.raw
    }

    #[inline]
    pub fn no_query(&self) -> &PathAndQuery {
        &self.no_query
    }
}
