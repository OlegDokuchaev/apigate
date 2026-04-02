use std::sync::Arc;

use http::uri::PathAndQuery;

use crate::PipelineFn;
use crate::policy::Policy;

#[derive(Clone)]
pub struct RouteMeta {
    pub service: &'static str,
    pub route_path: &'static str,
    pub prefix: &'static str,
    pub rewrite: Rewrite,
    pub policy: Arc<Policy>,
    pub pipeline: Option<PipelineFn>,
}

#[derive(Clone, Debug)]
pub enum Rewrite {
    StripPrefix,
    Static(FixedRewrite),
    Template(&'static RewriteTemplate),
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

#[derive(Debug, Clone, Copy)]
pub struct RewriteTemplate {
    pub src: &'static [SrcSeg],
    pub dst: &'static [DstChunk],
    pub static_len: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum SrcSeg {
    Lit(&'static str),
    Param,
}

#[derive(Debug, Clone, Copy)]
pub enum DstChunk {
    Lit(&'static str),
    Capture { src_index: u8 },
}

#[derive(Clone, Copy, Debug)]
pub enum RewriteSpec {
    StripPrefix,
    Static(&'static str),
    Template(&'static RewriteTemplate),
}
