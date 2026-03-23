# ApiGate

`apigate` — библиотека для единой точки входа (прокси) в микросервисной системе на Rust.

Что делает библиотека:

* генерирует маршруты через макросы;
* типизированно извлекает `Path / Query / Json / Form / Multipart`;
* позволяет запускать пользовательские хуки (`before`) для аутентификации/валидации/обновления заголовков;
* позволяет типизированно преобразовывать `Json / Query / Form` через `map`;
* применяет runtime-политики (таймауты, балансировка, маршрутизация), заданные в `main`;
* использует `axum` во внутренней реализации (публично используется только API `apigate`).

---

## Быстрый старт

### 1) `main`: конфиги и runtime-настройки

Все runtime-параметры задаются в `main`:

* backend-адреса,
* таймауты,
* балансировка,
* стратегия маршрутизации.

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = AppConfig::from_env();

    let app = apigate::App::builder()
        // Backend-пулы
        .backend("sales", cfg.sales_backends)
        .backend("files", cfg.files_backends)
        
        // Общий timeout по умолчанию
        .default_timeout(std::time::Duration::from_millis(cfg.default_timeout_ms))

        // Политики
        .policy(
            "sales_default",
            apigate::Policy::new()
                .router(apigate::routing::HeaderSticky::new("x-user-id"))
                .balancer(apigate::balancing::ConsistentHash::new()),
        )
        .policy(
            "files_default",
            apigate::Policy::new()
                .router(apigate::routing::NoRouteKey)
                .balancer(apigate::balancing::RoundRobin::new()),
        )

        // Подключаем сгенерированные маршруты
        .mount(sales::routes!())
        .mount(files::routes!())

        .build()?;

    apigate::run(cfg.listen, app).await
}
```

---

## Сервис

Сервис описывается модулем с `#[apigate::service]`.

* `name` — имя сервиса (и ключ backend-пула в `main`)
* `prefix` — внешний префикс
* `policy` — политика по умолчанию

```rust
#[apigate::service(
    name = "sales",
    prefix = "/sales",
    policy = "sales_default"
)]
mod sales {
    // маршруты
}
```

---

## Маршруты

### Простой `GET`

```rust
#[apigate::get("/ping")]
async fn ping() {}
```

### Путь по умолчанию (`to` не нужен)

Если `to` не указан, библиотека считает:

* `to == path`

То есть `#[apigate::get("/ping")]` проксирует в `/ping`.

### Переопределение пути в целевом сервисе

```rust
#[apigate::get("/public-products", to = "/internal/products")]
async fn products() {}
```

---

## Типизированные входные данные

### `Path`

Используется синтаксис путей `/{id}`.

```rust
#[derive(serde::Deserialize)]
struct SaleIdPath {
    sale_id: uuid::Uuid,
}

#[apigate::get("/{sale_id}", path = SaleIdPath)]
async fn get_sale_by_id() {}
```

### `Query`

```rust
#[derive(serde::Deserialize)]
struct SearchQuery {
    page: Option<u32>,
    size: Option<u32>,
}

#[apigate::get("/search", query = SearchQuery)]
async fn search() {}
```

### `Json`

```rust
#[derive(serde::Deserialize)]
struct BuyInput {
    sale_ids: Vec<uuid::Uuid>,
}

#[apigate::post("/buy", json = BuyInput)]
async fn buy() {}
```

### `Form` (`application/x-www-form-urlencoded`)

```rust
#[derive(serde::Deserialize)]
struct LegacyFilterForm {
    page: Option<u32>,
    size: Option<u32>,
}

#[apigate::post("/legacy-filter", form = LegacyFilterForm)]
async fn legacy_filter() {}
```

### `Multipart` (загрузка файла)

```rust
#[apigate::post("/upload", multipart)]
async fn upload_file() {}
```

---

## Хуки `before`

`before` — это пользовательская логика, которая выполняется **до отправки запроса** в сервис.

`before`-хуки работают с частями запроса (заголовки, путь, query-строка, extensions) и обычно используются для:

* аутентификации;
* валидации;
* добавления/изменения заголовков;
* генерации служебных значений (`x-request-id` и т.п.).

### Объявление хука

```rust
#[apigate::hook]
async fn verify_api_key(ctx: &mut apigate::PartsCtx<'_>) -> apigate::HookResult {
    let api_key = ctx
        .header("x-api-key")
        .ok_or_else(|| apigate::HookError::forbidden("missing api key"))?;

    if api_key != "secret-key" {
        return Err(apigate::HookError::forbidden("invalid api key"));
    }

    Ok(())
}
```

### Подключение хука

```rust
#[apigate::get(
    "/admin/stats",
    before = [verify_api_key]
)]
async fn admin_stats() {}
```

---

## Изменение заголовков (`before`)

```rust
#[apigate::hook]
async fn get_current_user(ctx: &mut apigate::PartsCtx<'_>) -> apigate::HookResult {
    // ваша логика аутентификации
    ctx.set_header("x-user-id", "11111111-1111-1111-1111-111111111111");
    ctx.set_header("x-user-role", "user");
    ctx.set_header_if_absent("x-request-id", "generated-request-id");
    Ok(())
}
```

Использование:

```rust
#[apigate::get(
    "/user",
    before = [get_current_user]
)]
async fn get_user_sales() {}
```

---

## Преобразование данных (`map`)

`map` — это типизированное преобразование входных данных перед отправкой в сервис.

Поддерживаются:

* `query = T, map = ...`
* `json = T, map = ...`
* `form = T, map = ...`

### Почему `map` отдельно от `before`

* `before` — для заголовков/проверок;
* `map` — для преобразования типизированного тела/параметров.

Так проще и понятнее: нет смешивания аутентификации и преобразования данных.

---

## Изменение `Json` через `map`

