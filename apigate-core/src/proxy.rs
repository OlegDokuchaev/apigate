use axum::body::Body;
use http::HeaderMap;
use http::header::{CONNECTION, HOST};
use http::{Request, Response, StatusCode, Uri};

use crate::backend::{BackendPool, BaseUri};

#[derive(Clone, Copy, Debug)]
pub enum Rewrite {
    /// Быстрый путь: forwarded_path = incoming_path.strip_prefix(prefix)
    StripPrefix,
    /// Фиксированный путь (минимальная версия, без подстановки path params)
    Fixed(&'static str),
}

#[derive(Clone, Debug)]
pub struct RouteMeta {
    pub service: &'static str,
    pub prefix: &'static str,
    pub rewrite: Rewrite,
}

pub async fn proxy_request(
    pool: &BackendPool,
    client: &hyper_util::client::legacy::Client<
        hyper_util::client::legacy::connect::HttpConnector,
        Body,
    >,
    meta: &RouteMeta,
    req: Request<Body>,
) -> Response<Body> {
    let base = match pool.pick() {
        Some(b) => b,
        None => return bad_gateway("no backends"),
    };

    let (mut parts, body) = req.into_parts();

    // --- вычисляем path ---
    let incoming_path = parts.uri.path();

    let path = match meta.rewrite {
        Rewrite::StripPrefix => {
            // ожидаем, что incoming_path начинается с prefix
            match incoming_path.strip_prefix(meta.prefix) {
                Some("") => "/", // если ровно "/sales" => в сервисе "/"
                Some(rest) => {
                    if rest.starts_with('/') {
                        rest
                    } else {
                        "/"
                    }
                }
                None => incoming_path, // fallback
            }
        }
        Rewrite::Fixed(to) => to,
    };

    // query прокидываем как есть
    // TODO: Dont allocate memory
    let mut pq = String::with_capacity(
        path.len()
            + parts
                .uri
                .path_and_query()
                .map(|p| p.as_str().len())
                .unwrap_or(0)
            + 1,
    );
    pq.push_str(path);
    if let Some(q) = parts.uri.query() {
        pq.push('?');
        pq.push_str(q);
    }

    let new_uri = build_uri(base, &pq);
    parts.uri = match new_uri {
        Ok(u) => u,
        Err(e) => return bad_gateway(&format!("bad upstream uri: {e}")),
    };

    // hop-by-hop headers
    strip_hop_headers(&mut parts.headers);

    // Host -> upstream authority
    if let Some(host) = parts.headers.get_mut(HOST) {
        *host = base.authority.as_str().parse().unwrap();
    } else {
        parts
            .headers
            .insert(HOST, base.authority.as_str().parse().unwrap());
    }

    let out_req = Request::from_parts(parts, body);

    let resp = match client.request(out_req).await {
        Ok(r) => r,
        Err(e) => return bad_gateway(&format!("upstream error: {e}")),
    };

    let (mut resp_parts, resp_body) = resp.into_parts();
    strip_hop_headers(&mut resp_parts.headers);

    // Важно: axum требует axum::body::Body в ответе; используем Body::new(...)
    Response::from_parts(resp_parts, Body::new(resp_body))
}

fn build_uri(base: &BaseUri, path_and_query: &str) -> Result<Uri, http::Error> {
    Uri::builder()
        .scheme(base.scheme.clone())
        .authority(base.authority.clone())
        .path_and_query(path_and_query)
        .build()
}

fn bad_gateway(msg: &str) -> Response<Body> {
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
