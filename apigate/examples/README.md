# Примеры

| Пример | Что показывает |
|---|---|
| `basic` | Passthrough, static rewrite (`to`), rewrite-шаблон (`{id}`) |
| `hooks` | Shared state в хуке, auth, инъекция заголовков, цепочка хуков, per-request data через scope |
| `path` | Валидация path (UUID), доступ к path в хуке (`&T`), доступ к path в map |
| `map` | Преобразование query, json (+ shared state), form |

## Запуск

```sh
# 1) Mock upstream (один Caddyfile для всех примеров)
caddy run --config apigate/examples/upstream/Caddyfile

# 2) Любой пример (в другом терминале)
cargo run --example basic
cargo run --example hooks
cargo run --example path
cargo run --example map
```

Каждый пример при запуске выводит curl-команды для тестирования.
