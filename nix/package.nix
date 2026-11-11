# ENV-030..034: the installable `omfp`. `desktop = true` (Linux) builds the `desktop` feature (Tauri 2 on WebKitGTK
# 4.1, the tray over gio) wrapped by wrapGAppsHook3; `desktop = false` is the lean daemon-only build
# (`packages.omfp-headless`). Both ship the release web bundle as `bin/web` (DMN-008) and run the pinned omp from the
# wrapper's PATH (DMN-006).
{
  lib,
  stdenv,
  rustPlatform,
  makeWrapper,
  pkg-config,
  wrapGAppsHook3,
  copyDesktopItems,
  makeDesktopItem,
  webkitgtk_4_1,
  libsoup_3,
  glib-networking,
  gsettings-desktop-schemas,
  gitMinimal,
  sqlite,
  omp,
  web,
  desktop ? stdenv.hostPlatform.isLinux,
}:
let
  # The Rust workspace only: the UI, docs and fixtures do not change the binary.
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../crates
      ../xtask
      ../e2e
    ];
  };

  # The pinned omp first (DMN-001 refuses another version); git (without perl and python) last, so the user's own git
  # wins.
  pathArgs = [
    "--prefix"
    "PATH"
    ":"
    (lib.makeBinPath [ omp ])
    "--suffix"
    "PATH"
    ":"
    (lib.makeBinPath [ gitMinimal ])
  ];
in
rustPlatform.buildRustPackage {
  pname = if desktop then "omfp" else "omfp-headless";
  inherit (web) version;
  inherit src;

  cargoLock = {
    lockFile = ../Cargo.lock;
    # The omp-rpc git dependency (the oh-my-pi tag of ENV-010); changes with every omp upgrade (ENV-020).
    outputHashes."omp-rpc-0.1.0" = "sha256-wRNg5x4ARxFVhizkLc36td3Q/r/mSIFLqCeG0pJ7Hok=";
  };

  cargoBuildFlags = [
    "--package"
    "omfp"
  ];
  buildFeatures = lib.optional desktop "desktop";

  nativeBuildInputs = [
    pkg-config
  ]
  ++ (
    if desktop then
      [
        wrapGAppsHook3
        copyDesktopItems
      ]
    else
      [ makeWrapper ]
  );

  # nixpkgs' cargo setup hook sets LIBSQLITE3_SYS_USE_PKG_CONFIG=1: rusqlite links nixpkgs' sqlite instead of its
  # bundled copy. GTK3, GLib and cairo come through webkitgtk's .pc files; glib-networking is TLS for `omfp desktop
  # --connect https://…` (GIO_EXTRA_MODULES, set by the wrapper), gsettings-desktop-schemas the GTK settings schemas.
  buildInputs = [
    sqlite
  ]
  ++ lib.optionals desktop [
    webkitgtk_4_1
    libsoup_3
    glib-networking
    gsettings-desktop-schemas
  ];

  # `just ci` runs the tests (several need omp, a browser or a display).
  doCheck = false;

  desktopItems = lib.optionals desktop [
    (makeDesktopItem {
      name = "omfp";
      desktopName = "omfp";
      comment = "Web and desktop GUI for the omp coding agent";
      exec = "omfp desktop";
      icon = "omfp";
      categories = [ "Development" ];
    })
  ];

  # DMN-008: the bundle is `web/` next to the executable; the wrappers keep `bin/` as the real binary's directory
  # (`bin/.omfp-wrapped`). A symlink, which the wrappers do not try to wrap.
  postInstall = ''
    mkdir -p "$out/share/omfp"
    ln -s ${web} "$out/share/omfp/web"
    ln -s ../share/omfp/web "$out/bin/web"
  ''
  + lib.optionalString desktop ''
    install -Dm644 crates/omfp/icons/icon.png "$out/share/icons/hicolor/512x512/apps/omfp.png"
  '';

  preFixup = lib.optionalString desktop ''
    gappsWrapperArgs+=(${lib.escapeShellArgs pathArgs})
  '';

  postFixup = lib.optionalString (!desktop) ''
    wrapProgram "$out/bin/omfp" ${lib.escapeShellArgs pathArgs}
  '';

  meta = {
    description = "Web and desktop GUI for the omp coding agent (oh-my-pi)";
    license = lib.licenses.gpl3Plus;
    mainProgram = "omfp";
    platforms = lib.platforms.linux ++ lib.platforms.darwin;
  };
}
