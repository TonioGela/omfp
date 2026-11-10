# Development tasks. Run inside the dev shell (`nix develop`).

# List recipes
default:
    @just --list

# Fast tests: unit, component and integration; no omp process (L0, L1, L2, L4)
test:
    cargo nextest run --workspace -E 'not binary(/^contract_/)'

# Contract tests against the pinned omp and the fake LLM (L3)
test-contract:
    cargo nextest run --workspace -E 'binary(/^contract_/)'

# Re-record omp fixtures under fixtures/omp/<version>/ (TST-012)
record-fixtures:
    PMO_RECORD_FIXTURES=1 just test-contract

# spec: ARC-010 — the shared crates compile for the browser
check-wasm:
    cargo check -p pmo-core -p pmo-protocol --target wasm32-unknown-unknown

# Web bundle in target/dx/pmo-ui (dx uses the dev shell's wasm-bindgen)
build-web:
    dx build -p pmo-ui --platform web

# Formatting and lints
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings

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
    cargo mutants --test-tool nextest -p pmo-core -p pmo-protocol -p fake-llm -p xtask {{ args }}

# Coverage report (HTML in target/llvm-cov/html)
cov:
    cargo llvm-cov nextest --workspace --html -E 'not binary(/^contract_/)'

# Everything a change must pass
ci: lint check-versions spec-trace check-wasm test test-contract deny
