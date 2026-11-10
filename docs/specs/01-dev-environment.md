# 01 — Development environment and versions

## Dev shell

`flake.nix` (flake-parts) provides only a development shell; no package derivations yet.

```sh
nix develop          # or: direnv allow (.envrc contains `use flake`)
```

| Group | Tools |
| --- | --- |
| Agent | `omp` built from the pinned upstream release (`inputs.oh-my-pi`, `packages.<system>.omp`), with its `nixpkgs`, `rust-overlay` and `bun2nix` inputs following ours |
| Rust | toolchain from `rust-toolchain.toml` via rust-overlay (1.99.1, `wasm32-unknown-unknown`, clippy, rustfmt, rust-analyzer, rust-src, llvm-tools) |
| Dioxus | `dx` (dioxus-cli 0.7.10, with a local override adding `zstd`, see below), `wasm-bindgen-cli` 0.2.130, `binaryen` |
| Testing | `cargo-nextest`, `cargo-insta`, `cargo-llvm-cov`, `cargo-mutants`, `cargo-deny`, `bacon` |
| E2E / smoke (Linux) | `chromium`, `chromedriver`, `xvfb-run` |
| Desktop libs (Linux) | gtk3, webkitgtk 4.1, libsoup 3, glib(-networking), cairo, pango, gdk-pixbuf, atk, xdotool, openssl |
| Utilities | `just`, `git`, `jq`, `websocat`, `nixfmt` |

Environment set by the shell (Linux): `WEBKIT_DISABLE_DMABUF_RENDERER=1` (blank webkitgtk windows on some GPUs),
`GIO_MODULE_DIR` (TLS in webkitgtk), `CHROME_BIN`, `RUST_SRC_PATH`.

Known workaround: nixpkgs' `dioxus-cli` 0.7.10 fails to build at the locked nixpkgs revision (`zstd-sys` cannot find
`libzstd` through pkg-config). The flake overrides it with `zstd` in `buildInputs`. Remove the override when nixpkgs
fixes it.

| ID | Requirement |
| --- | --- |
| ENV-001 | Every tool needed by `just ci` MUST come from the dev shell; nothing is installed with `cargo install`, `npm` or downloaded by tools at run time (S-05 confirmed `dx` uses the shell's wasm-bindgen). |
| ENV-002 | `flake.lock` pins every input. `nixpkgs` and `rust-overlay` are updated deliberately, in their own commits, with `just ci` green. |
| ENV-003 | Supported dev systems: `x86_64-linux`, `aarch64-linux`, `aarch64-darwin`. Linux-only tools (desktop libs, Chromium, xvfb) are excluded on macOS. |

## Version pinning

The omp RPC protocol changes with every release, and `omp-rpc` is generated from it. The app MUST always run against the
exact omp version its client was built for.

| Item | Where | Current |
| --- | --- | --- |
| omp binary | `flake.nix` → `inputs.oh-my-pi.url = "github:can1357/oh-my-pi/v18.18.6"` (locked rev `9058ab9d229663e229e2867824ba6df6f21e9e59`) | 18.18.6 |
| `omp-rpc` crate | workspace `Cargo.toml` → `omp-rpc = { git = "https://github.com/can1357/oh-my-pi", tag = "v18.18.6" }` | 18.18.6 |
| Expected version in code | `pmo_core::OMP_VERSION` (DMN-001, contract tests) | `18.18.6` |
| Rust | `rust-toolchain.toml` | 1.99.1 |
| `dioxus` crates | workspace `Cargo.toml`, same minor as `dx` | 0.7.x (dx 0.7.10) |
| `wasm-bindgen` crate | workspace `Cargo.toml`, exact pin `=0.2.130`, equal to `wasm-bindgen-cli` in the shell | 0.2.130 |

| ID | Requirement |
| --- | --- |
| ENV-010 | `cargo xtask check-versions` (part of `just ci`) MUST check that the oh-my-pi tag and commit in `flake.lock`, the `omp-rpc` tag and commit in `Cargo.lock`, `pmo_core::OMP_VERSION`, and `omp --version` all agree. |
| ENV-011 | omp MUST be pinned to a release tag, never to `main` or an untagged commit. |
| ENV-012 | The same check MUST verify that the `wasm-bindgen` version in `Cargo.lock` equals `wasm-bindgen --version` from the shell, and that the `dioxus` minor version equals `dx --version`'s. |

## omp upgrade playbook

| ID | Requirement |
| --- | --- |
| ENV-020 | Upgrading omp MUST follow these steps, in one change: |

1. Pick the new release tag (`https://github.com/can1357/oh-my-pi/releases`).
2. Read the upstream diff of `docs/rpc.md` and `packages/coding-agent/CHANGELOG.md` between the two tags.
3. Update `inputs.oh-my-pi.url` and run `nix flake lock --update-input oh-my-pi`.
4. Update the `omp-rpc` tag and `pmo_core::OMP_VERSION`; run `cargo update -p omp-rpc`.
5. Re-record contract fixtures (`PMO_RECORD_FIXTURES=1 just test-contract`) and review their diff: new event kinds,
   renamed fields, changed tool argument shapes (TST-012, UI-051).
6. Fix what broke, then `just ci`.
