use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Request as AxumRequest, State};
use axum::routing;
use axum::{Extension, Router};

use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

use crate::backend::{BackendPool, BaseUri};
use crate::proxy::{Rewrite, RouteMeta, proxy_request};
use crate::{Method, Routes};

struct Inner {
    backends: HashMap<String, BackendPool>,
    client: Client<hyper_util::client::legacy::connect::HttpConnector, Body>,
    _default_timeout: Duration, // пока не используется (в будущих версиях)
}

pub struct App {
    router: Router,
}

pub struct AppBuilder {
    backends: HashMap<String, Vec<String>>,
    mounted: Vec<Routes>,
    default_timeout: Duration,
}

impl AppBuilder {
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
            mounted: Vec::new(),
            default_timeout: Duration::from_secs(30),
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

    pub fn default_timeout(mut self, d: Duration) -> Self {
        self.default_timeout = d;
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
        });

        // build router with state
        let mut router = Router::new();

        for svc_routes in self.mounted {
            router = mount_service(router, svc_routes)?;
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
    // axum::serve intentionally simple (и это нам подходит как внутренняя обертка) :contentReference[oaicite:6]{index=6}
    axum::serve(listener, app.router).await
}

fn mount_service(
    mut router: Router<Arc<Inner>>,
    routes: Routes,
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

        let meta = RouteMeta {
            service: routes.service,
            prefix: routes.prefix,
            rewrite: match rd.to {
                None => Rewrite::StripPrefix,
                Some(to) => Rewrite::Fixed(to),
            },
        };

        let method_router = method_router(rd.method).layer(Extension(meta));

        router = router.route(&full_path, method_router);
    }

    Ok(router)
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
    req: AxumRequest,
) -> axum::response::Response {
    let pool = match inner.backends.get(meta.service) {
        Some(p) => p,
        None => {
            return http::Response::builder()
                .status(http::StatusCode::BAD_GATEWAY)
                .body(Body::from(format!("unknown backend `{}`", meta.service)))
                .unwrap(); // TODO: Remove unwrap
        }
    };

    proxy_request(pool, &inner.client, &meta, req).await
}
