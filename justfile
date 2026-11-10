# Development tasks. Run inside the dev shell (`nix develop`).

# List recipes
default:
    @just --list

# Fast tests: unit, component and integration; no omp process (L0, L1, L2, L4). `e2e` builds on its own: thirtyfour
# turns on `serde_json/preserve_order`, which would change every crate's `Value` if built together.
test:
    cargo nextest run --workspace --exclude e2e -E 'not binary(/^contract_/)'

# Contract tests against the pinned omp and the fake LLM (L3)
test-contract:
    cargo nextest run --workspace --exclude e2e -E 'binary(/^contract_/)'

# End-to-end journeys (L5): headless Chromium on the web bundle, a daemon with the pinned omp and the fake LLM
test-e2e: build-web
    cargo nextest run -p e2e

# Re-record omp fixtures under fixtures/omp/<version>/ (TST-012)
record-fixtures:
    OMFP_RECORD_FIXTURES=1 just test-contract

# spec: ARC-010 — the shared crates compile for the browser
check-wasm:
    cargo check -p omfp-core -p omfp-protocol -p omfp-client --target wasm32-unknown-unknown

# spec: TST-030 — the web transport runs the transport suite in headless Chromium against a daemon started here
test-wasm:
    #!/usr/bin/env bash
    set -euo pipefail
    data=$(mktemp -d)
    coproc daemon { exec cargo run -q -p omfp-client --example test_daemon -- "$data"; }
    trap 'kill "$daemon_PID" 2>/dev/null; wait "$daemon_PID" 2>/dev/null || true; rm -rf "$data"' EXIT
    read -r url token runner <&"${daemon[0]}"
    OMFP_TEST_DAEMON_URL="$url" OMFP_TEST_DAEMON_TOKEN="$token" WASM_BINDGEN_TEST_ADDRESS="$runner" \
    CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
        cargo test -p omfp-client --target wasm32-unknown-unknown --test transport_web

# Web bundle in target/dx/omfp (dx uses the dev shell's wasm-bindgen)
build-web:
    dx build -p omfp-ui --platform web

# Formatting and lints
lint:
    cargo fmt --all --check
    cargo clippy --workspace --exclude e2e --all-targets -- -D warnings
    cargo clippy -p e2e --all-targets --no-deps -- -D warnings
    cargo clippy -p omfp-ui --target wasm32-unknown-unknown --features web -- -D warnings

# spec: ENV-010, ENV-011, ENV-012 — omp, omp-rpc, wasm-bindgen and dioxus versions agree
check-versions:
    cargo xtask check-versions

# spec: TST-040 — requirement traceability
spec-trace:
    cargo xtask spec-trace

# Dependency licenses, advisories and sources
deny:
    cargo deny check

# Mutation testing of the pure crates (TST-005); pass `--in-diff <file>` to limit it to a change
mutants *args:
    cargo mutants --test-tool nextest -p omfp-core -p omfp-protocol -p fake-llm -p xtask {{ args }}

# Coverage report (HTML in target/llvm-cov/html)
cov:
    cargo llvm-cov nextest --workspace --exclude e2e --html -E 'not binary(/^contract_/)'

# Everything a change must pass
ci: lint check-versions spec-trace check-wasm test test-contract test-wasm test-e2e deny
