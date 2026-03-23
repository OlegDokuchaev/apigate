# Demo

Пример демонстрирует основные возможности `apigate`:

* passthrough-маршруты (`/ping`)
* alias (`/public` -> `/internal`)
* before-хуки: аутентификация, инъекция заголовков
* map: преобразование query, json, form
* path-параметры (`/{id}`)
* комбинирование нескольких хуков

## Требования

* [Caddy](https://caddyserver.com/docs/install) (mock upstream)

## Запуск

**1) Mock upstream** (Caddy на :8081):

```sh
caddy run --config apigate/examples/upstream/Caddyfile
```

**2) apigate** (в другом терминале):

```sh
cargo run --example demo
```

## Тестирование

```sh
# Простой passthrough
curl -i http://127.0.0.1:8080/sales/ping

# Alias: /public -> /internal
curl -i http://127.0.0.1:8080/sales/public

# API key
curl -i -H 'x-api-key: secret-key' http://127.0.0.1:8080/sales/admin/stats

# Auth + инъекция заголовков
curl -i -H 'authorization: Bearer test' http://127.0.0.1:8080/sales/user

# Несколько before-хуков
curl -i \
  -H 'x-api-key: secret-key' \
  -H 'authorization: Bearer test' \
  http://127.0.0.1:8080/sales/secure-user

# Path-параметр
curl -i http://127.0.0.1:8080/sales/abc-123

# Map query: page/size -> offset/limit
curl -i 'http://127.0.0.1:8080/sales/products?page=2&size=5&q=test'

# Map json: coupon -> promo_code, use_bonus_points -> payment_mode
curl -i -X POST http://127.0.0.1:8080/sales/buy \
  -H 'authorization: Bearer test' \
  -H 'content-type: application/json' \
  -d '{"sale_ids":["11111111-1111-1111-1111-111111111111"],"coupon":"sale10","use_bonus_points":true}'

# Map form: category -> category_code
curl -i -X POST http://127.0.0.1:8080/sales/legacy-create \
  -H 'x-api-key: secret-key' \
  -H 'content-type: application/x-www-form-urlencoded' \
  --data 'title=Demo+Title&category=pets'
```
