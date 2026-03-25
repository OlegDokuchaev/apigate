use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{Request as AxumRequest, State};
use axum::response::IntoResponse;
use axum::routing;
use axum::{Extension, Router};

use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

use crate::backend::{BackendPool, BaseUri};
use crate::balancing::{BalanceCtx, ProxyErrorKind, ResultEvent, RoundRobin, StartEvent};
use crate::policy::Policy;
use crate::proxy::{bad_gateway, proxy_request};
use crate::route::{FixedRewrite, Rewrite, RouteMeta};
use crate::routing::{NoRouteKey, RouteCtx};
use crate::{Method, PartsCtx, Routes};

struct Inner {
    backends: HashMap<String, BackendPool>,
    client: Client<hyper_util::client::legacy::connect::HttpConnector, Body>,
    map_body_limit: usize,
    _default_timeout: Duration, // пока не используется
}

pub struct App {
    router: Router,
}

pub struct AppBuilder {
    backends: HashMap<String, Vec<String>>,
    mounted: Vec<Routes>,
    policies: HashMap<String, Arc<Policy>>,
    default_policy: Arc<Policy>,
    default_timeout: Duration,
    map_body_limit: usize,
}

impl AppBuilder {
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
            mounted: Vec::new(),
            policies: HashMap::new(),
            default_policy: Arc::new(Policy::new().router(NoRouteKey).balancer(RoundRobin::new())),
            default_timeout: Duration::from_secs(30),
            map_body_limit: 2 * 1024 * 1024,
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

    pub fn default_timeout(mut self, d: Duration) -> Self {
        self.default_timeout = d;
        self
    }

    pub fn map_body_limit(mut self, bytes: usize) -> Self {
        self.map_body_limit = bytes;
        self
    }

    pub fn mount(mut self, routes: Routes) -> Self {
        self.mounted.push(routes);
        self
    }

    pub fn build(self) -> Result<App, String> {
        // client (pooling внутри hyper-util)
        let client = Client::builder(TokioExecutor::new()).build_http();

        // backend pools
        let mut pools = HashMap::with_capacity(self.backends.len());
        for (svc, urls) in self.backends {
            let bases = urls
                .iter()
                .map(|u| BaseUri::parse(u))
                .collect::<Result<Vec<_>, _>>()?;
            pools.insert(svc, BackendPool::new(bases));
        }

        let inner = Arc::new(Inner {
            backends: pools,
            client,
            _default_timeout: self.default_timeout,
            map_body_limit: self.map_body_limit,
        });

        // build router with state
        let mut router = Router::new();

        for svc_routes in self.mounted {
            if !inner.backends.contains_key(svc_routes.service) {
                return Err(format!(
                    "backend for service `{}` is not registered",
                    svc_routes.service
                ));
            }

            router = mount_service(
                router,
                svc_routes,
                &self.policies,
                self.default_policy.clone(),
            )?;
        }

        let router = router.with_state(inner);

        Ok(App { router })
    }
}

impl App {
    pub fn builder() -> AppBuilder {
        AppBuilder::new()
    }
}

pub async fn run(addr: SocketAddr, app: App) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    // axum::serve intentionally simple (и это нам подходит как внутренняя обертка)
    axum::serve(listener, app.router).await
}

