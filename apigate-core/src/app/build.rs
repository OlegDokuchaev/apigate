use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::routing;
use axum::{Extension, Router};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioTimer};

use super::dispatch::proxy_handler;
use super::{App, AppBuilder, Inner};
use crate::backend::{BackendPool, BaseUri};
use crate::balancing::RoundRobin;
use crate::error::{ApigateBuildError, ApigateFrameworkError, default_error_renderer};
use crate::observability::{RuntimeEvent, default_tracing_observer};
use crate::policy::Policy;
use crate::route::{FixedRewrite, Rewrite, RewriteSpec, RouteMeta};
use crate::routing::NoRouteKey;
use crate::{Method, Routes};

impl AppBuilder {
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
            mounted: Vec::new(),
            policies: HashMap::new(),
            default_policy: Arc::new(Policy::new().router(NoRouteKey).balancer(RoundRobin::new())),
            request_timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(5),
            pool_idle_timeout: Duration::from_secs(90),
            map_body_limit: 2 * 1024 * 1024,
            state: http::Extensions::new(),
            error_renderer: Arc::new(default_error_renderer),
            runtime_observer: None,
        }
    }

    pub fn backend<I, S>(mut self, service: &str, urls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.backends.insert(
            service.to_string(),
            urls.into_iter().map(|s| s.into()).collect(),
        );
        self
    }

    pub fn policy(mut self, name: &str, policy: Policy) -> Self {
        self.policies.insert(name.to_string(), Arc::new(policy));
        self
    }

    pub fn default_policy(mut self, policy: Policy) -> Self {
        self.default_policy = Arc::new(policy);
        self
    }

    pub fn request_timeout(mut self, d: Duration) -> Self {
        self.request_timeout = d;
        self
    }

    pub fn connect_timeout(mut self, d: Duration) -> Self {
        self.connect_timeout = d;
        self
    }

    pub fn pool_idle_timeout(mut self, d: Duration) -> Self {
        self.pool_idle_timeout = d;
        self
    }

    pub fn map_body_limit(mut self, bytes: usize) -> Self {
        self.map_body_limit = bytes;
        self
    }

    pub fn state<T: Clone + Send + Sync + 'static>(mut self, val: T) -> Self {
        self.state.insert(val);
        self
    }

    /// Sets the renderer for framework-generated errors (`ApigateError::*` constructors).
    ///
    /// This lets applications return a uniform JSON error envelope instead of plain text.
    /// The renderer is used both for pipeline errors and proxy/runtime errors (502/504, etc.).
    pub fn error_renderer<F>(mut self, renderer: F) -> Self
    where
        F: Fn(ApigateFrameworkError) -> axum::response::Response + Send + Sync + 'static,
    {
        self.error_renderer = Arc::new(renderer);
        self
    }

    /// Enables built-in structured runtime events through `tracing`.
    pub fn enable_default_tracing(mut self) -> Self {
        self.runtime_observer = Some(Arc::new(default_tracing_observer));
        self
    }

    /// Disables runtime observer events.
    ///
    /// This is the lowest-overhead mode: request handling only performs a cheap
    /// `Option::is_some` check and does not call an observer.
    pub fn disable_runtime_observer(mut self) -> Self {
        self.runtime_observer = None;
        self
    }

    /// Sets observer for structured runtime events.
    pub fn runtime_observer<F>(mut self, observer: F) -> Self
    where
        F: for<'a> Fn(RuntimeEvent<'a>) + Send + Sync + 'static,
    {
        self.runtime_observer = Some(Arc::new(observer));
        self
    }

    pub fn mount(mut self, routes: Routes) -> Self {
        self.mounted.push(routes);
        self
    }

    /// Registers backend URLs for `routes.service` and mounts these routes.
    ///
    /// Equivalent to:
    /// `builder.backend(routes.service, urls).mount(routes)`
    pub fn mount_service<I, S>(mut self, routes: Routes, urls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.backends.insert(
            routes.service.to_string(),
            urls.into_iter().map(|s| s.into()).collect(),
        );
        self.mounted.push(routes);
        self
    }

    pub fn build(self) -> Result<App, ApigateBuildError> {
        // HTTP client
        let mut connector = HttpConnector::new();
        connector.set_nodelay(true);
        connector.set_connect_timeout(Some(self.connect_timeout));
        connector.set_keepalive(Some(self.pool_idle_timeout));

        let client = Client::builder(TokioExecutor::new())
            .pool_timer(TokioTimer::new())
            .pool_idle_timeout(self.pool_idle_timeout)
            .build(connector);

        // backend pools
        let mut pools: HashMap<String, Arc<BackendPool>> = HashMap::new();
        for (svc, urls) in self.backends {
            let mut bases = Vec::with_capacity(urls.len());
            for url in urls {
                let base = match BaseUri::parse(&url) {
                    Ok(base) => base,
                    Err(source) => {
                        return Err(ApigateBuildError::InvalidBackendUri {
                            service: svc.clone(),
                            url,
                            source,
                        });
                    }
                };
                bases.push(base);
            }
            pools.insert(svc, Arc::new(BackendPool::new(bases)));
        }

        // build router + route metadata table
        let mut router = Router::new();
        let mut route_metas = Vec::new();

        for svc_routes in self.mounted {
            let pool = pools
                .get(svc_routes.service)
                .ok_or(ApigateBuildError::BackendNotRegistered {
                    service: svc_routes.service,
                })?
                .clone();

            router = mount_routes(
                router,
                svc_routes,
                &self.policies,
                self.default_policy.clone(),
                pool,
                &mut route_metas,
            )?;
        }

        let inner = Arc::new(Inner {
            client,
            state: self.state,
            request_timeout: self.request_timeout,
            map_body_limit: self.map_body_limit,
            route_metas: route_metas.into_boxed_slice(),
            error_renderer: self.error_renderer,
            runtime_observer: self.runtime_observer,
        });

        let router = router.with_state(inner);

        Ok(App { router })
    }
}

