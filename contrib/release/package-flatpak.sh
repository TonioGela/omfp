#!/usr/bin/env bash
# ENV-065: the Linux desktop download, `omfp-<version>-linux-x86_64.flatpak` (a single-file bundle) in the directory
# `$1`: contrib/flatpak/io.github.aciceri.omfp.yml on the GNOME 51 runtime, with the desktop build of `omfp` (linked in
# the dev shell against the runtime's glibc 2.44 and WebKitGTK 4.1 sonames, then pointed at the runtime's loader), the
# pinned omp's release build (ENV-061) and the web bundle. flatpak-builder and its runtimes use a Flatpak installation
# of their own, target/flatpak/user (or FLATPAK_USER_DIR when set), never the user's. Inside the dev shell on x86_64
# Linux (`just package-flatpak`).
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || die "package-flatpak.sh builds on x86_64 Linux"
app=io.github.aciceri.omfp
flathub=https://dl.flathub.org/repo/flathub.flatpakrepo

out=$(out_dir "${1:?usage: package-flatpak.sh <out-dir>}")
version=$(release_version)
work="$root/target/flatpak"
export FLATPAK_USER_DIR="${FLATPAK_USER_DIR:-$work/user}"
stage="$work/src"
rm -rf "$stage" "$work/build" "$work/repo"
mkdir -p "$stage/files"
cp -R "$root/contrib/flatpak/." "$stage/"

build_omfp build --features desktop
install -m 0755 "$root/target/package/release/omfp" "$stage/files/omfp"
# The runtime's loader and libraries, not the dev shell's store paths.
patchelf --set-interpreter /lib64/ld-linux-x86-64.so.2 --remove-rpath "$stage/files/omfp"
fetch_omp omp-linux-x64 "$stage/files/omp"
build_web "$stage/files/web"
cp "$root/crates/omfp/icons/icon.png" "$stage/files/icon.png"

flatpak remote-add --user --if-not-exists flathub "$flathub"
flatpak-builder --user --install-deps-from=flathub --disable-rofiles-fuse --force-clean \
    --state-dir="$work/state" --repo="$work/repo" "$work/build" "$stage/$app.yml"
bundle="$out/omfp-$version-linux-x86_64.flatpak"
flatpak build-bundle --runtime-repo="$flathub" "$work/repo" "$bundle" "$app"

# The bundle installs and its binary starts in the sandbox (every library it links is in the runtime).
flatpak install --user --noninteractive --reinstall --bundle "$bundle"
flatpak run --user --command=/app/lib/omfp/omfp "$app" --version
echo "$bundle"
