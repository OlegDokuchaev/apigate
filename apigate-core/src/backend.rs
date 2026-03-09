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

#[derive(Clone, Debug)]
pub struct Backend {
    pub base: BaseUri,
}

impl Backend {
    pub fn new(base: BaseUri) -> Self {
        Self { base }
    }
}

#[derive(Debug)]
pub struct BackendPool {
    backends: Box<[Backend]>,
}

impl BackendPool {
    pub fn new(bases: Vec<BaseUri>) -> Self {
        let backends = bases
            .into_iter()
            .map(Backend::new)
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Self { backends }
    }

    pub fn is_empty(&self) -> bool {
        self.backends.is_empty()
    }

    pub fn len(&self) -> usize {
        self.backends.len()
    }

    pub fn get(&self, index: usize) -> Option<&Backend> {
        self.backends.get(index)
    }

    pub fn backends(&self) -> &[Backend] {
        &self.backends
    }
}
