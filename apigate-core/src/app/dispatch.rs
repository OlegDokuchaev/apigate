use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Extension;
use axum::body::Body;
use axum::extract::{Request as AxumRequest, State};

use super::Inner;
use crate::backend::Backend;
use crate::balancing::{BalanceCtx, ProxyErrorKind, ResultEvent, StartEvent};
use crate::error::{ApigateCoreError, ApigateFrameworkError};
use crate::observability::{RuntimeEvent, RuntimeEventKind};
use crate::proxy::proxy_request;
use crate::route::RouteMeta;
use crate::routing::RouteCtx;
use crate::{ApigateError, PartsCtx, RequestScope};

pub(super) async fn proxy_handler(
    State(inner): State<Arc<Inner>>,
    Extension(route_idx): Extension<usize>,
    req: AxumRequest,
) -> axum::response::Response {
    let meta = &inner.route_metas[route_idx];
    let (mut parts, body) = req.into_parts();

    emit_request_start(inner.as_ref(), meta, &parts);

    let body = match run_pipeline_if_needed(inner.as_ref(), meta, &mut parts, body).await {
        Ok(body) => body,
        Err(err) => {
            emit_pipeline_failure(inner.as_ref(), meta, &err);
            return err.into_response_with(inner.error_renderer.as_ref());
        }
    };

    let (backend_index, backend) = match select_backend(meta, &parts) {
        Ok(selected) => selected,
        Err(core_error) => {
            let framework_error = ApigateFrameworkError::from(core_error);
            emit_dispatch_failure(inner.as_ref(), meta, &framework_error);
            return ApigateError::from(framework_error)
                .into_response_with(inner.error_renderer.as_ref());
        }
    };

    emit_backend_selected(inner.as_ref(), meta, backend_index);

    proxy_to_backend(inner.as_ref(), meta, backend_index, backend, parts, body).await
}

async fn run_pipeline_if_needed(
    inner: &Inner,
    meta: &RouteMeta,
    parts: &mut http::request::Parts,
    body: Body,
) -> Result<Body, ApigateError> {
    let Some(pipeline) = meta.pipeline else {
        return Ok(body);
    };

    let ctx = PartsCtx::new(meta.service, meta.route_path, parts);
    let scope = RequestScope::new(&inner.state, body, inner.map_body_limit);
    pipeline(ctx, scope).await
}

fn select_backend<'a>(
    meta: &'a RouteMeta,
    parts: &http::request::Parts,
) -> Result<(usize, &'a Backend), ApigateCoreError> {
    let pool = &meta.pool;

    let route_ctx = RouteCtx {
        service: meta.service,
        prefix: meta.prefix,
        route_path: meta.route_path,
        method: &parts.method,
        uri: &parts.uri,
        headers: &parts.headers,
    };
    let routing = meta.policy.router.route(&route_ctx, pool);

    let balance_ctx = BalanceCtx {
        service: meta.service,
        affinity: routing.affinity.as_ref(),
        pool,
        candidates: routing.candidates,
    };
    let backend_index = meta
        .policy
        .balancer
        .pick(&balance_ctx)
        .ok_or(ApigateCoreError::NoBackendsSelectedByBalancer)?;
    let backend = pool
        .get(backend_index)
        .ok_or(ApigateCoreError::InvalidBackendIndex)?;

    Ok((backend_index, backend))
}

