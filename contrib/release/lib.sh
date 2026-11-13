# Shared by package-linux.sh and package-macos.sh (ENV-060..ENV-063); sourced, inside the dev shell.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cache="$root/target/release-cache"

die() {
    echo "error: $*" >&2
    exit 1
}

# The workspace version; with OMFP_RELEASE_TAG set (the release workflow on a tag), the tag must be `v<version>`
# (ENV-060), so the binary's `--version`, the Info.plist and the release agree.
release_version() {
    local version
    version=$(cargo metadata --no-deps --format-version 1 --manifest-path "$root/Cargo.toml" |
        jq -r '.packages[] | select(.name == "omfp") | .version')
    if [[ -n ${OMFP_RELEASE_TAG:-} && $OMFP_RELEASE_TAG != "v$version" ]]; then
        die "the tag $OMFP_RELEASE_TAG is not v$version, the workspace version (Cargo.toml)"
    fi
    echo "$version"
}

# The release build of the `omfp` crate into target/package (apart from the dev builds): optimized, stripped, std's
# source paths (the toolchain's store path) remapped; `$1` is the cargo command (`build`, `zigbuild`), extra cargo
# arguments follow.
build_omfp() {
    local command=$1 sysroot
    shift
    sysroot=$(rustc --print sysroot)
    RUSTFLAGS="--remap-path-prefix=$sysroot=/rustc" CARGO_PROFILE_RELEASE_STRIP=symbols \
        cargo "$command" --manifest-path "$root/Cargo.toml" --release --locked -p omfp \
        --target-dir "$root/target/package" "$@"
}

# ENV-061: the pinned omp's release asset `$1` (e.g. omp-linux-x64) from github.com/can1357/oh-my-pi, checked against
# contrib/release/omp.sha256, installed as the executable `$2`. Downloads are kept in target/release-cache.
fetch_omp() {
    local asset=$1 dest=$2 tag line sha file
    tag=$(jq -r '.nodes."oh-my-pi".original.ref' "$root/flake.lock")
    line=$(grep -E "^$tag $asset [0-9a-f]{64}\$" "$root/contrib/release/omp.sha256") ||
        die "contrib/release/omp.sha256 has no hash of $asset $tag (ENV-020)"
    sha=${line##* }
    file="$cache/$tag/$asset"
    if [[ ! -f $file ]] || ! echo "$sha  $file" | sha256sum --check --status; then
        mkdir -p "$cache/$tag"
        curl -fsSL --retry 3 -o "$file.part" "https://github.com/can1357/oh-my-pi/releases/download/$tag/$asset"
        echo "$sha  $file.part" | sha256sum --check --status || die "$asset $tag does not match its pinned sha256"
        mv "$file.part" "$file"
    fi
    install -m 0755 "$file" "$dest"
}

# The release web bundle (`just build-web`, DMN-008) into the absolute directory `$1`.
build_web() {
    just --justfile "$root/justfile" build-web "$1"
}

# `$1` as an absolute path, created as a directory.
out_dir() {
    mkdir -p "$1"
    (cd "$1" && pwd)
}
