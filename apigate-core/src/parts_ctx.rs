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
