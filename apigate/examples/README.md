# Примеры

| Пример | Что показывает |
|---|---|
| `basic` | Passthrough, static rewrite (`to`), rewrite-шаблон (`{id}`) |
| `hooks` | Shared state в хуке, auth, инъекция заголовков, цепочка хуков, per-request data через scope |
| `errors` | Глобальный JSON error renderer, `user_message`/`debug_details`, кастомный JSON из hook |
| `path` | Валидация path (UUID), доступ к path в хуке (`&T`), доступ к path в map |
| `map` | Преобразование query, json (+ shared state), form |
| `policy` | HeaderSticky + ConsistentHash, LeastRequest, LeastTime, RoundRobin |
| `multipart` | Загрузка файлов: passthrough с auth и без |

## Запуск

```sh
# 1) Mock upstream (один Caddyfile для всех примеров)
caddy run --config apigate/examples/upstream/Caddyfile

# 2) Любой пример (в другом терминале)
cargo run --example basic
cargo run --example hooks
cargo run --example errors
cargo run --example path
cargo run --example map
cargo run --example policy
cargo run --example multipart
```

Каждый пример при запуске выводит curl-команды для тестирования.
