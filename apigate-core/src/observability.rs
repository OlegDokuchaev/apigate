use std::time::Duration;

use http::{Method, StatusCode, Uri};

use crate::balancing::ProxyErrorKind;
use crate::error::ApigateFrameworkError;

/// Structured event emitted by the gateway runtime.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct RuntimeEvent<'a> {
    /// Logical service name.
    pub service: &'static str,
    /// Route path relative to the service prefix.
    pub route_path: &'static str,
    /// Event-specific payload.
    pub kind: RuntimeEventKind<'a>,
}

/// Runtime event kinds.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub enum RuntimeEventKind<'a> {
    /// Request entered the gateway handler.
    RequestStart {
        /// Incoming request method.
        method: &'a Method,
        /// Incoming request URI.
        uri: &'a Uri,
        /// Whether this route has a generated pipeline.
        has_pipeline: bool,
    },
    /// Generated pipeline failed with a framework-rendered error.
    PipelineFailedFramework {
        /// Framework error returned by the pipeline.
        error: &'a ApigateFrameworkError,
    },
    /// Generated pipeline returned a custom HTTP response.
    PipelineFailedCustom {
        /// Response status.
        status: StatusCode,
    },
    /// Backend selection or request dispatch failed before upstream I/O.
    DispatchFailed {
        /// Framework error describing the dispatch failure.
        error: &'a ApigateFrameworkError,
    },
    /// A backend was selected for the request.
    BackendSelected {
        /// Selected backend index.
        backend_index: usize,
    },
    /// Upstream returned a response.
    UpstreamSucceeded {
        /// Selected backend index.
        backend_index: usize,
        /// Upstream response status.
        status: StatusCode,
        /// Time from upstream dispatch to response head.
        upstream_latency: Duration,
    },
    /// Upstream request failed before receiving a response.
    UpstreamFailed {
        /// Selected backend index.
        backend_index: usize,
        /// Proxy error kind.
        error: ProxyErrorKind,
        /// Time from upstream dispatch to failure.
        upstream_latency: Duration,
    },
}

/// Custom observer for runtime events.
pub type RuntimeObserver = dyn for<'a> Fn(RuntimeEvent<'a>) + Send + Sync + 'static;

/// Default observer: emits structured events through `tracing`.
pub fn default_tracing_observer(event: RuntimeEvent<'_>) {
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

fn log_pipeline_framework_failure(event: RuntimeEvent<'_>, error: &ApigateFrameworkError) {
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

fn log_dispatch_framework_failure(event: RuntimeEvent<'_>, error: &ApigateFrameworkError) {
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

fn log_pipeline_debug_details(event: RuntimeEvent<'_>, error: &ApigateFrameworkError) {
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

fn log_dispatch_debug_details(event: RuntimeEvent<'_>, error: &ApigateFrameworkError) {
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

fn log_custom_pipeline_failure(event: RuntimeEvent<'_>, status: StatusCode) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{ApigateCoreError, ApigatePipelineError};

    fn event(kind: RuntimeEventKind<'_>) -> RuntimeEvent<'_> {
        RuntimeEvent {
            service: "sales",
            route_path: "/items/{id}",
            kind,
        }
    }

    #[test]
    fn default_tracing_observer_accepts_all_runtime_event_kinds() {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_writer(std::io::sink)
            .finish();

        let method = Method::GET;
        let uri: Uri = "/sales/items/42?verbose=true".parse().unwrap();
        let pipeline_client_error = ApigateFrameworkError::from(
            ApigatePipelineError::InvalidJsonBody("expected string".to_string()),
        );
        let pipeline_server_error = ApigateFrameworkError::from(
            ApigatePipelineError::FailedSerializeMappedJson("missing field".to_string()),
        );
        let dispatch_client_error =
            ApigateFrameworkError::from(ApigateCoreError::InvalidHeaderName);
        let dispatch_server_error = ApigateFrameworkError::from(ApigateCoreError::NoBackends);

        tracing::subscriber::with_default(subscriber, || {
            default_tracing_observer(event(RuntimeEventKind::RequestStart {
                method: &method,
                uri: &uri,
                has_pipeline: true,
            }));
            default_tracing_observer(event(RuntimeEventKind::PipelineFailedFramework {
                error: &pipeline_client_error,
            }));
            default_tracing_observer(event(RuntimeEventKind::PipelineFailedFramework {
                error: &pipeline_server_error,
            }));
            default_tracing_observer(event(RuntimeEventKind::PipelineFailedCustom {
                status: StatusCode::BAD_REQUEST,
            }));
            default_tracing_observer(event(RuntimeEventKind::PipelineFailedCustom {
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }));
            default_tracing_observer(event(RuntimeEventKind::DispatchFailed {
                error: &dispatch_client_error,
            }));
            default_tracing_observer(event(RuntimeEventKind::DispatchFailed {
                error: &dispatch_server_error,
            }));
            default_tracing_observer(event(RuntimeEventKind::BackendSelected {
                backend_index: 1,
            }));
            default_tracing_observer(event(RuntimeEventKind::UpstreamSucceeded {
                backend_index: 1,
                status: StatusCode::OK,
                upstream_latency: Duration::from_millis(7),
            }));
            default_tracing_observer(event(RuntimeEventKind::UpstreamFailed {
                backend_index: 1,
                error: ProxyErrorKind::Timeout,
                upstream_latency: Duration::from_millis(9),
            }));
        });
    }

    #[test]
    fn default_tracing_observer_keeps_debug_only_events_noop_without_subscriber() {
        let method = Method::POST;
        let uri: Uri = "/sales/items".parse().unwrap();

        default_tracing_observer(event(RuntimeEventKind::RequestStart {
            method: &method,
            uri: &uri,
            has_pipeline: false,
        }));
        default_tracing_observer(event(RuntimeEventKind::BackendSelected {
            backend_index: 0,
        }));
        default_tracing_observer(event(RuntimeEventKind::UpstreamSucceeded {
            backend_index: 0,
            status: StatusCode::CREATED,
            upstream_latency: Duration::from_millis(1),
        }));
    }
}