async fn proxy_to_backend(
    inner: &Inner,
    meta: &RouteMeta,
    backend_index: usize,
    backend: &Backend,
    parts: http::request::Parts,
    body: Body,
) -> axum::response::Response {
    meta.policy.balancer.on_start(&StartEvent {
        service: meta.service,
        backend_index,
    });

    let started_at = Instant::now();
    let result = tokio::time::timeout(
        inner.request_timeout,
        proxy_request(backend, &inner.client, meta, parts, body),
    )
    .await
    .unwrap_or_else(|_| Err(ProxyErrorKind::Timeout));

    match result {
        Ok(response) => {
            let elapsed = started_at.elapsed();

            emit_upstream_succeeded(inner, meta, backend_index, response.status(), elapsed);
            meta.policy.balancer.on_result(&ResultEvent {
                service: meta.service,
                backend_index,
                status: Some(response.status()),
                error: None,
                head_latency: elapsed,
            });

            response
        }
        Err(error) => {
            let elapsed = started_at.elapsed();

            emit_upstream_failed(inner, meta, backend_index, error, elapsed);
            meta.policy.balancer.on_result(&ResultEvent {
                service: meta.service,
                backend_index,
                status: None,
                error: Some(error),
                head_latency: elapsed,
            });

            ApigateError::from(map_proxy_error_to_framework(error))
                .into_response_with(inner.error_renderer.as_ref())
        }
    }
}

#[inline]
fn map_proxy_error_to_framework(error: ProxyErrorKind) -> ApigateFrameworkError {
    match error {
        ProxyErrorKind::NoBackends => ApigateFrameworkError::from(ApigateCoreError::NoBackends),
        ProxyErrorKind::InvalidUpstreamUri => {
            ApigateFrameworkError::from(ApigateCoreError::InvalidUpstreamUri)
        }
        ProxyErrorKind::UpstreamRequest => {
            ApigateFrameworkError::from(ApigateCoreError::UpstreamRequestFailed)
        }
        ProxyErrorKind::Timeout => {
            ApigateFrameworkError::from(ApigateCoreError::UpstreamRequestTimedOut)
        }
    }
}

#[inline]
fn emit_request_start(inner: &Inner, meta: &RouteMeta, parts: &http::request::Parts) {
    emit_runtime_event(inner, meta, || RuntimeEventKind::RequestStart {
        method: &parts.method,
        uri: &parts.uri,
        has_pipeline: meta.pipeline.is_some(),
    });
}

#[inline]
fn emit_pipeline_failure(inner: &Inner, meta: &RouteMeta, err: &ApigateError) {
    if let Some(framework_error) = err.framework_error() {
        emit_runtime_event(inner, meta, || RuntimeEventKind::PipelineFailedFramework {
            error: framework_error,
        });
    } else {
        emit_runtime_event(inner, meta, || RuntimeEventKind::PipelineFailedCustom {
            status: err.status_code_for_log(),
        });
    }
}

#[inline]
fn emit_dispatch_failure(inner: &Inner, meta: &RouteMeta, error: &ApigateFrameworkError) {
    emit_runtime_event(inner, meta, || RuntimeEventKind::DispatchFailed { error });
}

#[inline]
fn emit_backend_selected(inner: &Inner, meta: &RouteMeta, backend_index: usize) {
    emit_runtime_event(inner, meta, || RuntimeEventKind::BackendSelected {
        backend_index,
    });
}

#[inline]
fn emit_upstream_succeeded(
    inner: &Inner,
    meta: &RouteMeta,
    backend_index: usize,
    status: http::StatusCode,
    upstream_latency: Duration,
) {
    emit_runtime_event(inner, meta, || RuntimeEventKind::UpstreamSucceeded {
        backend_index,
        status,
        upstream_latency,
    });
}

#[inline]
fn emit_upstream_failed(
    inner: &Inner,
    meta: &RouteMeta,
    backend_index: usize,
    error: ProxyErrorKind,
    upstream_latency: Duration,
) {
    emit_runtime_event(inner, meta, || RuntimeEventKind::UpstreamFailed {
        backend_index,
        error,
        upstream_latency,
    });
}

#[inline]
fn emit_runtime_event<'a>(
    inner: &Inner,
    meta: &RouteMeta,
    make_kind: impl FnOnce() -> RuntimeEventKind<'a>,
) {
    let Some(observer) = inner.runtime_observer.as_ref() else {
        return;
    };

    observer(RuntimeEvent {
        service: meta.service,
        route_path: meta.route_path,
        kind: make_kind(),
    });
}
