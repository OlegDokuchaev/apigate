use http::Uri;
use http::header::HeaderValue;
use http::uri::{Authority, Scheme};

use crate::error::BaseUriParseError;

#[derive(Clone, Debug)]
pub struct BaseUri {
    pub scheme: Scheme,
    pub authority: Authority,
    /// Pre-computed `"{scheme}://{authority}"` for fast URI building.
    pub prefix: String,
    /// Pre-computed Host header value.
    pub host_header: HeaderValue,
}

impl BaseUri {
    pub fn parse(s: &str) -> Result<Self, BaseUriParseError> {
        let uri: Uri = s.parse().map_err(|source| BaseUriParseError::InvalidUri {
            input: s.to_owned(),
            source,
        })?;
        let scheme = uri
            .scheme()
            .cloned()
            .ok_or_else(|| BaseUriParseError::MissingScheme {
                input: s.to_owned(),
            })?;
        let authority =
            uri.authority()
                .cloned()
                .ok_or_else(|| BaseUriParseError::MissingAuthority {
                    input: s.to_owned(),
                })?;

        let prefix = format!("{scheme}://{authority}");
        let host_header = HeaderValue::from_str(authority.as_str()).map_err(|source| {
            BaseUriParseError::InvalidHostHeader {
                authority: authority.as_str().to_owned(),
                source,
            }
        })?;

        Ok(Self {
            scheme,
            authority,
            prefix,
            host_header,
        })
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
