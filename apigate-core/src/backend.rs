use std::sync::atomic::{AtomicUsize, Ordering};

use http::Uri;
use http::uri::{Authority, Scheme};

#[derive(Clone, Debug)]
pub struct BaseUri {
    pub scheme: Scheme,
    pub authority: Authority,
}

impl BaseUri {
    pub fn parse(s: &str) -> Result<Self, String> {
        let uri: Uri = s.parse().map_err(|e| format!("invalid uri `{s}`: {e}"))?;
        let scheme = uri
            .scheme()
            .cloned()
            .ok_or_else(|| format!("uri `{s}` missing scheme (expected http://...)"))?;
        let authority = uri
            .authority()
            .cloned()
            .ok_or_else(|| format!("uri `{s}` missing authority (expected http://host:port)"))?;

        Ok(Self { scheme, authority })
    }
}

#[derive(Debug)]
pub struct BackendPool {
    bases: Vec<BaseUri>,
    rr: AtomicUsize,
}

impl BackendPool {
    pub fn new(bases: Vec<BaseUri>) -> Self {
        Self {
            bases,
            rr: AtomicUsize::new(0),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.bases.is_empty()
    }

    pub fn pick(&self) -> Option<&BaseUri> {
        if self.bases.is_empty() {
            return None;
        }
        let i = self.rr.fetch_add(1, Ordering::Relaxed);
        Some(&self.bases[i % self.bases.len()])
    }
}
