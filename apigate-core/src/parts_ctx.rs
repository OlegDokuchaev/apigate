use axum::extract::FromRequestParts;

use crate::error::{ApigateCoreError, ApigateError, ApigatePipelineError};
use http::header::{HeaderName, HeaderValue};
use http::request::Parts;
use http::uri::PathAndQuery;

/// Mutable request-head context passed to hooks and maps.
///
/// `PartsCtx` exposes method, URI, headers, extensions, and route metadata
/// without giving hooks ownership of the request body.
pub struct PartsCtx<'a> {
    service: &'static str,
    route_path: &'static str,
    parts: &'a mut Parts,
}

impl<'a> PartsCtx<'a> {
    pub(crate) fn new(
        service: &'static str,
        route_path: &'static str,
        parts: &'a mut Parts,
    ) -> Self {
        Self {
            service,
            route_path,
            parts,
        }
    }

    /// Returns the logical service name for the current route.
    pub fn service(&self) -> &'static str {
        self.service
    }

    /// Returns the route path relative to the service prefix.
    pub fn route_path(&self) -> &'static str {
        self.route_path
    }

    /// Returns the incoming HTTP method.
    pub fn method(&self) -> &http::Method {
        &self.parts.method
    }

    /// Returns the current request URI.
    pub fn uri(&self) -> &http::Uri {
        &self.parts.uri
    }

    /// Returns a mutable reference to the request URI.
    pub fn uri_mut(&mut self) -> &mut http::Uri {
        &mut self.parts.uri
    }

    /// Serializes `query` as `application/x-www-form-urlencoded` data and
    /// replaces the URI query string.
    pub fn set_query<T>(&mut self, query: &T) -> Result<(), ApigateError>
    where
        T: serde::Serialize + ?Sized,
    {
        let encoded = self.serialize_query(query).map_err(|err| {
            ApigateError::from(ApigatePipelineError::FailedSerializeMappedQuery(
                err.to_string(),
            ))
        })?;
        self.set_encoded_query(&encoded)
    }

    /// Returns request headers.
    pub fn headers(&self) -> &http::HeaderMap {
        &self.parts.headers
    }

    /// Returns mutable request headers.
    pub fn headers_mut(&mut self) -> &mut http::HeaderMap {
        &mut self.parts.headers
    }

    /// Returns a UTF-8 header value by name.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.parts.headers.get(name).and_then(|v| v.to_str().ok())
    }

    /// Inserts or replaces a request header.
    pub fn set_header(
        &mut self,
        name: impl TryInto<HeaderName>,
        value: impl TryInto<HeaderValue>,
    ) -> Result<(), ApigateError> {
        let name = name
            .try_into()
            .map_err(|_| ApigateError::from(ApigateCoreError::InvalidHeaderName))?;
        let value = value
            .try_into()
            .map_err(|_| ApigateError::from(ApigateCoreError::InvalidHeaderValue))?;

        self.parts.headers.insert(name, value);
        Ok(())
    }

    /// Inserts a request header only when it is absent.
    pub fn set_header_if_absent(
        &mut self,
        name: impl TryInto<HeaderName>,
        value: impl TryInto<HeaderValue>,
    ) -> Result<(), ApigateError> {
        let name = name
            .try_into()
            .map_err(|_| ApigateError::from(ApigateCoreError::InvalidHeaderName))?;
        if self.parts.headers.contains_key(&name) {
            return Ok(());
        }

        let value = value
            .try_into()
            .map_err(|_| ApigateError::from(ApigateCoreError::InvalidHeaderValue))?;

        self.parts.headers.insert(name, value);
        Ok(())
    }

    /// Removes a request header by name.
    pub fn remove_header(&mut self, name: &str) {
        self.parts.headers.remove(name);
    }

    /// Returns request extensions.
    pub fn extensions(&self) -> &http::Extensions {
        &self.parts.extensions
    }

    /// Returns mutable request extensions.
    pub fn extensions_mut(&mut self) -> &mut http::Extensions {
        &mut self.parts.extensions
    }

    /// Extracts typed path parameters using axum's `Path` extractor.
    pub async fn extract_path<T>(&mut self) -> Result<T, ApigateError>
    where
        T: Send,
        axum::extract::Path<T>: FromRequestParts<()>,
    {
        axum::extract::Path::<T>::from_request_parts(self.parts, &())
            .await
            .map(|p| p.0)
            .map_err(|_| ApigateError::from(ApigateCoreError::InvalidPathParameters))
    }

    /// Extracts typed query parameters using `serde_html_form`.
    pub fn extract_query<T>(&self) -> Result<T, ApigateError>
    where
        T: serde::de::DeserializeOwned,
    {
        let raw = self.parts.uri.query().unwrap_or_default();
        serde_html_form::from_str(raw)
            .map_err(|err| ApigateError::from(ApigatePipelineError::InvalidQuery(err.to_string())))
    }

    fn serialize_query<T>(&self, query: &T) -> Result<String, serde_html_form::ser::Error>
    where
        T: serde::Serialize + ?Sized,
    {
        let mut encoded = String::with_capacity(self.parts.uri.query().map_or(0, str::len));
        serde_html_form::push_to_string(&mut encoded, query)?;
        Ok(encoded)
    }

    fn set_encoded_query(&mut self, encoded: &str) -> Result<(), ApigateError> {
        match (encoded.is_empty(), self.parts.uri.query()) {
            (true, None) => return Ok(()),
            (false, Some(current)) if current == encoded => return Ok(()),
            _ => {}
        }

        let path = self.parts.uri.path();
        if encoded.is_empty() && path == "/" {
            return self.replace_path_and_query(PathAndQuery::from_static("/"));
        }

        let mut path_and_query =
            String::with_capacity(path.len() + encoded.len() + usize::from(!encoded.is_empty()));
        path_and_query.push_str(path);
        if !encoded.is_empty() {
            path_and_query.push('?');
            path_and_query.push_str(encoded);
        }

        let path_and_query = PathAndQuery::from_maybe_shared(path_and_query).map_err(|err| {
            ApigateError::from(ApigatePipelineError::FailedRebuildUri(err.to_string()))
        })?;

        self.replace_path_and_query(path_and_query)
    }

    fn replace_path_and_query(&mut self, path_and_query: PathAndQuery) -> Result<(), ApigateError> {
        match (
            self.parts.uri.scheme().is_some(),
            self.parts.uri.authority().is_some(),
        ) {
            (true, false) => {
                return Err(ApigateError::from(ApigatePipelineError::FailedRebuildUri(
                    "cannot rebuild uri with scheme but no authority".to_owned(),
                )));
            }
            (false, true) => {
                return Err(ApigateError::from(ApigatePipelineError::FailedRebuildUri(
                    "cannot rebuild authority-form uri with path and query".to_owned(),
                )));
            }
            (true, true) | (false, false) => {}
        }

        let mut parts = std::mem::take(&mut self.parts.uri).into_parts();
        parts.path_and_query = Some(path_and_query);
        self.parts.uri = http::Uri::from_parts(parts).map_err(|err| {
            ApigateError::from(ApigatePipelineError::FailedRebuildUri(err.to_string()))
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::response::IntoResponse;
    use http::{Method, Request, StatusCode};
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct ExtensionValue(&'static str);

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct IncomingQuery {
        active: bool,
    }

    #[derive(Debug, Serialize)]
    struct UpstreamQuery<'a> {
        offset: u32,
        limit: u32,
        q: Option<&'a str>,
    }

    #[derive(Debug, Serialize)]
    struct EmptyQuery {}

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct IncomingListQuery {
        #[serde(default)]
        ids: Vec<u32>,
        #[serde(default, rename = "tags[]")]
        tags: Vec<String>,
    }

    #[derive(Debug, Serialize)]
    struct UpstreamListQuery<'a> {
        ids: &'a [u32],
        tags: &'a [&'a str],
    }

    #[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
    struct UpstreamListQueryOwned {
        #[serde(default)]
        ids: Vec<u32>,
        #[serde(default)]
        tags: Vec<String>,
    }

    fn parts() -> Parts {
        Request::builder()
            .method(Method::POST)
            .uri("/sales/123?active=true")
            .header("x-existing", "one")
            .body(Body::empty())
            .unwrap()
            .into_parts()
            .0
    }

    #[test]
    fn parts_ctx_exposes_route_and_request_metadata() {
        let mut parts = parts();
        let ctx = PartsCtx::new("sales", "/{id}", &mut parts);

        assert_eq!(ctx.service(), "sales");
        assert_eq!(ctx.route_path(), "/{id}");
        assert_eq!(ctx.method(), Method::POST);
        assert_eq!(ctx.uri().path(), "/sales/123");
        assert_eq!(ctx.uri().query(), Some("active=true"));
        assert_eq!(ctx.header("x-existing"), Some("one"));
    }

    #[test]
    fn parts_ctx_mutates_uri_headers_and_extensions() {
        let mut parts = parts();
        let mut ctx = PartsCtx::new("sales", "/{id}", &mut parts);

        *ctx.uri_mut() = "/internal/123".parse().unwrap();
        ctx.set_header("x-new", "two").unwrap();
        ctx.set_header_if_absent("x-existing", "ignored").unwrap();
        ctx.set_header_if_absent("x-absent", "three").unwrap();
        ctx.remove_header("x-new");
        ctx.extensions_mut().insert(ExtensionValue("value"));

        assert_eq!(ctx.uri().path(), "/internal/123");
        assert!(ctx.header("x-new").is_none());
        assert_eq!(ctx.header("x-existing"), Some("one"));
        assert_eq!(ctx.header("x-absent"), Some("three"));
        assert_eq!(
            ctx.extensions().get::<ExtensionValue>(),
            Some(&ExtensionValue("value"))
        );
    }

    #[test]
    fn parts_ctx_extracts_typed_query() {
        let mut parts = parts();
        let ctx = PartsCtx::new("sales", "/{id}", &mut parts);

        let query = ctx.extract_query::<IncomingQuery>().unwrap();

        assert_eq!(query, IncomingQuery { active: true });
    }

    #[test]
    fn parts_ctx_extracts_repeated_query_values_as_lists() {
        let mut parts = Request::builder()
            .method(Method::GET)
            .uri("/sales/search?ids=1&ids=2&tags[]=office&tags[]=sale")
            .body(Body::empty())
            .unwrap()
            .into_parts()
            .0;
        let ctx = PartsCtx::new("sales", "/search", &mut parts);

        let query = ctx.extract_query::<IncomingListQuery>().unwrap();

        assert_eq!(
            query,
            IncomingListQuery {
                ids: vec![1, 2],
                tags: vec!["office".to_owned(), "sale".to_owned()],
            }
        );
    }

    #[test]
    fn parts_ctx_replaces_query_from_serializable_value() {
        let mut parts = parts();
        let mut ctx = PartsCtx::new("sales", "/{id}", &mut parts);

        ctx.set_query(&UpstreamQuery {
            offset: 10,
            limit: 5,
            q: Some("hello world"),
        })
        .unwrap();

        assert_eq!(ctx.uri().path(), "/sales/123");
        let query: HashMap<String, String> =
            serde_html_form::from_str(ctx.uri().query().unwrap()).unwrap();
        assert_eq!(query.get("offset"), Some(&"10".to_owned()));
        assert_eq!(query.get("limit"), Some(&"5".to_owned()));
        assert_eq!(query.get("q"), Some(&"hello world".to_owned()));

        ctx.set_query(&EmptyQuery {}).unwrap();
        assert_eq!(ctx.uri().query(), None);
    }

    #[test]
    fn parts_ctx_serializes_lists_as_repeated_query_values() {
        let mut parts = Request::builder()
            .method(Method::GET)
            .uri("/sales/search")
            .body(Body::empty())
            .unwrap()
            .into_parts()
            .0;
        let mut ctx = PartsCtx::new("sales", "/search", &mut parts);

        ctx.set_query(&UpstreamListQuery {
            ids: &[1, 2],
            tags: &["office", "sale"],
        })
        .unwrap();

        assert_eq!(ctx.uri().query(), Some("ids=1&ids=2&tags=office&tags=sale"));
    }

    #[test]
    fn parts_ctx_round_trips_repeated_query_values() {
        let mut parts = Request::builder()
            .method(Method::GET)
            .uri("/sales/search?ids=1&ids=2&tags=office&tags=sale")
            .body(Body::empty())
            .unwrap()
            .into_parts()
            .0;
        let mut ctx = PartsCtx::new("sales", "/search", &mut parts);

        let query = ctx.extract_query::<UpstreamListQueryOwned>().unwrap();
        ctx.set_query(&query).unwrap();

        assert_eq!(ctx.uri().query(), Some("ids=1&ids=2&tags=office&tags=sale"));
    }

    #[test]
    fn parts_ctx_set_query_preserves_absolute_uri_parts() {
        let mut parts = Request::builder()
            .method(Method::GET)
            .uri("http://example.com/sales/123?active=true")
            .body(Body::empty())
            .unwrap()
            .into_parts()
            .0;
        let mut ctx = PartsCtx::new("sales", "/{id}", &mut parts);

        ctx.set_query(&UpstreamQuery {
            offset: 1,
            limit: 2,
            q: None,
        })
        .unwrap();

        assert_eq!(ctx.uri().scheme_str(), Some("http"));
        assert_eq!(
            ctx.uri().authority().map(|v| v.as_str()),
            Some("example.com")
        );
        assert_eq!(ctx.uri().path(), "/sales/123");
        assert_eq!(ctx.uri().query(), Some("offset=1&limit=2"));
    }

    #[test]
    fn parts_ctx_set_query_clears_root_query_without_allocating_path_data() {
        let mut parts = Request::builder()
            .method(Method::GET)
            .uri("http://example.com/?active=true")
            .body(Body::empty())
            .unwrap()
            .into_parts()
            .0;
        let mut ctx = PartsCtx::new("sales", "/", &mut parts);

        ctx.set_query(&EmptyQuery {}).unwrap();

        assert_eq!(ctx.uri().scheme_str(), Some("http"));
        assert_eq!(
            ctx.uri().authority().map(|v| v.as_str()),
            Some("example.com")
        );
        assert_eq!(ctx.uri().path(), "/");
        assert_eq!(ctx.uri().query(), None);
    }

    #[test]
    fn parts_ctx_set_query_rejects_authority_form_without_mutating_uri() {
        let mut parts = Request::builder()
            .method(Method::CONNECT)
            .uri("example.com:443")
            .body(Body::empty())
            .unwrap()
            .into_parts()
            .0;
        let original = parts.uri.clone();
        let mut ctx = PartsCtx::new("sales", "/", &mut parts);

        let err = ctx
            .set_query(&UpstreamQuery {
                offset: 1,
                limit: 2,
                q: None,
            })
            .expect_err("authority-form URI cannot carry path/query");

        assert_eq!(
            err.into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(ctx.uri(), &original);
    }

    #[test]
    fn parts_ctx_reports_invalid_header_mutations() {
        let mut parts = parts();
        let mut ctx = PartsCtx::new("sales", "/{id}", &mut parts);

        let invalid_name = ctx
            .set_header("bad header", "value")
            .expect_err("invalid header name");
        assert_eq!(
            invalid_name.into_response().status(),
            StatusCode::BAD_REQUEST
        );

        let invalid_value = ctx
            .set_header("x-value", "bad\nvalue")
            .expect_err("invalid header value");
        assert_eq!(
            invalid_value.into_response().status(),
            StatusCode::BAD_REQUEST
        );
    }
}