### Пример: внешний и внутренний JSON отличаются

```rust
#[derive(serde::Deserialize)]
struct PublicBuyInput {
    sale_ids: Vec<uuid::Uuid>,
    coupon: Option<String>,
    use_bonus_points: Option<bool>,
}

#[derive(serde::Serialize)]
struct ServiceBuyInput {
    sale_ids: Vec<uuid::Uuid>,
    promo_code: Option<String>,
    payment_mode: String,
    source: &'static str,
}

#[apigate::map]
async fn remap_buy_json(
    input: PublicBuyInput,
    _ctx: &mut apigate::PartsCtx<'_>,
) -> apigate::MapResult<ServiceBuyInput> {
    Ok(ServiceBuyInput {
        sale_ids: input.sale_ids,
        promo_code: input.coupon,
        payment_mode: if input.use_bonus_points.unwrap_or(false) {
            "bonus".to_string()
        } else {
            "money".to_string()
        },
        source: "apigate",
    })
}
```

Использование:

```rust
#[apigate::post(
    "/buy",
    json = PublicBuyInput,
    before = [get_current_user],
    map = remap_buy_json
)]
async fn buy() {}
```

---

## Изменение `Query` через `map`

```rust
#[derive(serde::Deserialize)]
struct ProductsQueryPublic {
    page: Option<u32>,
    size: Option<u32>,
    q: Option<String>,
}

#[derive(serde::Serialize)]
struct ProductsQueryService {
    offset: u32,
    limit: u32,
    query: Option<String>,
}

#[apigate::map]
async fn remap_products_query(
    input: ProductsQueryPublic,
    _ctx: &mut apigate::PartsCtx<'_>,
) -> apigate::MapResult<ProductsQueryService> {
    let page = input.page.unwrap_or(1).max(1);
    let size = input.size.unwrap_or(20).clamp(1, 100);

    Ok(ProductsQueryService {
        offset: (page - 1) * size,
        limit: size,
        query: input.q.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()),
    })
}
```

Использование:

```rust
#[apigate::get(
    "/products",
    query = ProductsQueryPublic,
    map = remap_products_query
)]
async fn get_products() {}
```

---

## Изменение `Form` через `map`

```rust
#[derive(serde::Deserialize)]
struct LegacyFormPublic {
    title: String,
    category: String,
}

#[derive(serde::Serialize)]
struct LegacyFormService {
    title: String,
    category_code: String,
}

#[apigate::map]
async fn remap_legacy_form(
    input: LegacyFormPublic,
    _ctx: &mut apigate::PartsCtx<'_>,
) -> apigate::MapResult<LegacyFormService> {
    let code = match input.category.as_str() {
        "pets" => "P",
        "items" => "I",
        _ => "U",
    };

    Ok(LegacyFormService {
        title: input.title.trim().to_string(),
        category_code: code.to_string(),
    })
}
```

Использование:

```rust
#[apigate::post(
    "/legacy-create",
    form = LegacyFormPublic,
    before = [verify_api_key],
    map = remap_legacy_form
)]
async fn legacy_create() {}
```

---

## Multipart (файлы)

Для загрузки файлов используйте `multipart`-маршрут. По умолчанию библиотека проксирует данные в сервис без преобразования тела.

```rust
#[apigate::post(
    "/upload",
    multipart,
    before = [get_current_user]
)]
async fn upload_file() {}
```

---

## Политики (таймауты, балансировка, маршрутизация)

Политики задаются **только в `main`** и привязываются по имени.

```rust
let app = apigate::App::builder()
    .default_timeout(std::time::Duration::from_millis(1500))
    .policy(
        "sales_default",
        apigate::Policy::new()
            .router(apigate::routing::HeaderSticky::new("x-user-id"))
            .balancer(apigate::balancing::ConsistentHash::new()),
    )
    .build()?;
```

### Переопределение политики на маршруте

```rust
#[apigate::get(
    "/user",
    before = [get_current_user],
    policy = "sales_user_sticky"
)]
async fn get_user_sales() {}
```

---

## Производительность

`apigate` спроектирован так, чтобы по умолчанию проксировать запросы с минимальными накладными расходами:

* если нет `map`, тело запроса проксируется дальше без преобразования;
* `before`-хуки работают только с частями запроса;
* `multipart` по умолчанию проксируется как passthrough;
* разбор `Json / Query / Form` выполняется только если он явно указан в маршруте.

---

## Правила

1. `service.name` должен совпадать с ключом backend-пула в `main`:

   * `name = "sales"` ↔ `.backend("sales", ...)`

2. Если `to` не указан, используется `path`.

3. Для `map` маршрут должен объявлять соответствующий тип входа:

   * `json = T` + `map = ...`
   * `query = T` + `map = ...`
   * `form = T` + `map = ...`

4. `Path` использует синтаксис `/{id}`.

5. `Multipart` используется для `multipart/form-data`, `Form<T>` — для `application/x-www-form-urlencoded`.

6. `Multipart`, `Json`, `Form` — body-режимы маршрута; в одном маршруте используется только один body-режим.

---

## Минимальный пример

```rust
#[apigate::service(name = "sales", prefix = "/sales", policy = "sales_default")]
mod sales {
    use super::*;

    #[apigate::get("/ping")]
    async fn ping() {}

    #[apigate::get("/admin/stats", before = [verify_api_key])]
    async fn admin_stats() {}

    #[apigate::get("/user", before = [get_current_user], policy = "sales_user_sticky")]
    async fn get_user_sales() {}

    #[apigate::post("/buy", json = PublicBuyInput, before = [get_current_user], map = remap_buy_json)]
    async fn buy() {}

    #[apigate::post("/upload", multipart, before = [get_current_user])]
    async fn upload_file() {}
}
```
