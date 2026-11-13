#!/usr/bin/env bash
# ENV-062: the Linux x86_64 download, `omfp-<version>-linux-x86_64.tar.gz` in the directory `$1`: the daemon-only
# `omfp` (no desktop window, no WebKitGTK), the pinned omp next to it (DMN-006, ENV-061) and the web bundle in `web/`
# (DMN-008). It runs on any x86_64 glibc system (glibc ≥ 2.34) without Nix: cargo-zigbuild links it against glibc
# 2.34's symbols (Nix's glibc would require newer ones, weak references included, which older systems' loaders warn
# about at every start), with the system's `/lib64/ld-linux-x86-64.so.2` and no RUNPATH. Inside the dev shell (`just
# package-linux`).
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || die "package-linux.sh builds on x86_64 Linux"
# The oldest glibc the binary runs on: Ubuntu 22.04, Debian 12, RHEL 9.
glibc_floor=2.34

out=$(out_dir "${1:?usage: package-linux.sh <out-dir>}")
version=$(release_version)
name="omfp-$version-linux-x86_64"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
dir="$stage/$name"
mkdir -p "$dir"

build_omfp zigbuild --target "x86_64-unknown-linux-gnu.$glibc_floor"
install -m 0755 "$root/target/package/x86_64-unknown-linux-gnu/release/omfp" "$dir/omfp"
[[ $(patchelf --print-interpreter "$dir/omfp") == /lib64/ld-linux-x86-64.so.2 && -z $(patchelf --print-rpath "$dir/omfp") ]] ||
    die "omfp must use the system's loader and no RUNPATH"
glibc=$(objdump -p "$dir/omfp" | awk '$4 ~ /^GLIBC_/ { print substr($4, 7) }' | sort -V | tail -n 1)
[[ $(printf '%s\n' "$glibc" "$glibc_floor" | sort -V | tail -n 1) == "$glibc_floor" ]] ||
    die "omfp needs glibc $glibc, newer than $glibc_floor"
if patchelf --print-needed "$dir/omfp" | grep -v -E '^(libc|libm|libgcc_s|ld-linux-x86-64)\.so\.[0-9]+$'; then
    die "omfp links a library a glibc system may not have"
fi

fetch_omp omp-linux-x64 "$dir/omp"
build_web "$dir/web"
cp "$root/LICENSE" "$dir/LICENSE"
cat > "$dir/README.txt" <<EOF
omfp $version for x86_64 Linux (glibc $glibc_floor or newer), without the desktop window.

  ./omfp daemon        the daemon and the web UI on 127.0.0.1:7420; it prints the URL with its token
  ./omfp --help        every command and option

omp (the pinned oh-my-pi) and the web UI (web/) are found next to omfp; keep the three together. git must be on PATH.
https://github.com/aciceri/omfp
EOF

# Reproducible archive: sorted entries, no owner, the commit's time.
mtime=$(git -C "$root" log -1 --format=%ct)
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@$mtime" -C "$stage" -czf "$out/$name.tar.gz" "$name"
echo "$out/$name.tar.gz"
