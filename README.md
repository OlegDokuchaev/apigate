# ApiGate

Типизированный API-шлюз (reverse proxy) для микросервисов на Rust.

Макросы генерируют маршруты с валидацией `Path / Query / Json / Form / Multipart`, хуками `before`, преобразованием `map` и runtime-политиками (балансировка, routing, таймауты). Внутри — `axum`, снаружи — только API `apigate`.

---

## Быстрый старт

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = AppConfig::from_env();

    let app = apigate::App::builder()
        .backend("sales", cfg.sales_backends)
        .backend("files", cfg.files_backends)
        .state(cfg.db_pool.clone())
        .state(cfg.auth_config.clone())
        .request_timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(3))
        .pool_idle_timeout(std::time::Duration::from_secs(60))
        .policy(
            "sales_default",
            apigate::Policy::new()
                .router(apigate::routing::HeaderSticky::new("x-user-id"))
                .balancer(apigate::balancing::ConsistentHash::new()),
        )
        .mount(sales::routes!())
        .mount(files::routes!())
        .build()?;

    apigate::run(cfg.listen, app).await
}
```

---

## Сервис

```rust
#[apigate::service(name = "sales", prefix = "/sales", policy = "sales_default")]
mod sales {
    use super::*;

    #[apigate::get("/ping")]
    async fn ping() {}

    #[apigate::get("/{id}", path = SaleIdPath, before = [auth])]
    async fn get_by_id() {}

    #[apigate::get("/public", to = "/internal")]
    async fn public_alias() {}

    #[apigate::post("/buy", json = BuyInput, before = [auth], map = remap_buy)]
    async fn buy() {}

    #[apigate::post("/upload", multipart, before = [auth])]
    async fn upload() {}
}
```

| Параметр `service` | Описание |
|---|---|
| `name` | Имя сервиса = ключ `.backend(...)` в main |
| `prefix` | Внешний URL-префикс |
| `policy` | Политика по умолчанию для всех маршрутов сервиса |

---

## Атрибуты маршрута

```rust
#[apigate::get("/path", to = "/rewrite", path = T, query = T, json = T, form = T,
               multipart, before = [hook1, hook2], map = map_fn, policy = "name")]
```

| Атрибут | Описание |
|---|---|
| `"/path"` | Внешний путь. Поддерживает `/{param}` |
| `to` | Путь в upstream. Без `to` — проксирует как есть (`StripPrefix`). Поддерживает `/{param}` |
| `path = T` | Десериализует и валидирует path-параметры (`T: Deserialize + Clone`). 400 при ошибке |
| `query = T` | Валидирует query string |
| `json = T` | Валидирует JSON body |
| `form = T` | Валидирует `application/x-www-form-urlencoded` body |
| `multipart` | Passthrough для `multipart/form-data` |
| `before = [...]` | Хуки, выполняемые до проксирования |
| `map = fn` | Преобразование `query/json/form` перед отправкой в upstream |
| `policy = "name"` | Переопределяет политику сервиса для этого маршрута |

> `json`, `form`, `multipart` — взаимоисключающие (один body-режим на маршрут).

---

## Входные данные

### Path

```rust
#[derive(Clone, serde::Deserialize)]
struct SaleIdPath { sale_id: uuid::Uuid }

#[apigate::get("/{sale_id}", path = SaleIdPath)]
async fn get_sale() {}
```

Извлекается **до** хуков, попадает в `RequestScope`. Доступен в хуках как `path: SaleIdPath` (owned) или `path: &SaleIdPath`.

### Query / Json / Form

```rust
#[apigate::get("/search", query = SearchQuery)]           // валидация query
#[apigate::post("/buy", json = BuyInput)]                  // валидация JSON
#[apigate::post("/legacy", form = LegacyForm)]             // валидация form
```

Без `map` — валидация + passthrough оригинального тела. С `map` — преобразование перед отправкой.

### Multipart

```rust
#[apigate::post("/upload", multipart)]
async fn upload() {}
```

Passthrough без чтения тела. `map` не поддерживается.

---

## Хуки (`before`)

Выполняются до проксирования. Работают с заголовками, URI, extensions.

```rust
#[apigate::hook]
async fn auth(ctx: &mut apigate::PartsCtx) -> apigate::HookResult {
    let token = ctx.header("authorization")
        .ok_or_else(|| apigate::ApigateError::unauthorized("missing token"))?;
    ctx.set_header("x-user-id", "...")?;
    Ok(())
}

#[apigate::get("/protected", before = [auth])]
async fn protected() {}
```

---

## Преобразование (`map`)

Типизированное преобразование `query/json/form` перед отправкой в upstream.

```rust
#[apigate::map]
async fn remap_buy(input: PublicBuy) -> apigate::MapResult<ServiceBuy> {
    Ok(ServiceBuy {
        ids: input.sale_ids,
        source: "apigate",
    })
}

