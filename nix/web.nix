# ENV-031: the release web bundle (`just build-web`, `bun run build` in ui/) as a derivation. The JS dependencies come
# from the same Bun install cache as the dev shell (`bunDeps`, built from ui/bun.nix); `bun install` links the
# `@omfp/*` workspace packages from the source.
{
  lib,
  stdenvNoCC,
  bun2nix,
  bunDeps,
  version,
}:
stdenvNoCC.mkDerivation {
  pname = "omfp-web";
  inherit version bunDeps;

  # build.ts reads the favicons from docs/brand (index-html.ts); everything else lives in ui/.
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../ui
      ../docs/brand/icon-32.png
      ../docs/brand/icon.svg
    ];
  };

  nativeBuildInputs = [ bun2nix.hook ];

  bunRoot = "ui";
  # As `just ui-install`: the lockfile as committed, nothing from the network.
  bunInstallFlags = [
    "--frozen-lockfile"
    "--offline"
  ];
  # No dependency needs its install scripts to build the bundle.
  dontRunLifecycleScripts = true;

  buildPhase = ''
    runHook preBuild
    (cd ui && bun run build)
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    cp -r ui/packages/app/dist "$out"
    runHook postInstall
  '';

  dontFixup = true;
}
