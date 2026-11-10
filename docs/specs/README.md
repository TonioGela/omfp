# omfp — Specifications

A GUI client for [oh-my-pi](https://github.com/can1357/oh-my-pi) (omp), in the spirit of Orca and T3 Code: it runs many
agent threads in parallel. A thread is not tied to a repository: it works on any set of **workspaces** (typically one git
worktree per PR, possibly across several repositories, e.g. frontend + backend). It ships as a native desktop app and as
a web app in the browser, from a single Rust codebase (Dioxus). Long term, the interface is meant to be **driven by the
agents themselves** (spawning threads, publishing views) — see [08](08-agent-control.md).

## Documents

| # | Document | Contents |
| --- | --- | --- |
| 00 | [Overview and architecture](00-overview.md) | Goals, non-goals, components, crates, flows |
| 01 | [Development environment and versions](01-dev-environment.md) | Flake, toolchain, pinning of omp / omp-rpc / wasm-bindgen |
| 02 | [Daemon](02-daemon.md) | Threads, workspaces, omp process supervision, persistence |
| 03 | [UI ↔ daemon protocol](03-protocol.md) | WebSocket, snapshot/delta, commands, interactive requests |
| 04 | [Dioxus UI](04-ui.md) | Views, state, tool renderers, web vs desktop |
| 05 | [Terminal (escape hatch to the TUI)](05-terminal.md) | PTY, xterm.js, session ownership handoff |
| 06 | [Testing strategy (TDD)](06-testing.md) | Process, layers, fake LLM, E2E, mutation testing |
| 07 | [Roadmap](07-roadmap.md) | Milestones with acceptance criteria |
| 08 | [Agent control (direction)](08-agent-control.md) | Agents spawning threads and drawing UI; constraints that apply from day one |

## Conventions

- **MUST / SHOULD / MAY** as in RFC 2119.
- Every requirement has a stable ID (`DMN-012`, `PRT-003`, …). Tests covering it cite the ID in their name or in a
  `// spec: DMN-012` comment. A requirement without a test counts as not implemented (see
  [06](06-testing.md#traceability)).
- Prefixes: `ARC` architecture, `ENV` environment, `DMN` daemon, `PRT` protocol, `UI` interface, `TRM` terminal,
  `TST` testing, `AGT` agent control.
- Facts about omp cite upstream documentation (`docs/*.md` in the oh-my-pi repo, at the tag pinned in `flake.lock`).
  Unverified points are marked **[VERIFY]** and have a spike in the roadmap.
- Docs, plans, code, comments and commit messages are in English.

## Decision log

| ID | Decision | Rationale | Rejected alternatives |
| --- | --- | --- | --- |
| D-01 | Integrate with omp over **RPC** (`omp --mode rpc-ui`), one process per thread | Complete protocol, official Rust client (`omp-rpc`), per-thread isolation | ACP (too narrow), in-process SDK (Bun/TS only), TSP (single vendor, we would have to write the renderer) |
| D-02 | UI in **Dioxus** 0.7, same app for web (wasm) and desktop (webview) | One codebase; the browser is a first-class target | GPUI (web support is experimental), Leptos + Tauri (two pieces to integrate) |
| D-03 | The **daemon** is the only component talking to omp; the UI only ever talks to the daemon over WebSocket, desktop included | One transport path to test; several clients at once | Desktop UI talking to omp directly (two code paths) |
| D-04 | Thread state is derived by a **pure reducer shared** by daemon and UI (`omfp-core`) | Testable in isolation (TDD, property tests); snapshots for late-joining clients | State computed only in the daemon with a "dumb" UI (more bandwidth, less reuse) |
| D-05 | Worktrees are managed **by the daemon** through the `git` CLI, not by omp (`/wt`); the daemon can also **adopt** any existing directory | We own the lifecycle; testable with temporary repos; agents or the user may have created worktrees elsewhere | omp's `/wt` (couples worktree to session, not drivable over RPC) |
| D-06 | Anything RPC does not expose (plan mode, `/vibe`, `/settings`, extensions with custom UI) goes through a **terminal panel** running interactive omp on the same session | Full TUI parity at low cost | Implementing TSP (see D-01) |
| D-07 | Pinned versions: omp (flake input at a release tag) = `omp-rpc` (git dependency at the same tag) | The RPC protocol changes every release; mismatched versions drop frames | Tracking `main` (unstable), hand-copied omp-rpc |
| D-08 | **TDD** is mandatory; a scripted fake LLM written in Rust makes tests against the real omp deterministic | Fast, repeatable tests without real model calls | Only protocol mocks (would not catch omp breaking changes) |
| D-09 | Daemon persistence in **SQLite** (`rusqlite`, bundled) | Simple, transactional, no external service | JSON files (no transactions), Postgres (extra service) |
| D-10 | Styling with **plain CSS** and custom properties (design tokens), no Node toolchain | Fewer dependencies in the dev shell | Tailwind (needs its binary and config) |
| D-11 | **Threads and workspaces are many-to-many**; repositories are an optional registry, not a container. A thread's cwd is its own **home directory** (`threads/<id>/home`); every workspace is passed with `--add-dir` (live: `/add-dir`, `/remove-dir`) | Matches a PR-per-worktree workflow where one session spans several repos; the UI must not impose a hierarchy; per-thread `.omp/` config (e.g. an injected MCP server, 08) lives in the home dir without touching user repos | Repo → threads tree (Orca/T3 style), too rigid; first workspace as cwd (per-thread config would have to be written into a user repo) |
| D-12 | **API-first**: every capability of the UI is a daemon protocol method usable by any authorized client, including (later) agents | Agent control (08) becomes an adapter, not a rewrite | UI-only features |
| D-13 | **Network exposure is the deployer's job**: the daemon listens on a configurable address and port, with the token as its only authentication; tunnels, reverse proxies and TLS are configured by whoever installs it | Keeps the daemon small; every setup (SSH, Tailscale, reverse proxy) works the same way | A built-in login system or TLS termination |
| D-14 | **omp is bundled** with the app at the pinned version; a different binary is only used when given explicitly (`--omp-bin`) and still passes the version check (DMN-001) | The client is generated for one omp version; bundling removes "wrong omp on PATH" failures | Relying on the system omp |
| D-15 | **One binary with subcommands**: `omfp daemon` runs the headless daemon (serving the web UI); `omfp desktop` opens a desktop window connected to the local daemon, starting one if none is running (one command for users who do everything on one machine). The web UI is the same crate built for wasm. A build without the `desktop` feature gives a daemon-only binary for headless hosts | Simple installation and UX; one artifact to ship next to the bundled omp | Separate daemon and desktop binaries |
| D-16 | **Many windows, one daemon** (like Zen browser windows): every `omfp desktop` opens a new window on the same local daemon, so all windows share threads and state, while each window chooses its own thread and layout. The daemon started by `omfp desktop` runs as a background process, so closing a window never stops the others | Several views of the same work without duplicating threads or omp processes | A daemon per window (duplicate threads); the first window owning the daemon (closing it would break the others) |
| D-17 | **Agents have full access and build UI in JavaScript**: every protocol method is an agent tool; agents mount arbitrary HTML/CSS/JS pieces of UI with an `omfp` SDK (full protocol, live sources, storage) and an optional widget kit; full-access ("yolo") is the default, limits are opt-in | Maximum freedom for agents; JavaScript is what models write best and runs in every client webview | A fixed declarative vocabulary (too limiting); a sandboxed script engine in the daemon with restricted access |

## Open questions

1. **Desktop platforms**: Linux only at first, macOS later? (Windows: not planned.)
2. **Project license.**
3. **Reusable agent-made UI**: the SDK surface, the widget kit, how pieces load on desktop, and whether pieces may run
   without a window — direction in [08](08-agent-control.md#agent-made-ui-javascript).