fn mount_service(
    mut router: Router<Arc<Inner>>,
    routes: Routes,
    policies: &HashMap<String, Arc<Policy>>,
    default_policy: Arc<Policy>,
) -> Result<Router<Arc<Inner>>, String> {
    // проверим что backend зарегистрирован
    // (в минимальной версии лучше фейлиться сразу)
    // В будущем тут будет ServiceRegistry/PolicyRegistry.
    //
    // NOTE: State<Inner> у нас хранит HashMap<String, BackendPool>.
    // Мы проверяем наличие ключа на запросе в handler'е тоже, но ранняя проверка приятнее.
    // Т.к. mounted routes имеют &'static str service, мы делаем check на build-time.
    //
    // Здесь нет доступа к Inner.backends (он внутри Arc), поэтому check сделаем в handler'е.
    // (Можно перестроить на builder-time, но это минимальная версия.)

    for rd in routes.routes {
        let full_path = join(routes.prefix, rd.path);

        let policy = resolve_policy(routes.policy, rd.policy, policies, &default_policy)?;

        let meta = RouteMeta {
            service: routes.service,
            route_path: rd.path,
            prefix: routes.prefix,
            rewrite: match rd.to {
                None => Rewrite::StripPrefix,
                Some(to) => Rewrite::Fixed(FixedRewrite::new(to)),
            },
            policy,
            before: rd.before,
            map: rd.map,
        };

        let method_router = method_router(rd.method).layer(Extension(meta));

        router = router.route(&full_path, method_router);
    }

    Ok(router)
}

fn resolve_policy(
    service_policy: Option<&'static str>,
    route_policy: Option<&'static str>,
    registry: &HashMap<String, Arc<Policy>>,
    default_policy: &Arc<Policy>,
) -> Result<Arc<Policy>, String> {
    let effective = route_policy.or(service_policy);

    match effective {
        Some(name) => registry
            .get(name)
            .cloned()
            .ok_or_else(|| format!("policy `{name}` is not registered")),
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

async fn proxy_handler(
    State(inner): State<Arc<Inner>>,
    Extension(meta): Extension<RouteMeta>,
    mut req: AxumRequest,
) -> axum::response::Response {
    let Some(pool) = inner.backends.get(meta.service) else {
        return bad_gateway(format!("unknown backend `{}`", meta.service));
    };

    // Before hook
    if let Some(before) = meta.before {
        let (mut parts, body) = req.into_parts();
        let ctx = PartsCtx::new(meta.service, meta.route_path, &mut parts);

        if let Err(err) = before(ctx).await {
            return err.into_response();
        }

        req = http::Request::from_parts(parts, body);
    }

    // Map
    if let Some(map) = meta.map {
        let (mut parts, body) = req.into_parts();
        let ctx = PartsCtx::new(meta.service, meta.route_path, &mut parts);

        let body = match map(ctx, body, inner.map_body_limit).await {
            Ok(body) => body,
            Err(err) => return err.into_response(),
        };

        req = http::Request::from_parts(parts, body);
    }

    // Routing
    let route_ctx = RouteCtx {
        service: meta.service,
        route_path: meta.route_path,
        method: req.method(),
        uri: req.uri(),
        headers: req.headers(),
    };
    let routing = meta.policy.router.route(&route_ctx, pool);

    // Balancer
    let balance_ctx = BalanceCtx {
        service: meta.service,
        affinity: routing.affinity.as_ref(),
        pool,
        candidates: routing.candidates,
    };
    let Some(backend_index) = meta.policy.balancer.pick(&balance_ctx) else {
        return bad_gateway("no backends selected by balancer");
    };
    let Some(backend) = pool.get(backend_index) else {
        return bad_gateway("balancer returned invalid backend index");
    };

    // Make request
    meta.policy.balancer.on_start(&StartEvent {
        service: meta.service,
        backend_index,
    });

    let started_at = Instant::now();

    match proxy_request(backend, &inner.client, &meta, req).await {
        Ok(response) => {
            let elapsed = started_at.elapsed();

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

            meta.policy.balancer.on_result(&ResultEvent {
                service: meta.service,
                backend_index,
                status: None,
                error: Some(error),
                head_latency: elapsed,
            });

            match error {
                ProxyErrorKind::NoBackends => bad_gateway("no backends"),
                ProxyErrorKind::InvalidUpstreamUri => bad_gateway("bad upstream uri"),
                ProxyErrorKind::UpstreamRequest => bad_gateway("upstream request failed"),
            }
        }
    }
}
