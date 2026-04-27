# Contributing

Thanks for working on ApiGate. This repository is a Rust workspace with three crates:

- `apigate-core`: runtime, proxying, routing, balancing, errors, and observability.
- `apigate-macros`: procedural macros and macro-time parsers/codegen.
- `apigate`: public facade crate, examples, macro UI tests, and e2e tests.

## Requirements

- Rust 1.88 or newer. Latest stable Rust is recommended and is what full CI uses.
- `rustfmt` and `clippy` components.
- No external services are required for the test suite.

Install components if needed:

```bash
rustup component add rustfmt clippy
```

## Local Checks

Run the same checks as CI before opening a pull request:

```bash
cargo fmt --all --check
cargo +1.88.0 check --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
cargo test --workspace --examples --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps --locked
RUSTDOCFLAGS='-D warnings' cargo rustdoc -p apigate-core --all-features --locked -- -D missing_docs
RUSTDOCFLAGS='-D warnings' cargo rustdoc -p apigate-macros --all-features --locked -- -D missing_docs
cargo package -p apigate-core --locked --allow-dirty
cargo package -p apigate-macros --locked --allow-dirty
cargo package -p apigate --locked --allow-dirty --no-verify
```

Use `--allow-dirty` only for local package checks while you have uncommitted changes. CI runs package verification without it.

The facade crate uses `--no-verify` for package checks because its local path dependencies are converted to registry dependencies during packaging. Full facade behavior is still covered by workspace tests, docs, and examples; verified facade packaging requires matching `apigate-core` and `apigate-macros` versions to already be published.

## Test Layout

- Put runtime/unit tests next to the relevant `apigate-core/src/**` module when private helpers are involved.
- Put parser/template unit tests next to the relevant `apigate-macros/src/**` module.
- Put public API, macro UI, and e2e tests under `apigate/tests`.
- Use `trybuild` for macro compile-pass/compile-fail behavior.

## Updating Trybuild Snapshots

When intentional macro diagnostics change, update stderr snapshots with:

```bash
TRYBUILD=overwrite cargo test -p apigate --test macro_params
```

Review generated `.stderr` files before committing them.

## Coverage

Coverage is optional and not a merge gate. To run it locally, install `cargo-llvm-cov` and run:

```bash
cargo llvm-cov --workspace --all-features --all-targets --summary-only
cargo llvm-cov --workspace --all-features --all-targets --lcov --output-path lcov.info
```

The GitHub Actions coverage workflow can also be started manually.

## Release Checklist

Before publishing crates:

1. Update `CHANGELOG.md`.
2. Run the full local checks.
3. Package crates in dependency order: `apigate-core`, `apigate-macros`, then `apigate`.
4. Publish crates in the same order.
5. Create a Git tag matching the released version.

## Style

- Keep public APIs documented.
- Keep examples and documentation in English.
- Do not add runtime logging or allocations unless they are justified by behavior.
- Prefer small, explicit tests over broad tests that make failures hard to diagnose.
