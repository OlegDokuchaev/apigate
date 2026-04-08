use crate::backend::BackendPool;

use super::{AffinityKey, CandidateSet, RouteCtx, RouteStrategy, RoutingDecision};

#[derive(Debug, Clone)]
pub struct PathSticky {
    param: &'static str,
}

impl PathSticky {
    pub fn new(param: &'static str) -> Self {
        Self { param }
    }
}

impl RouteStrategy for PathSticky {
    fn route<'a>(&self, ctx: &RouteCtx<'a>, _pool: &'a BackendPool) -> RoutingDecision<'a> {
        let affinity = extract_param(ctx.route_path, ctx.prefix, ctx.uri.path(), self.param)
            .map(AffinityKey::borrowed);

        RoutingDecision {
            affinity,
            candidates: CandidateSet::All,
        }
    }
}

#[inline]
fn extract_param<'a>(
    route_path: &str,
    prefix: &str,
    uri_path: &'a str,
    param: &str,
) -> Option<&'a str> {
    let stripped = uri_path.strip_prefix(prefix).unwrap_or(uri_path);
    let mut path_segs = stripped.split('/').filter(|s| !s.is_empty());

    for tmpl in route_path.split('/').filter(|s| !s.is_empty()) {
        let value = path_segs.next()?;
        if let Some(name) = tmpl.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            if name == param {
                return Some(value);
            }
        }
    }

    None
}
