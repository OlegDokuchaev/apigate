use std::time::Duration;

use http::{Method, StatusCode, Uri};

use crate::balancing::ProxyErrorKind;
use crate::error::ApigateFrameworkError;

/// Structured event emitted by the gateway runtime.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct RuntimeEvent<'a> {
    pub service: &'static str,
    pub route_path: &'static str,
    pub kind: RuntimeEventKind<'a>,
}

/// Runtime event kinds.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub enum RuntimeEventKind<'a> {
    RequestStart {
        method: &'a Method,
        uri: &'a Uri,
        has_pipeline: bool,
    },
    PipelineFailedFramework {
        error: &'a ApigateFrameworkError,
    },
    PipelineFailedCustom {
        status: StatusCode,
    },
    DispatchFailed {
        error: &'a ApigateFrameworkError,
    },
    BackendSelected {
        backend_index: usize,
    },
    UpstreamSucceeded {
        backend_index: usize,
        status: StatusCode,
        upstream_latency: Duration,
    },
    UpstreamFailed {
        backend_index: usize,
        error: ProxyErrorKind,
        upstream_latency: Duration,
    },
}

/// Custom observer for runtime events.
pub type RuntimeObserver = dyn for<'a> Fn(RuntimeEvent<'a>) + Send + Sync + 'static;

/// Default observer: emits structured events through `tracing`.
pub fn default_tracing_observer(event: RuntimeEvent) {
    match event.kind {
        RuntimeEventKind::RequestStart {
            method,
            uri,
            has_pipeline,
        } => {
            if !tracing::enabled!(target: "apigate::request", tracing::Level::DEBUG) {
                return;
            }

            tracing::debug!(
                target: "apigate::request",
                service = event.service,
                route = event.route_path,
                method = %method,
                path = uri.path(),
                has_query = uri.query().is_some(),
                has_pipeline,
                "request started"
            );
        }
        RuntimeEventKind::PipelineFailedFramework { error } => {
            log_pipeline_framework_failure(event, error);
            log_pipeline_debug_details(event, error);
        }
        RuntimeEventKind::PipelineFailedCustom { status } => {
            log_custom_pipeline_failure(event, status);
        }
        RuntimeEventKind::DispatchFailed { error } => {
            log_dispatch_framework_failure(event, error);
            log_dispatch_debug_details(event, error);
        }
        RuntimeEventKind::BackendSelected { backend_index } => {
            if !tracing::enabled!(target: "apigate::balancer", tracing::Level::DEBUG) {
                return;
            }

            tracing::debug!(
                target: "apigate::balancer",
                service = event.service,
                route = event.route_path,
                backend_index,
                "backend selected"
            );
        }
        RuntimeEventKind::UpstreamSucceeded {
            backend_index,
            status,
            upstream_latency,
        } => {
            if !tracing::enabled!(target: "apigate::proxy", tracing::Level::DEBUG) {
                return;
            }

            tracing::debug!(
                target: "apigate::proxy",
                service = event.service,
                route = event.route_path,
                backend_index,
                status = status.as_u16(),
                upstream_latency = ?upstream_latency,
                "upstream request succeeded"
            );
        }
        RuntimeEventKind::UpstreamFailed {
            backend_index,
            error,
            upstream_latency,
        } => {
            tracing::warn!(
                target: "apigate::proxy",
                service = event.service,
                route = event.route_path,
                backend_index,
                error = ?error,
                upstream_latency = ?upstream_latency,
                "upstream request failed"
            );
        }
    }
}

fn log_pipeline_framework_failure(event: RuntimeEvent, error: &ApigateFrameworkError) {
    let status = error.status_code();

    if status.is_server_error() {
        tracing::warn!(
            target: "apigate::pipeline",
            service = event.service,
            route = event.route_path,
            code = error.code(),
            status = status.as_u16(),
            message = error.user_message(),
            "pipeline failed"
        );
    } else {
        tracing::info!(
            target: "apigate::pipeline",
            service = event.service,
            route = event.route_path,
            code = error.code(),
            status = status.as_u16(),
            message = error.user_message(),
            "pipeline failed"
        );
    }
}

fn log_dispatch_framework_failure(event: RuntimeEvent, error: &ApigateFrameworkError) {
    let status = error.status_code();

    if status.is_server_error() {
        tracing::warn!(
            target: "apigate::dispatch",
            service = event.service,
            route = event.route_path,
            code = error.code(),
            status = status.as_u16(),
            message = error.user_message(),
            "request dispatch failed"
        );
    } else {
        tracing::info!(
            target: "apigate::dispatch",
            service = event.service,
            route = event.route_path,
            code = error.code(),
            status = status.as_u16(),
            message = error.user_message(),
            "request dispatch failed"
        );
    }
}

fn log_pipeline_debug_details(event: RuntimeEvent, error: &ApigateFrameworkError) {
    if !tracing::enabled!(target: "apigate::pipeline", tracing::Level::DEBUG) {
        return;
    }

    let Some(details) = error.debug_details() else {
        return;
    };

    tracing::debug!(
        target: "apigate::pipeline",
        service = event.service,
        route = event.route_path,
        code = error.code(),
        debug_details = details,
        "pipeline debug details"
    );
}

fn log_dispatch_debug_details(event: RuntimeEvent, error: &ApigateFrameworkError) {
    if !tracing::enabled!(target: "apigate::dispatch", tracing::Level::DEBUG) {
        return;
    }

    let Some(details) = error.debug_details() else {
        return;
    };

    tracing::debug!(
        target: "apigate::dispatch",
        service = event.service,
        route = event.route_path,
        code = error.code(),
        debug_details = details,
        "dispatch debug details"
    );
}

fn log_custom_pipeline_failure(event: RuntimeEvent, status: StatusCode) {
    if status.is_server_error() {
        tracing::warn!(
            target: "apigate::pipeline",
            service = event.service,
            route = event.route_path,
            status = status.as_u16(),
            "pipeline returned custom error response"
        );
    } else {
        tracing::info!(
            target: "apigate::pipeline",
            service = event.service,
            route = event.route_path,
            status = status.as_u16(),
            "pipeline returned custom error response"
        );
    }
}
