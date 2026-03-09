use std::sync::Arc;

use axum::body::Body;
use http::header::{CONNECTION, HOST};
use http::{HeaderMap, Request, Response, StatusCode, Uri};

use crate::backend::{Backend, BaseUri};
use crate::balancing::ProxyErrorKind;
use crate::policy::Policy;

#[derive(Clone, Copy, Debug)]
pub enum Rewrite {
    /// forwarded_path = incoming_path.strip_prefix(prefix)
    StripPrefix,
    /// фиксированный путь (без подстановки path params)
    Fixed(&'static str),
}

#[derive(Clone)]
pub struct RouteMeta {
    pub service: &'static str,
    pub route_path: &'static str,
    pub prefix: &'static str,
    pub rewrite: Rewrite,
    pub policy: Arc<Policy>,
}

pub async fn proxy_request(
    backend: &Backend,
    client: &hyper_util::client::legacy::Client<
        hyper_util::client::legacy::connect::HttpConnector,
        Body,
    >,
    meta: &RouteMeta,
    req: Request<Body>,
) -> Result<Response<Body>, ProxyErrorKind> {
    let (mut parts, body) = req.into_parts();

    // --- вычисляем path ---
    let incoming_path = parts.uri.path();

    let path = match meta.rewrite {
        Rewrite::StripPrefix => match incoming_path.strip_prefix(meta.prefix) {
            Some("") => "/",
            Some(rest) => {
                if rest.starts_with('/') {
                    rest
                } else {
                    "/"
                }
            }
            None => incoming_path,
        },
        Rewrite::Fixed(to) => to,
    };

    // TODO: later можно заменить на менее аллоцирующую сборку URI,
    // но для path rewrite одна новая строка здесь нормальна.
    let mut path_and_query = String::with_capacity(
        path.len()
            + parts
                .uri
                .path_and_query()
                .map(|p| p.as_str().len())
                .unwrap_or(0)
            + 1,
    );

    path_and_query.push_str(path);

    if let Some(q) = parts.uri.query() {
        path_and_query.push('?');
        path_and_query.push_str(q);
    }

    let new_uri = build_uri(&backend.base, &path_and_query)
        .map_err(|_| ProxyErrorKind::InvalidUpstreamUri)?;
    parts.uri = new_uri;

    // hop-by-hop headers
    strip_hop_headers(&mut parts.headers);

    // Host -> upstream authority
    if let Ok(host) = backend.base.authority.as_str().parse() {
        parts.headers.insert(HOST, host);
    } else {
        return Err(ProxyErrorKind::InvalidUpstreamUri);
    }

    let out_req = Request::from_parts(parts, body);

    let resp = client
        .request(out_req)
        .await
        .map_err(|_| ProxyErrorKind::UpstreamRequest)?;

    let (mut resp_parts, resp_body) = resp.into_parts();
    strip_hop_headers(&mut resp_parts.headers);

    Ok(Response::from_parts(resp_parts, Body::new(resp_body)))
}

fn build_uri(base: &BaseUri, path_and_query: &str) -> Result<Uri, http::Error> {
    Uri::builder()
        .scheme(base.scheme.clone())
        .authority(base.authority.clone())
        .path_and_query(path_and_query)
        .build()
}

pub fn bad_gateway(msg: &str) -> Response<Body> {
    Response::builder()
        .status(StatusCode::BAD_GATEWAY)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(msg.to_string()))
        .unwrap()
}

/// Минимальная фильтрация hop-by-hop заголовков (потом расширишь).
fn strip_hop_headers(headers: &mut HeaderMap) {
    // стандартный список hop-by-hop
    const HOPS: [&str; 8] = [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailers",
        "transfer-encoding",
        "upgrade",
    ];

    // удалить те, что перечислены явно
    for h in HOPS {
        headers.remove(h);
    }

    // удалить заголовки, перечисленные в Connection: ...
    if let Some(conn) = headers.get(CONNECTION).cloned() {
        if let Ok(s) = conn.to_str() {
            for name in s.split(',').map(|v| v.trim()).filter(|v| !v.is_empty()) {
                headers.remove(name);
            }
        }
        headers.remove(CONNECTION);
    }
}
