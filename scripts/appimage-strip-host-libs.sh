#!/usr/bin/env bash
# Remove host-only libraries from a built AppImage, in place.
#
#   scripts/appimage-strip-host-libs.sh path/to/Stroke.AppImage
#
# linuxdeploy bundles the build machine's Wayland client libraries (Ubuntu
# 22.04: 1.20). The host's Mesa EGL driver is built against the HOST's Wayland
# and needs its newer symbols, so on current Arch/Fedora/etc. it fails to load
# (`libEGL_mesa.so.0: undefined symbol: wl_fixes_interface`) and
# WebKitWebProcess aborts with "Could not create surfaceless EGL display".
# The AppImage community excludelist leaves libwayland-client out for exactly
# this reason. Only the client goes: libwayland-server must stay bundled, since
# WebKit links it and Fedora, RHEL and Ubuntu 24.04 don't install it by default.
# Tested on Arch, Fedora 41/45, Debian 12/13 and Ubuntu 22.04/24.04.
# Issue: https://github.com/stroke-app/stroke/issues/112
#
# The image is repacked with its own runtime and the same squashfs settings,
# so nothing else about the AppImage changes. Needs squashfs-tools.

set -euo pipefail

APP="${1:?usage: $0 path/to/App.AppImage}"
APP="$(realpath "$APP")"

# Never bundled: the host's copies must be used.
EXCLUDE=(
  'libwayland-client.so*'
)

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

offset="$("$APP" --appimage-offset)"
head -c "$offset" "$APP" > "$work/runtime"

# --appimage-extract needs no FUSE; it writes ./squashfs-root.
(cd "$work" && "$APP" --appimage-extract > /dev/null)
root="$work/squashfs-root"

find_excluded() {
  local args=()
  for p in "${EXCLUDE[@]}"; do args+=(-name "$p" -o); done
  unset 'args[${#args[@]}-1]'
  find "$root" \( -type f -o -type l \) \( "${args[@]}" \) "$@"
}

removed="$(find_excluded -print | sed "s|^$root/||" | sort)"
if [ -z "$removed" ]; then
  echo "appimage: no host-only libraries bundled, nothing to do"
  exit 0
fi
find_excluded -delete
echo "appimage: removed"
printf '  %s\n' $removed

if [ -n "$(find_excluded -print)" ]; then
  echo "appimage: host-only libraries still present" >&2
  exit 1
fi

# Same compression and block size linuxdeploy's appimagetool uses.
mksquashfs "$root" "$work/fs.squashfs" -root-owned -noappend -comp zstd -b 131072 -no-progress -quiet
cat "$work/runtime" "$work/fs.squashfs" > "$work/out.AppImage"
chmod 755 "$work/out.AppImage"

# Sanity: the repacked image still extracts and holds the app.
check="$work/check"
mkdir "$check"
(cd "$check" && "$work/out.AppImage" --appimage-extract > /dev/null)
[ -x "$check/squashfs-root/AppRun" ] || { echo "appimage: repacked image is broken" >&2; exit 1; }

mv "$work/out.AppImage" "$APP"
echo "appimage: repacked $(basename "$APP") ($(du -h "$APP" | cut -f1))"
