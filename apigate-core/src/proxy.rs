use std::ops::Range;

use axum::body::Body;
use http::header::{
    CONNECTION, HOST, PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, TE, TRAILER, TRANSFER_ENCODING,
    UPGRADE,
};
use http::request::Parts;
use http::uri::{PathAndQuery, Uri};
use http::{HeaderMap, HeaderName, Request, Response};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use smallvec::SmallVec;

use crate::backend::{Backend, BaseUri};
use crate::balancing::ProxyErrorKind;
use crate::route::{DstChunk, Rewrite, RewriteTemplate, RouteMeta, SrcSeg};

// ---------------------------------------------------------------------------
// Proxy
// ---------------------------------------------------------------------------

/// Proxies an incoming request to an upstream backend:
/// rewrite URI → strip hop-by-hop headers → set Host → forward → strip response hops.
pub async fn proxy_request(
    backend: &Backend,
    client: &Client<HttpConnector, Body>,
    meta: &RouteMeta,
    mut parts: Parts,
    body: Body,
) -> Result<Response<Body>, ProxyErrorKind> {
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

// ---------------------------------------------------------------------------
// URI rewrite
// ---------------------------------------------------------------------------

/// Replaces scheme + authority with the upstream's and applies the rewrite
/// strategy to produce a new path_and_query.
#[inline]
fn rewrite_uri(base: &BaseUri, meta: &RouteMeta, uri: Uri) -> Result<Uri, ProxyErrorKind> {
    let new_pq = rewrite_path_and_query(meta, &uri)?;

    let mut parts = uri.into_parts();
    parts.scheme = Some(base.scheme.clone());
    parts.authority = Some(base.authority.clone());

    if let Some(pq) = new_pq {
        parts.path_and_query = Some(pq);
    }

    Uri::from_parts(parts).map_err(|_| ProxyErrorKind::InvalidUpstreamUri)
}

/// Computes a new path+query based on the rewrite strategy.
/// Returns `None` when the path should be kept as-is (prefix mismatch).
#[inline]
fn rewrite_path_and_query(
    meta: &RouteMeta,
    uri: &Uri,
) -> Result<Option<PathAndQuery>, ProxyErrorKind> {
    let query = uri.query();

    match &meta.rewrite {
        Rewrite::Static(fixed) => {
            if query.is_none() {
                Ok(Some(fixed.no_query().clone()))
            } else {
                Ok(Some(make_path_and_query(fixed.raw(), query)?))
            }
        }

        Rewrite::StripPrefix => {
            let stripped = strip_prefix(uri.path(), meta.prefix);

            if query.is_none() {
                Ok(Some(
                    PathAndQuery::try_from(stripped)
                        .map_err(|_| ProxyErrorKind::InvalidUpstreamUri)?,
                ))
            } else {
                Ok(Some(make_path_and_query(stripped, query)?))
            }
        }

        Rewrite::Template(tpl) => {
            let stripped = strip_prefix(uri.path(), meta.prefix);

            let rendered =
                render_template(tpl, stripped).ok_or(ProxyErrorKind::InvalidUpstreamUri)?;

            Ok(Some(make_path_and_query_owned(rendered, query)?))
        }
    }
}

/// Strips the service prefix from the incoming path.
#[inline]
fn strip_prefix<'a>(incoming_path: &'a str, prefix: &str) -> &'a str {
    match incoming_path.strip_prefix(prefix) {
        None => incoming_path,
        Some("") => "/",
        Some(rest) if rest.starts_with('/') => rest,
        Some(_) => "/",
    }
}

// ---------------------------------------------------------------------------
// Template rendering
// ---------------------------------------------------------------------------

/// Substitutes captured path parameters into the destination template.
#[inline]
fn render_template(tpl: &RewriteTemplate, stripped_path: &str) -> Option<String> {
    let captures = capture_raw(tpl.src, stripped_path)?;

    let extra: usize = captures.iter().map(|r| r.len()).sum();
    let mut out = String::with_capacity(tpl.static_len + extra);

    for chunk in tpl.dst {
        match chunk {
            DstChunk::Lit(s) => out.push_str(s),
            DstChunk::Capture { src_index } => {
                out.push_str(&stripped_path[captures[*src_index as usize].clone()]);
            }
        }
    }

    Some(out)
}

/// Single pass over the path: matches segments against the source pattern
/// and returns byte ranges of captured parameters within `stripped_path`.
#[inline]
fn capture_raw(src: &[SrcSeg], stripped_path: &str) -> Option<SmallVec<[Range<usize>; 8]>> {
    let mut captures = SmallVec::new();
    let content = stripped_path.strip_prefix('/')?;
    let mut remaining = content;

    for src_seg in src {
        if remaining.is_empty() {
            return None;
        }

        let (seg, rest) = match remaining.find('/') {
            Some(pos) => (&remaining[..pos], &remaining[pos + 1..]),
            None => (remaining, ""),
        };

        match src_seg {
            SrcSeg::Lit(expected) => {
                if seg != *expected {
                    return None;
                }
            }
            SrcSeg::Param => {
                let start = seg.as_ptr() as usize - stripped_path.as_ptr() as usize;
                captures.push(start..start + seg.len());
            }
        }

        remaining = rest;
    }

    if !remaining.is_empty() {
        return None;
    }

    Some(captures)
}

// ---------------------------------------------------------------------------
// PathAndQuery builders
// ---------------------------------------------------------------------------

/// Builds a `PathAndQuery` from a borrowed path and an optional query string.
#[inline]
fn make_path_and_query(path: &str, query: Option<&str>) -> Result<PathAndQuery, ProxyErrorKind> {
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

/// Builds a `PathAndQuery` from an owned path and an optional query string.
#[inline]
fn make_path_and_query_owned(
    mut path: String,
    query: Option<&str>,
) -> Result<PathAndQuery, ProxyErrorKind> {
    if let Some(q) = query {
        path.reserve(1 + q.len());
        path.push('?');
        path.push_str(q);
    }
    PathAndQuery::try_from(path).map_err(|_| ProxyErrorKind::InvalidUpstreamUri)
}

// ---------------------------------------------------------------------------
// Hop-by-hop headers
// ---------------------------------------------------------------------------

/// Removes hop-by-hop headers per RFC 7230 §6.1:
/// first those listed in `Connection`, then the standard set.
pub fn strip_hop_headers(headers: &mut HeaderMap) {
    // Collect Connection tokens before removing
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

    // Standard hop-by-hop (typed constants — pre-hashed, no string conversion)
    headers.remove(PROXY_AUTHENTICATE);
    headers.remove(PROXY_AUTHORIZATION);
    headers.remove(TE);
    headers.remove(TRAILER);
    headers.remove(TRANSFER_ENCODING);
    headers.remove(UPGRADE);
    headers.remove("keep-alive");
    headers.remove("proxy-connection");
}
