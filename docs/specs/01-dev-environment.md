# 01 — Development environment and versions

## Dev shell

`flake.nix` (flake-parts) provides only a development shell; no package derivations yet.

```sh
nix develop          # or: direnv allow (.envrc contains `use flake`)
```

| Group | Tools |
| --- | --- |
| Agent | `omp` built from the pinned upstream release (`inputs.oh-my-pi`, `packages.<system>.omp`), with its `nixpkgs`, `rust-overlay` and `bun2nix` inputs following ours |
| Rust | toolchain from `rust-toolchain.toml` via rust-overlay (1.99.1, `wasm32-unknown-unknown` for `just check-wasm` (ARC-010), clippy, rustfmt, rust-analyzer, rust-src, llvm-tools) |
| Testing | `cargo-nextest`, `cargo-insta`, `cargo-llvm-cov`, `cargo-mutants`, `cargo-deny`, `bacon` |
| TypeScript (`ui/`) | `bun` 1.4.4 (runtime, package manager, test runner), `bun2nix` 2.1.2 (patched, see below), `biome` 2.6.5 (format and lint; from nixpkgs because the npm build is a glibc binary NixOS cannot run); `typescript` 7.0.2 (native `tsc`) and `@types/bun` come from `ui/bun.lock` through `ui/bun.nix` |
| E2E / smoke (Linux) | `chromium`, `chromedriver`, `xvfb-run` |
| Utilities | `just`, `git`, `jq`, `websocat`, `nixfmt` |

Environment set by the shell: `RUST_SRC_PATH`, `OMFP_BUN_CACHE` (the Bun install cache of `ui/`'s dependencies, built by
Nix from `ui/bun.nix`); on Linux also `CHROME_BIN`.

Known workarounds:

- bun2nix 2.1.2 (the `oh-my-pi/bun2nix` input) rejects Bun 1.4's `bun.lock` only because it checks
  `lockfileVersion == 1`; versions 2 and 3 parse unchanged. The flake applies the open upstream PR
  nix-community/bun2nix#110 (commit `0456acb`) with `fetchpatch`. Drop it once the PR is released and the input moves
  to that bun2nix (or an omp upgrade, ENV-020, brings it).

| ID | Requirement |
| --- | --- |
| ENV-001 | Every tool needed by `just ci` MUST come from the dev shell; nothing is installed with `cargo install`, `npm` or downloaded by tools at run time. The JavaScript dependencies of `ui/` come from Nix too: `bun2nix` turns `ui/bun.lock` into `ui/bun.nix` (`just gen-bun-nix`, committed; `just check-bun-nix`, run by `just ui-install`, fails when it is stale), Nix builds Bun's install cache from it (`OMFP_BUN_CACHE`), and `just ui-install` runs `bun install --frozen-lockfile --offline` against that cache. |
| ENV-002 | `flake.lock` pins every input. `nixpkgs` and `rust-overlay` are updated deliberately, in their own commits, with `just ci` green. |
| ENV-003 | Supported dev systems: `x86_64-linux`, `aarch64-linux`, `aarch64-darwin`. Linux-only tools (Chromium, chromedriver, xvfb) are excluded on macOS. |

## Version pinning

The omp RPC protocol changes with every release, and `omp-rpc` is generated from it. The app MUST always run against the
exact omp version its client was built for.

| Item | Where | Current |
| --- | --- | --- |
| omp binary | `flake.nix` → `inputs.oh-my-pi.url = "github:can1357/oh-my-pi/v18.18.6"` (locked rev `9058ab9d229663e229e2867824ba6df6f21e9e59`) | 18.18.6 |
| `omp-rpc` crate | workspace `Cargo.toml` → `omp-rpc = { git = "https://github.com/can1357/oh-my-pi", tag = "v18.18.6" }` | 18.18.6 |
| Expected version in code | `omfp_core::OMP_VERSION` (DMN-001, contract tests) | `18.18.6` |
| Rust | `rust-toolchain.toml` | 1.99.1 |

| ID | Requirement |
| --- | --- |
| ENV-010 | `cargo xtask check-versions` (part of `just ci`) MUST check that the oh-my-pi tag and commit in `flake.lock`, the `omp-rpc` tag and commit in `Cargo.lock`, `omfp_core::OMP_VERSION`, and `omp --version` all agree. |
| ENV-011 | omp MUST be pinned to a release tag, never to `main` or an untagged commit. |

ENV-012 (the `wasm-bindgen` and `dioxus` pins agree with the shell's `wasm-bindgen-cli` and `dx`) was retired in M3
with the Dioxus UI (D-18); the ID is not reused.

## omp upgrade playbook

| ID | Requirement |
| --- | --- |
| ENV-020 | Upgrading omp MUST follow these steps, in one change: |

1. Pick the new release tag (`https://github.com/can1357/oh-my-pi/releases`).
2. Read the upstream diff of `docs/rpc.md` and `packages/coding-agent/CHANGELOG.md` between the two tags.
3. Update `inputs.oh-my-pi.url` and run `nix flake lock --update-input oh-my-pi`.
4. Update the `omp-rpc` tag and `omfp_core::OMP_VERSION`; run `cargo update -p omp-rpc`.
5. Re-record contract fixtures (`OMFP_RECORD_FIXTURES=1 just test-contract`) and review their diff: new event kinds,
   renamed fields, changed tool argument shapes (TST-012, UI-051).
6. Re-sync the vendored live prompt `crates/omfp-daemon/src/live_instructions.txt` from omp's
   `packages/coding-agent/src/live/prompts/live-instructions.md` and update the version named in
   `crates/omfp-daemon/src/live_context.rs` (DMN-105).
7. Fix what broke, then `just ci`.