#[apigate::post("/buy", json = PublicBuy, before = [auth], map = remap_buy)]
async fn buy() {}
```

Работает аналогично для `query = T, map = ...` и `form = T, map = ...`:
- **query**: map переписывает query string в URI
- **json**: map сериализует результат в новое тело
- **form**: map сериализует результат в URL-encoded тело

---

## Инъекция параметров в `hook` / `map`

Макрос анализирует типы параметров и генерирует код извлечения:

| Тип | Источник | Пример |
|---|---|---|
| `&mut PartsCtx` | Контекст запроса | `ctx: &mut PartsCtx` |
| `&mut RequestScope` | Прямой доступ к scope | `scope: &mut RequestScope` |
| `&T` | `scope.get::<T>()` — shared state / per-request данные | `config: &AuthConfig` |
| `&mut T` | `scope.get_mut::<T>()` — только из local | `state: &mut Counter` |
| `T` (owned в hook) | `scope.take::<T>()` — local, fallback clone из shared | `path: SaleIdPath` |
| `T` (первый owned в map) | Входные данные (json/query/form) | `input: PublicBuy` |

Все параметры опциональны.

**Ограничения:** `&mut PartsCtx` / `&mut RequestScope` — макс. по одному; `&mut T` — макс. один и нельзя совмещать с `&T`; `&mut RequestScope` нельзя совмещать с `&T` / `&mut T`.

---

## Таймауты

| Метод | Дефолт | Описание |
|---|---|---|
| `.request_timeout(Duration)` | 30s | Полное время upstream-запроса. 504 при превышении |
| `.connect_timeout(Duration)` | 5s | TCP handshake к backend'у |
| `.pool_idle_timeout(Duration)` | 90s | Время жизни idle-соединений в connection pool |

---

## Политики

Политика = routing (какие backend'ы) + balancing (какой конкретно). Дефолт: `NoRouteKey` + `RoundRobin`.

```rust
.policy("sticky_sales", apigate::Policy::new()
    .router(apigate::routing::HeaderSticky::new("x-user-id"))
    .balancer(apigate::balancing::ConsistentHash::new()))
```

Приоритет: атрибут маршрута > политика сервиса > дефолтная.

---

## Маршрутизация (routing)

Определяет набор кандидатов и опциональный affinity key для sticky sessions.

| Стратегия | Описание |
|---|---|
| `NoRouteKey` | Все backend'ы, без аффинности. **Дефолт** |
| `HeaderSticky::new("header")` | Affinity key из заголовка |
| `PathSticky::new("param")` | Affinity key из path-параметра `{param}` шаблона маршрута |

### Кастомная стратегия

```rust
use apigate::routing::{RouteStrategy, RouteCtx, RoutingDecision, AffinityKey, CandidateSet};

struct CookieSticky(&'static str);

impl RouteStrategy for CookieSticky {
    fn route<'a>(&self, ctx: &RouteCtx<'a>, _pool: &'a BackendPool) -> RoutingDecision<'a> {
        let affinity = ctx.headers.get("cookie")
            .and_then(|v| v.to_str().ok())
            .and_then(|c| c.split(';').map(str::trim)
                .find(|s| s.starts_with(self.0))
                .and_then(|s| s.split('=').nth(1)))
            .map(AffinityKey::borrowed);

        RoutingDecision { affinity, candidates: CandidateSet::All }
    }
}
```

**`RouteCtx`**: `service`, `prefix`, `route_path`, `method`, `uri`, `headers`.

**`RoutingDecision`**: `affinity: Option<AffinityKey>`, `candidates: CandidateSet` (`All` | `Indices(&[usize])`).

---

## Балансировка (balancing)

Выбирает конкретный backend из кандидатов.

| Стратегия | Описание |
|---|---|
| `RoundRobin::new()` | Циклический перебор. **Дефолт** |
| `ConsistentHash::new()` | Jump consistent hash по affinity key (xxh3). Без ключа — round-robin |
| `LeastRequest::new()` | Наименьшее число in-flight запросов |
| `LeastTime::new()` | Наименьшая EWMA-латентность |

Все балансировщики lock-free (атомарные операции).

### Кастомный балансировщик

```rust
use apigate::balancing::{Balancer, BalanceCtx, StartEvent, ResultEvent};

struct MyBalancer;

impl Balancer for MyBalancer {
    fn pick(&self, ctx: &BalanceCtx) -> Option<usize> {
        // ctx.candidate_len(), ctx.candidate_index(nth), ctx.affinity
        Some(ctx.candidate_index(0)?)
    }

    fn on_start(&self, _event: &StartEvent) {}        // опционально
    fn on_result(&self, _event: &ResultEvent) {}       // опционально
}
```

**`BalanceCtx`**: `service`, `affinity`, `pool`, `candidates`, `candidate_len()`, `candidate_index(nth)`, `candidate_backend(nth)`, `is_candidate(idx)`.

**`ResultEvent`**: `service`, `backend_index`, `status: Option<StatusCode>`, `error: Option<ProxyErrorKind>`, `head_latency: Duration`.

---

## Custom State

```rust
let app = apigate::App::builder()
    .state(DbPool(pool.clone()))
    .state(AuthConfig { jwt_secret: "...".into() })
    // ...
```

Доступ в хуках через `&T`:

```rust
#[apigate::hook]
async fn auth(ctx: &mut apigate::PartsCtx, config: &AuthConfig) -> apigate::HookResult {
    // config из shared state, zero-copy
    Ok(())
}
```

State хранится в `Arc<Extensions>` — **не клонируется** на каждый запрос. `scope.get::<T>()` читает shared-ссылку. `scope.insert()` / `scope.take()` работают с per-request хранилищем.

---

## Производительность

* Без `json/query/form` — body проксируется без чтения (streaming)
* `json = T` без `map` — валидация + passthrough оригинального тела
* State: `Arc<Extensions>`, 0 heap-аллокаций для read-only доступа
* Pipeline: path + hooks + body в одном `Box::pin`
* HTTP-клиент: `TCP_NODELAY`, connection pooling, keep-alive
* `request_timeout` → `504 Gateway Timeout`
* Балансировщики lock-free (atomic counters)