fn mount_routes(
    mut router: Router<Arc<Inner>>,
    routes: Routes,
    policies: &HashMap<String, Arc<Policy>>,
    default_policy: Arc<Policy>,
    pool: Arc<BackendPool>,
    route_metas: &mut Vec<RouteMeta>,
) -> Result<Router<Arc<Inner>>, ApigateBuildError> {
    for rd in routes.routes {
        let full_path = join(routes.prefix, rd.path);
        let policy = resolve_policy(routes.policy, rd.policy, policies, &default_policy)?;

        let meta = RouteMeta {
            service: routes.service,
            route_path: rd.path,
            prefix: routes.prefix,
            rewrite: match rd.rewrite {
                RewriteSpec::StripPrefix => Rewrite::StripPrefix,
                RewriteSpec::Static(to) => Rewrite::Static(FixedRewrite::new(to)),
                RewriteSpec::Template(tpl) => Rewrite::Template(tpl),
            },
            pool: Arc::clone(&pool),
            policy,
            pipeline: rd.pipeline,
        };

        let route_idx = route_metas.len();
        route_metas.push(meta);

        let method_router = method_router(rd.method).layer(Extension(route_idx));

        router = router.route(&full_path, method_router);
    }

    Ok(router)
}

fn resolve_policy(
    service_policy: Option<&'static str>,
    route_policy: Option<&'static str>,
    registry: &HashMap<String, Arc<Policy>>,
    default_policy: &Arc<Policy>,
) -> Result<Arc<Policy>, ApigateBuildError> {
    let effective = route_policy.or(service_policy);

    match effective {
        Some(name) => registry
            .get(name)
            .cloned()
            .ok_or(ApigateBuildError::PolicyNotRegistered { name }),
        None => Ok(default_policy.clone()),
    }
}

fn join(prefix: &str, path: &str) -> String {
    // prefix="/sales", path="/ping" => "/sales/ping"
    // prefix="/sales", path="/" => "/sales/"
    let mut s = String::with_capacity(prefix.len() + path.len());
    if prefix.ends_with('/') {
        s.push_str(prefix.trim_end_matches('/'));
    } else {
        s.push_str(prefix);
    }
    if path.starts_with('/') {
        s.push_str(path);
    } else {
        s.push('/');
        s.push_str(path);
    }
    s
}

fn method_router(method: Method) -> routing::MethodRouter<Arc<Inner>> {
    match method {
        Method::Get => routing::get(proxy_handler),
        Method::Post => routing::post(proxy_handler),
        Method::Put => routing::put(proxy_handler),
        Method::Delete => routing::delete(proxy_handler),
        Method::Patch => routing::patch(proxy_handler),
        Method::Head => routing::head(proxy_handler),
        Method::Options => routing::options(proxy_handler),
    }
}
