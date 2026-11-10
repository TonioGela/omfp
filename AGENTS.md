# AGENTS.md

Guidance for coding agents working on **omfp** (formerly pmo, renamed at the end of M1): a GUI client (Dioxus, web +
desktop) for the omp coding agent (oh-my-pi). One binary, `omfp`: `omfp daemon` runs the daemon (one
`omp --mode rpc-ui` process per thread, WebSocket API, web UI); `omfp desktop` (M4) opens one more window on the local
daemon, starting it in the background if needed (D-15, D-16: many windows, one daemon, each window picks its thread).
Long-term direction (M8, D-17): agents get full access to the protocol and build arbitrary JavaScript UI pieces with an
`omfp` SDK; never design something that would only be reachable from the human UI.

## Read first

- `docs/specs/README.md` — index, conventions, decision log (D-01…D-17), open questions.
- `docs/specs/07-roadmap.md` — milestones and spike results (M0, M1 and M2 done; next: M3, tools and changes);
  `docs/specs/milestones.toml` — requirement IDs per milestone (enforced by `cargo xtask spec-trace`).
- The spec for the area you touch (`00`…`08`).

## Ground rules

- Everything in **English**: code, comments, docs, commit messages.
- **TDD**: tests first. Every test covering a requirement has `// spec: ID[, ID…]` right above it (parsed by
  `cargo xtask spec-trace`; never put the marker plus an ID inside a string literal). Deterministic tests only: no
  sleeps, temp dirs, no network beyond loopback, no real models.
- **Conventional Commits** (checked by commitizen in the commit-msg hook). Push to `master`; no branches/PRs during the
  PoC. Commits are SSH-signed with the user's key: if signing fails, ask the user to load the key, never disable it.
- Keep docs in sync: when behavior or a decision changes, update the spec rows (and spike results) in the same change.
- No new dependencies without a reason; workspace dependencies live in the root `Cargo.toml`.
- No `Box::leak`, no placeholder stubs, no `unwrap()` in non-test code where an error can be returned.

## Commands (always inside the dev shell)

```sh
nix develop                      # or: nix develop -c sh -c '<command>'
just ci                          # lint, check-versions, spec-trace, check-wasm, test, test-contract, test-wasm, test-e2e, deny
just test                        # fast tests (no omp process)
just test-contract               # contract tests against the pinned omp + fake LLM
just record-fixtures             # re-record fixtures/ (review the diff!)
just test-wasm                   # web transport suite in headless Chromium against a fake-launcher daemon
just test-e2e                    # L5 journeys: builds the web bundle, headless Chromium + daemon + omp + fake LLM
just build-web                   # dx build -p omfp-ui --platform web --release (no dev-tools overlay)
just serve [daemon args]         # try the app: release bundle + `omfp daemon` with your HOME, ~/.omp, default data dir
cargo xtask spec-trace           # requirement → citing tests; fails on broken traceability
cargo xtask check-versions       # omp / omp-rpc / wasm-bindgen / dioxus pins agree
```

Clippy runs with `pedantic` as errors. Notable: `assert_is_empty` forbids `assert!(x.is_empty())` — use
`assert_eq!(x, Vec::new())`; `large_enum_variant` is allowed only where we mirror omp-rpc unions.

## Layout

```text
crates/omfp-core      reducer over omp events (ThreadEvent → ThreadState); no I/O; compiles to wasm
crates/omfp-protocol  UI ↔ daemon protocol types, JSON Schemas; compiles to wasm
crates/omfp-daemon    daemon library: omp adapter (omp.rs), agent port + FakeLauncher (agent.rs, agent/fake.rs),
                      supervisor, event log, store, workspaces, threads, host tools, WebSocket server;
                      `omfp_daemon::{start, run, discover}` back `omfp daemon` (and later `omfp desktop`)
crates/omfp-client    protocol client without UI: Transport (native tokio-tungstenite / web-sys WebSocket), session
                      (hello, subscriptions, reconnect backoff), model (index, ThreadState per thread, dirty set)
crates/omfp-ui        the app: Dioxus UI (features web / desktop), the host glue (host/: client model → view input
                      per animation frame, intents → protocol calls, web token handling) and the `omfp` binary
crates/fake-llm       scripted OpenAI-compatible server (gated steps hold a reply to keep omp busy)
crates/omfp-testkit   TempRepo, OmpEnv (isolated omp), DenyProxy, fixture normalization
xtask                 spec-trace, check-versions
e2e                   L5 harness (thirtyfour + chromedriver, daemon in-process) and the M2 journeys
fixtures/omp/<ver>/      raw omp stdout frames (M0 smoke)
fixtures/threads/<ver>/  ThreadEvent JSONL recorded by daemon contract scenarios (reducer inputs)
```

## Version pinning

omp is pinned in three places that must agree (checked by `cargo xtask check-versions`): the `oh-my-pi` flake input
tag, the `omp-rpc` git tag in `Cargo.toml`, and `omfp_core::OMP_VERSION`. To upgrade, follow the playbook in
`docs/specs/01-dev-environment.md` (ENV-020).

## omp facts learned the hard way (omp 18.18.6)

- Launch with `omp --mode rpc-ui --session-dir <dir> --continue --add-dir …`: `--add-dir` roots (and their
  `AGENTS.md`) are lost if the session is switched later with `open_session` (S-07). `open_session` on the same dir is
  then a no-op that reports `resumed`.
- The generated `SetEventFilterCommand` lacks `messageUpdates`; the daemon sends its own frame for delta mode.
- `/add-dir` and `/remove-dir` work over RPC as `prompt` (local command, no model turn).
- The `ask` tool takes one HTML `<form>` and walks it with `select`/`input`/`editor` dialogs, not the RPC `ask` dialog.
- At startup omp always fetches its model catalog (`catalog.stencil.so` + keyless providers) and no setting disables
  it: contract tests run omp behind `DenyProxy` and fail on hosts outside `KNOWN_DISCOVERY_HOSTS`.
- omp reads project config from its cwd: the thread home (`threads/<id>/home`) holds per-thread `.omp/` files such as
  `mcp.json` (headers are sent).
- omp-rpc's client is blocking and closes the omp process group on drop; command default timeout is 30 s.
- Our harness docs (`omp://…`) may describe a different omp version than the pinned one: verify against the source at
  `~/.cargo/git/checkouts/oh-my-pi-*/<rev>/` (docs in `docs/`, Rust client in `sdk/rust/omp-rpc/`).

## Tooling gotchas

- `dx` 0.7.10 panics when the workspace has glob `default-members`: keep none; build with `dx build -p omfp-ui`.
- nixpkgs' `dioxus-cli` needs `zstd` (overridden in `flake.nix`); `convco` does not build, so commitizen checks
  commit messages.
- `typos` skips `fixtures/` (streamed deltas split words).
- Dioxus web only drops a `false` boolean attribute it knows (`disabled`, `checked`, `hidden`, …); others such as
  `inert` are written as `inert="false"`, which still applies. Use `attr: if condition { "true" }` for those.
- Fixtures are normalized (timestamps, durations, UUIDs, random ids, temp roots); if a re-record shows churn, add the
  volatile field to `omfp_testkit::normalize_frame` rather than accepting noise.
