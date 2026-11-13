#!/usr/bin/env bash
# ENV-063: the macOS arm64 download, `omfp-<version>-macos-arm64.dmg` and `.zip` in the directory `$1`: `omfp.app`
# without a Nix store path:
#
#   Contents/Info.plist          crates/omfp/Info.plist with the version (bundle id, NSMicrophoneUsageDescription)
#   Contents/MacOS/omfp          the desktop build; run without arguments it opens `omfp desktop`
#   Contents/MacOS/omp           the pinned omp's release build (DMN-006, ENV-061)
#   Contents/Resources/web/      the web bundle (DMN-008)
#   Contents/Resources/omfp.icns the icon
#
# Signed ad-hoc, or with a Developer ID under the hardened runtime when one is given, then notarized and stapled when
# the Apple ID is given too:
#
#   APPLE_CERTIFICATE (base64 .p12) + APPLE_CERTIFICATE_PASSWORD   imported into a temporary keychain (CI), or
#   OMFP_SIGN_IDENTITY                                              a Developer ID identity of your keychains (local)
#   APPLE_ID + APPLE_APP_PASSWORD (app-specific) + APPLE_TEAM_ID    notarization with notarytool
#
# Inside the dev shell on an arm64 Mac (`just package-macos`).
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

[[ $(uname -s) == Darwin && $(uname -m) == arm64 ]] || die "package-macos.sh builds on an arm64 Mac"

out=$(out_dir "${1:?usage: package-macos.sh <out-dir>}")
version=$(release_version)
name="omfp-$version-macos-arm64"
stage=$(mktemp -d)
keychain=""
keychains=()
cleanup() {
    if [[ -n $keychain ]]; then
        /usr/bin/security list-keychains -d user -s "${keychains[@]}"
        /usr/bin/security delete-keychain "$keychain"
    fi
    rm -rf "$stage"
}
trap cleanup EXIT

build_omfp build --features desktop
bin="$root/target/package/release/omfp"
web="$stage/web"
build_web "$web"
# Apple's tools from here on, not the SDK of the dev shell's compiler.
unset DEVELOPER_DIR SDKROOT

app="$stage/omfp.app"
contents="$app/Contents"
mkdir -p "$contents/MacOS" "$contents/Resources"
install -m 0755 "$bin" "$contents/MacOS/omfp"
fetch_omp omp-darwin-arm64 "$contents/MacOS/omp"
cp -R "$web" "$contents/Resources/web"
sed "s/@VERSION@/$version/g" "$root/crates/omfp/Info.plist" > "$contents/Info.plist"
/usr/bin/plutil -lint "$contents/Info.plist" > /dev/null

iconset="$stage/omfp.iconset"
mkdir "$iconset"
for size in 16 32 128 256 512; do
    /usr/bin/sips -z "$size" "$size" "$root/crates/omfp/icons/icon.png" --out "$iconset/icon_${size}x${size}.png" > /dev/null
    if ((size < 512)); then
        double=$((size * 2))
        /usr/bin/sips -z "$double" "$double" "$root/crates/omfp/icons/icon.png" \
            --out "$iconset/icon_${size}x${size}@2x.png" > /dev/null
    fi
done
/usr/bin/iconutil -c icns "$iconset" -o "$contents/Resources/omfp.icns"

