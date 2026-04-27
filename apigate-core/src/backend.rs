use http::Uri;
use http::header::HeaderValue;
use http::uri::{Authority, Scheme};

use crate::error::BaseUriParseError;

#[derive(Clone, Debug)]
pub(crate) struct BaseUri {
    pub(crate) scheme: Scheme,
    pub(crate) authority: Authority,
    /// Pre-computed `"{scheme}://{authority}"` for fast URI building.
    pub(crate) prefix: String,
    /// Pre-computed Host header value.
    pub(crate) host_header: HeaderValue,
}

impl BaseUri {
    pub(crate) fn parse(s: &str) -> Result<Self, BaseUriParseError> {
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

/// A configured upstream backend.
///
/// Backends are created from URLs registered on [`crate::AppBuilder`].
#[derive(Clone, Debug)]
pub struct Backend {
    pub(crate) base: BaseUri,
}

impl Backend {
    pub(crate) fn new(base: BaseUri) -> Self {
        Self { base }
    }

    /// Returns the upstream URI prefix, for example `http://127.0.0.1:8081`.
    pub fn uri_prefix(&self) -> &str {
        &self.base.prefix
    }
}

/// Immutable collection of upstream backends for one service.
#[derive(Debug)]
pub struct BackendPool {
    backends: Box<[Backend]>,
}

impl BackendPool {
    pub(crate) fn new(bases: Vec<BaseUri>) -> Self {
        let backends = bases
            .into_iter()
            .map(Backend::new)
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Self { backends }
    }

    /// Returns whether the pool has no backends.
    pub fn is_empty(&self) -> bool {
        self.backends.is_empty()
    }

    /// Returns the number of backends in the pool.
    pub fn len(&self) -> usize {
        self.backends.len()
    }

    /// Returns a backend by stable pool index.
    pub fn get(&self, index: usize) -> Option<&Backend> {
        self.backends.get(index)
    }

    /// Returns all configured backends.
    pub fn backends(&self) -> &[Backend] {
        &self.backends
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::BaseUriParseError;

    #[test]
    fn base_uri_parse_stores_prefix_and_host_header() {
        let base = BaseUri::parse("http://127.0.0.1:8081").expect("valid backend URI");

        assert_eq!(base.scheme.as_str(), "http");
        assert_eq!(base.authority.as_str(), "127.0.0.1:8081");
        assert_eq!(base.prefix, "http://127.0.0.1:8081");
        assert_eq!(base.host_header, "127.0.0.1:8081");
    }

    #[test]
    fn base_uri_parse_rejects_missing_scheme() {
        let err = BaseUri::parse("127.0.0.1:8081").expect_err("scheme is required");

        assert!(matches!(err, BaseUriParseError::MissingScheme { .. }));
    }

    #[test]
    fn base_uri_parse_rejects_uri_without_backend_authority() {
        let err = BaseUri::parse("http:///path").expect_err("authority is required");

        assert!(matches!(
            err,
            BaseUriParseError::MissingAuthority { .. } | BaseUriParseError::InvalidUri { .. }
        ));
    }

    #[test]
    fn backend_pool_exposes_stable_backend_indices() {
        let pool = BackendPool::new(vec![
            BaseUri::parse("http://127.0.0.1:8081").unwrap(),
            BaseUri::parse("http://127.0.0.1:8082").unwrap(),
        ]);

        assert!(!pool.is_empty());
        assert_eq!(pool.len(), 2);
        assert_eq!(pool.get(0).unwrap().uri_prefix(), "http://127.0.0.1:8081");
        assert_eq!(pool.get(1).unwrap().uri_prefix(), "http://127.0.0.1:8082");
        assert!(pool.get(2).is_none());
        assert_eq!(pool.backends().len(), 2);
    }
}
