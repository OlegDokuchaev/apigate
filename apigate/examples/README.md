# Examples

| Example | Shows |
|---|---|
| `basic` | Passthrough proxying, static rewrite (`to`), and rewrite templates (`{id}`). |
| `hooks` | Shared state in hooks, auth, header injection, hook chains, and per-request data through scope. |
| `errors` | Global JSON error renderer, `user_message`/`debug_details`, and custom JSON errors from hooks. |
| `logging` | `tracing` integration with `runtime_observer` and custom ApiGate runtime events. |
| `tower_logging` | External `tower_http::TraceLayer` added through `.with_router(...)`. |
| `runtime_tuning` | Listener socket tuning plus upstream hyper-util client/connector settings. |
| `path` | Path validation (UUID), path data in hooks (`&T`), and path data in map functions. |
| `map` | Query, JSON, and form transformations, including shared state access from a map. |
| `policy` | HeaderSticky + ConsistentHash, PathSticky, LeastRequest, LeastTime, and RoundRobin. |
| `multipart` | Multipart upload passthrough with and without auth. |

## Running

```sh
# 1. Start the mock upstream. One Caddyfile is shared by all examples.
caddy run --config apigate/examples/upstream/Caddyfile

# 2. Run any example in another terminal.
cargo run --example basic
cargo run --example hooks
cargo run --example errors
cargo run --example logging
cargo run --example tower_logging
cargo run --example runtime_tuning
cargo run --example path
cargo run --example map
cargo run --example policy
cargo run --example multipart
```

Each example prints ready-to-run `curl` commands.
