{
  description = "pmo: native + web GUI for oh-my-pi (Dioxus) — development environment only";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # omp pinned to an upstream release tag (never main).
    # Must match the `tag` of the `omp-rpc` git dependency in Cargo.toml.
    # No `follows`: omp builds with the nixpkgs/toolchain tested upstream.
    oh-my-pi.url = "github:can1357/oh-my-pi/v18.18.6";

    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [ inputs.git-hooks.flakeModule ];

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      perSystem =
        {
          config,
          system,
          lib,
          ...
        }:
        let
          pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.rust-overlay.overlays.default ];
          };

          rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          omp = inputs.oh-my-pi.packages.${system}.omp;

          # nixpkgs' dioxus-cli 0.7.10 fails to build: zstd-sys cannot find libzstd via pkg-config.
          dioxus-cli = pkgs.dioxus-cli.overrideAttrs (old: {
            buildInputs = (old.buildInputs or [ ]) ++ [ pkgs.zstd ];
          });

          # Native dependencies of dioxus-desktop (wry/webkitgtk) on Linux.
          desktopLibs = with pkgs; [
            gtk3
            webkitgtk_4_1
            libsoup_3
            glib
            glib-networking
            cairo
            pango
            gdk-pixbuf
            atk
            xdotool
            openssl
          ];
        in
        {
          formatter = pkgs.nixfmt;

          # Hooks run on commit (installed by the dev shell) and as `checks.<system>.pre-commit`.
          pre-commit.settings = {
            hooks = {
              # Nix
              nixfmt.enable = true;
              deadnix.enable = true;
              statix.enable = true;
              # Rust (runs only once .rs files exist)
              rustfmt = {
                enable = true;
                packageOverrides = {
                  cargo = rustToolchain;
                  rustfmt = rustToolchain;
                };
              };
              # TOML, Markdown, spelling
              taplo.enable = true;
              markdownlint = {
                enable = true;
                settings.configuration = {
                  MD013 = false; # line length: tables and long requirement rows
                  MD024.siblings_only = true; # repeated headings across sections
                };
              };
              typos.enable = true;
              # Generic hygiene
              check-merge-conflicts.enable = true;
              check-toml.enable = true;
              check-yaml.enable = true;
              end-of-file-fixer.enable = true;
              trim-trailing-whitespace.enable = true;
              # Conventional Commits on the commit message
              # (commitizen instead of convco: nixpkgs' convco 0.7.2 fails to build against libgit2.)
              commitizen.enable = true;
            };
          };

          devShells.default = pkgs.mkShell {
            packages = [
              # Agent (version pinned by the oh-my-pi input)
              omp

              # Rust + Dioxus
              rustToolchain
              dioxus-cli
              pkgs.wasm-bindgen-cli # must match the wasm-bindgen crate version (see docs/specs/01)
              pkgs.binaryen
              pkgs.pkg-config

              # Testing (TDD): runner, snapshots, coverage, mutation, dependency lint
              pkgs.cargo-nextest
              pkgs.cargo-insta
              pkgs.cargo-llvm-cov
              pkgs.cargo-mutants
              pkgs.cargo-deny
              pkgs.bacon

              # Utilities
              pkgs.just
              pkgs.git
              pkgs.jq
              pkgs.websocat
              pkgs.nixfmt
            ]
            ++ config.pre-commit.settings.enabledPackages
            ++ lib.optionals pkgs.stdenv.isLinux (
              desktopLibs
              ++ [
                # Web UI E2E over WebDriver (versions aligned in nixpkgs)
                pkgs.chromium
                pkgs.chromedriver
                # Desktop smoke test under a virtual X server
                pkgs.xvfb-run
              ]
            );

            env = {
              RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
            }
            // lib.optionalAttrs pkgs.stdenv.isLinux {
              # webkitgtk on NixOS/non-Intel GPUs: avoid blank windows from the DMABUF renderer.
              WEBKIT_DISABLE_DMABUF_RENDERER = "1";
              GIO_MODULE_DIR = "${pkgs.glib-networking}/lib/gio/modules";
              CHROME_BIN = lib.getExe pkgs.chromium;
            };

            shellHook = ''
              ${config.pre-commit.installationScript}
              echo "pmo dev shell — $(omp --version 2>/dev/null | head -n1), $(rustc --version), dx $(dx --version 2>/dev/null)"
            '';
          };
        };
    };
}