# The dev shell's linker points the binary at nixpkgs' libiconv: the system's has the same ABI (macOS ships it).
while read -r lib; do
    case $lib in
        /nix/store/*/lib/libiconv.2.dylib) /usr/bin/install_name_tool -change "$lib" /usr/lib/libiconv.2.dylib "$contents/MacOS/omfp" ;;
        /nix/store/*) die "omfp links $lib, which macOS does not ship" ;;
    esac
done < <(/usr/bin/otool -L "$contents/MacOS/omfp" | awk 'NR > 1 { print $1 }')
for code in "$contents/MacOS/omfp" "$contents/MacOS/omp"; do
    if /usr/bin/otool -l "$code" | grep -q /nix/store || /usr/bin/otool -L "$code" | grep -q /nix/store; then
        die "$code refers to /nix/store"
    fi
done

identity=${OMFP_SIGN_IDENTITY:-}
if [[ -n ${APPLE_CERTIFICATE:-} ]]; then
    keychain="$stage/signing.keychain-db"
    password=$(/usr/bin/uuidgen)
    read -r -a keychains < <(/usr/bin/security list-keychains -d user | tr -d '"' | tr '\n' ' ')
    /usr/bin/security create-keychain -p "$password" "$keychain"
    /usr/bin/security set-keychain-settings -lut 21600 "$keychain"
    /usr/bin/security unlock-keychain -p "$password" "$keychain"
    echo "$APPLE_CERTIFICATE" | base64 --decode > "$stage/certificate.p12"
    /usr/bin/security import "$stage/certificate.p12" -k "$keychain" -P "${APPLE_CERTIFICATE_PASSWORD:?}" \
        -T /usr/bin/codesign
    /usr/bin/security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" > /dev/null
    /usr/bin/security list-keychains -d user -s "$keychain" "${keychains[@]}"
    identity=$(/usr/bin/security find-identity -v -p codesigning "$keychain" |
        awk '/Developer ID Application/ { print $2; exit }')
    [[ -n $identity ]] || die "APPLE_CERTIFICATE holds no Developer ID Application identity"
fi

# Nested code first (omp), then the bundle, whose signature seals the executable, the Info.plist and the resources.
if [[ -n $identity ]]; then
    sign=(/usr/bin/codesign --force --timestamp --options runtime --sign "$identity")
    "${sign[@]}" --entitlements "$root/contrib/release/omp.entitlements" "$contents/MacOS/omp"
    "${sign[@]}" --entitlements "$root/contrib/release/omfp.entitlements" "$app"
else
    echo "warning: no Developer ID: signing ad-hoc (Gatekeeper asks the user to allow the app)" >&2
    /usr/bin/codesign --force --sign - "$contents/MacOS/omp"
    /usr/bin/codesign --force --sign - "$app"
fi
/usr/bin/codesign --verify --deep --strict "$app"
"$contents/MacOS/omfp" --version > /dev/null
"$contents/MacOS/omp" --version > /dev/null

notarizing=""
if [[ -n $identity && -n ${APPLE_ID:-} && -n ${APPLE_APP_PASSWORD:-} && -n ${APPLE_TEAM_ID:-} ]]; then
    notarizing=1
elif [[ -n $identity ]]; then
    echo "warning: APPLE_ID, APPLE_APP_PASSWORD or APPLE_TEAM_ID missing: signed, not notarized" >&2
fi
notarize() {
    /usr/bin/xcrun notarytool submit "$1" --apple-id "$APPLE_ID" --password "$APPLE_APP_PASSWORD" \
        --team-id "$APPLE_TEAM_ID" --wait
}

# The app is notarized and stapled first, so the copy a user drags out of the disk image carries its ticket.
if [[ -n $notarizing ]]; then
    /usr/bin/ditto -c -k --keepParent "$app" "$stage/notarize.zip"
    notarize "$stage/notarize.zip"
    /usr/bin/xcrun stapler staple "$app"
fi
rm -f "$out/$name.zip" "$out/$name.dmg"
/usr/bin/ditto -c -k --keepParent "$app" "$out/$name.zip"

# The disk image: the app and a link to /Applications to drag it onto.
volume="$stage/volume"
mkdir "$volume"
/usr/bin/ditto "$app" "$volume/omfp.app"
ln -s /Applications "$volume/Applications"
/usr/bin/hdiutil create -quiet -volname "omfp $version" -srcfolder "$volume" -fs HFS+ -format UDZO "$out/$name.dmg"
if [[ -n $identity ]]; then
    /usr/bin/codesign --force --timestamp --sign "$identity" "$out/$name.dmg"
fi
if [[ -n $notarizing ]]; then
    notarize "$out/$name.dmg"
    /usr/bin/xcrun stapler staple "$out/$name.dmg"
fi
echo "$out/$name.dmg"
echo "$out/$name.zip"
