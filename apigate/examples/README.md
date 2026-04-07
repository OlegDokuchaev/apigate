# Demo

Демонстрирует все возможности `apigate`.

## Что покрыто

| Фича | Маршрут | Что показывает |
|---|---|---|
| Passthrough | `GET /sales/ping` | Проксирование без хуков |
| Rewrite `to` | `GET /sales/public` | `/public` → `/internal` |
| Rewrite template | `GET /sales/item/{id}/review` | `/{id}/review` → `/api/v2/reviews/{id}` |
| Shared state в hook | `GET /sales/admin/stats` | `&AppConfig` из `.state()` |
| Auth hook | `GET /sales/user` | Проверка токена + инъекция заголовков |
| Цепочка хуков | `GET /sales/secure-user` | 4 хука: api_key → auth → request_id → log_meta |
| Per-request data | `GET /sales/secure-user` | `scope.insert(RequestMeta)` → `meta: RequestMeta` |
| Path validation | `GET /sales/{id}` | `path = SaleIdPath` (UUID), 400 при невалидном |
| Path в hook | `GET /sales/{id}` | `path: &SaleIdPath` — чтение из scope |
| Path в map | `POST /sales/{id}/update` | `path: &SaleIdPath` в map-функции |
| Query map | `GET /sales/products` | `page/size` → `offset/limit` |
| Json map | `POST /sales/buy` | Преобразование JSON + `&AppConfig` в map |
| Form map | `POST /sales/legacy-create` | `category` → `category_code` |
| Multipart | `POST /sales/upload` | Passthrough без чтения body |

## Запуск

```sh
# 1) Mock upstream (Caddy на :8081)
caddy run --config apigate/examples/upstream/Caddyfile

# 2) apigate (другой терминал)
cargo run --example demo
```

## Тесты

```sh
# Passthrough
curl -i http://127.0.0.1:8080/sales/ping

# Rewrite
curl -i http://127.0.0.1:8080/sales/public

# Shared state (api key из AppConfig)
curl -i -H 'x-api-key: secret-key' http://127.0.0.1:8080/sales/admin/stats

# Auth + inject headers
curl -i -H 'authorization: Bearer test' http://127.0.0.1:8080/sales/user

# Hook chain + per-request data (смотри stdout apigate)
curl -i -H 'x-api-key: secret-key' -H 'authorization: Bearer test' \
  http://127.0.0.1:8080/sales/secure-user

# Path validation (UUID) — 200
curl -i http://127.0.0.1:8080/sales/11111111-1111-1111-1111-111111111111

# Path validation — 400
curl -i http://127.0.0.1:8080/sales/not-a-uuid

# Path + json + map (path доступен в map)
curl -i -X POST \
  -H 'authorization: Bearer test' -H 'content-type: application/json' \
  -d '{"title":"New Title"}' \
  http://127.0.0.1:8080/sales/11111111-1111-1111-1111-111111111111/update

# Rewrite template
curl -i http://127.0.0.1:8080/sales/item/abc-123/review

# Query map
curl -i 'http://127.0.0.1:8080/sales/products?page=2&size=5&q=test'

# Json map + shared state в map
curl -i -X POST \
  -H 'authorization: Bearer test' -H 'content-type: application/json' \
  -d '{"sale_ids":["11111111-1111-1111-1111-111111111111"],"coupon":"sale10","use_bonus_points":true}' \
  http://127.0.0.1:8080/sales/buy

# Form map
curl -i -X POST \
  -H 'x-api-key: secret-key' -H 'content-type: application/x-www-form-urlencoded' \
  -d 'title=Demo+Title&category=pets' \
  http://127.0.0.1:8080/sales/legacy-create
```
