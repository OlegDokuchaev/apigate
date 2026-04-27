use axum::extract::FromRequestParts;

use crate::error::{ApigateCoreError, ApigateError};
use http::header::{HeaderName, HeaderValue};
use http::request::Parts;

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::response::IntoResponse;
    use http::{Method, Request, StatusCode};

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct ExtensionValue(&'static str);

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
