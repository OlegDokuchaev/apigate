use crate::backend::{Backend, BaseUri};
use crate::balancing::ProxyErrorKind;
use crate::route::{Rewrite, RouteMeta};
use axum::body::Body;
use http::header::{
    CONNECTION, HOST, PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, TE, TRAILER, TRANSFER_ENCODING,
    UPGRADE,
};
use http::uri::{PathAndQuery, Uri};
use http::{HeaderMap, HeaderName, Request, Response, StatusCode};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use smallvec::SmallVec;

pub async fn proxy_request(
    backend: &Backend,
    client: &Client<HttpConnector, Body>,
    meta: &RouteMeta,
    req: Request<Body>,
) -> Result<Response<Body>, ProxyErrorKind> {
    let (mut parts, body) = req.into_parts();

    let incoming_uri = std::mem::take(&mut parts.uri);
    parts.uri = rewrite_uri(&backend.base, meta, incoming_uri)?;

    strip_hop_headers(&mut parts.headers);
    parts.headers.insert(HOST, backend.base.host_header.clone());

    let resp = client
        .request(Request::from_parts(parts, body))
        .await
        .map_err(|_| ProxyErrorKind::UpstreamRequest)?;

    let (mut resp_parts, resp_body) = resp.into_parts();
    strip_hop_headers(&mut resp_parts.headers);

    Ok(Response::from_parts(resp_parts, Body::new(resp_body)))
}

#[inline]
fn rewrite_uri(base: &BaseUri, meta: &RouteMeta, uri: Uri) -> Result<Uri, ProxyErrorKind> {
    let new_pq = rewrite_path_and_query(meta, &uri)?;

    let mut uri_parts = uri.into_parts();
    uri_parts.scheme = Some(base.scheme.clone());
    uri_parts.authority = Some(base.authority.clone());

    if let Some(pq) = new_pq {
        uri_parts.path_and_query = Some(pq);
    }

    Uri::from_parts(uri_parts).map_err(|_| ProxyErrorKind::InvalidUpstreamUri)
}

#[inline]
fn rewrite_path_and_query(
    meta: &RouteMeta,
    uri: &Uri,
) -> Result<Option<PathAndQuery>, ProxyErrorKind> {
    let query = uri.query();

    match &meta.rewrite {
        Rewrite::Fixed(fixed) => {
            if query.is_none() {
                Ok(Some(fixed.no_query().clone()))
            } else {
                Ok(Some(build_path_and_query(fixed.raw(), query)?))
            }
        }

        Rewrite::StripPrefix => {
            let incoming_path = uri.path();

            let stripped = match incoming_path.strip_prefix(meta.prefix) {
                None => return Ok(None),
                Some("") | Some(_) if !incoming_path[meta.prefix.len()..].starts_with('/') => "/",
                Some(rest) => rest,
            };

            if query.is_none() {
                Ok(Some(
                    PathAndQuery::try_from(stripped)
                        .map_err(|_| ProxyErrorKind::InvalidUpstreamUri)?,
                ))
            } else {
                Ok(Some(build_path_and_query(stripped, query)?))
            }
        }
    }
}

#[inline]
fn build_path_and_query(path: &str, query: Option<&str>) -> Result<PathAndQuery, ProxyErrorKind> {
    if let Some(q) = query {
        let mut s = String::with_capacity(path.len() + 1 + q.len());
        s.push_str(path);
        s.push('?');
        s.push_str(q);
        PathAndQuery::try_from(s).map_err(|_| ProxyErrorKind::InvalidUpstreamUri)
    } else {
        PathAndQuery::try_from(path).map_err(|_| ProxyErrorKind::InvalidUpstreamUri)
    }
}

pub fn strip_hop_headers(headers: &mut HeaderMap) {
    let mut connection_tokens: SmallVec<[HeaderName; 8]> = SmallVec::new();

    for value in headers.get_all(CONNECTION).iter() {
        if let Ok(s) = value.to_str() {
            for token in s.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                if let Ok(name) = HeaderName::from_bytes(token.as_bytes()) {
                    connection_tokens.push(name);
                }
            }
        }
    }

    headers.remove(CONNECTION);

    for name in connection_tokens {
        headers.remove(name);
    }

    headers.remove(PROXY_AUTHENTICATE);
    headers.remove(PROXY_AUTHORIZATION);
    headers.remove(TE);
    headers.remove(TRAILER);
    headers.remove(TRANSFER_ENCODING);
    headers.remove(UPGRADE);
    headers.remove("keep-alive");
    headers.remove("proxy-connection");
}

pub fn bad_gateway(msg: impl Into<Body>) -> Response<Body> {
    Response::builder()
        .status(StatusCode::BAD_GATEWAY)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(msg.into())
        .unwrap()
}
