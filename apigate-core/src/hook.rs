use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;

use axum::response::{IntoResponse, Response};
use http::StatusCode;
use http::header::{HeaderName, HeaderValue};
use http::request::Parts;

pub type BeforeFuture<'a> = Pin<Box<dyn Future<Output = HookResult> + Send + 'a>>;
pub type BeforeFn = for<'a> fn(PartsCtx<'a>) -> BeforeFuture<'a>;
pub type HookResult = Result<(), HookError>;

#[derive(Debug, Clone)]
pub struct HookError {
    status: StatusCode,
    message: Cow<'static, str>,
}

impl HookError {
    pub fn new(status: StatusCode, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn unauthorized(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, message)
    }

    pub fn forbidden(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    pub fn internal(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

impl IntoResponse for HookError {
    fn into_response(self) -> Response {
        (
            self.status,
            [("content-type", "text/plain; charset=utf-8")],
            self.message.into_owned(),
        )
            .into_response()
    }
}

pub struct PartsCtx<'a> {
    service: &'static str,
    route_path: &'static str,
    parts: &'a mut Parts,
}

impl<'a> PartsCtx<'a> {
    pub fn new(service: &'static str, route_path: &'static str, parts: &'a mut Parts) -> Self {
        Self {
            service,
            route_path,
            parts,
        }
    }

    pub fn service(&self) -> &'static str {
        self.service
    }

    pub fn route_path(&self) -> &'static str {
        self.route_path
    }

    pub fn method(&self) -> &http::Method {
        &self.parts.method
    }

    pub fn uri(&self) -> &http::Uri {
        &self.parts.uri
    }

    pub fn uri_mut(&mut self) -> &mut http::Uri {
        &mut self.parts.uri
    }

    pub fn headers(&self) -> &http::HeaderMap {
        &self.parts.headers
    }

    pub fn headers_mut(&mut self) -> &mut http::HeaderMap {
        &mut self.parts.headers
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.parts.headers.get(name).and_then(|v| v.to_str().ok())
    }

    pub fn set_header(
        &mut self,
        name: impl TryInto<HeaderName>,
        value: impl TryInto<HeaderValue>,
    ) -> Result<(), HookError> {
        let name = name
            .try_into()
            .map_err(|_| HookError::bad_request("invalid header name"))?;
        let value = value
            .try_into()
            .map_err(|_| HookError::bad_request("invalid header value"))?;

        self.parts.headers.insert(name, value);
        Ok(())
    }

    pub fn set_header_if_absent(
        &mut self,
        name: impl TryInto<HeaderName>,
        value: impl TryInto<HeaderValue>,
    ) -> Result<(), HookError> {
        let name = name
            .try_into()
            .map_err(|_| HookError::bad_request("invalid header name"))?;
        if self.parts.headers.contains_key(&name) {
            return Ok(());
        }

        let value = value
            .try_into()
            .map_err(|_| HookError::bad_request("invalid header value"))?;

        self.parts.headers.insert(name, value);
        Ok(())
    }

    pub fn remove_header(&mut self, name: &str) {
        self.parts.headers.remove(name);
    }

    pub fn extensions(&self) -> &http::Extensions {
        &self.parts.extensions
    }

    pub fn extensions_mut(&mut self) -> &mut http::Extensions {
        &mut self.parts.extensions
    }
}
