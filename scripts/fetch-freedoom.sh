#!/bin/bash
# This program is free software: you can redistribute it and/or modify it under the terms of
# the GNU General Public License as published by the Free Software Foundation, either version 3
# of the License, or (at your option) any later version.
#
# This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
# without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See
# the GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License along with this program.
# If not, see <https://www.gnu.org/licenses/>.

# fetch-freedoom.sh: the game's data, Freedoom's freedoom2.wad (Freedoom: Phase 2, a DOOM II IWAD,
# BSD licensed), from Freedoom's release on GitHub. The WAD is never in git (.gitignore).
#
#   scripts/fetch-freedoom.sh [dir]     # -> <dir>/freedoom2.wad and <dir>/COPYING.txt
#                                       #    (default: wad/ in this repository)
#
# Into wad/ (the default) it also writes wad/Android.bp: the module doom_app_freedoom2_wad, which
# installs the WAD read-only at /vendor/etc/doom/freedoom2.wad, doom_app's default. So the WAD is
# in the image only when it was fetched before the build, and only when the product lists the
# module (PRODUCT_PACKAGES) or it is built by name (m doom_app_freedoom2_wad).
#
# The zip is checked against its size and SHA-256 below before anything is unpacked: the engine
# reads the WAD with C code of the 1990s, so only the released file goes in. GitHub publishes no
# digest for this asset: the hash is the one recorded when the zip was first downloaded for this
# repository (2026-11-17). FREEDOOM_SHA256 and FREEDOOM_SIZE override them (another release).
# FREEDOOM_ZIP=<file>: a zip downloaded before, checked the same way, instead of a download.
set -euo pipefail
VERSION=0.13.0
URL="https://github.com/freedoom/freedoom/releases/download/v$VERSION/freedoom-$VERSION.zip"
SHA256=${FREEDOOM_SHA256:-3f9b264f3e3ce503b4fb7f6bdcb1f419d93c7b546f4df3e874dd878db9688f59}
SIZE=${FREEDOOM_SIZE:-24143781}
HERE=$(cd "$(dirname "$0")/.." && pwd)
OUT=${1:-$HERE/wad}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
if [ -n "${FREEDOOM_ZIP:-}" ]; then
    echo ">> $FREEDOOM_ZIP"
    cp "$FREEDOOM_ZIP" "$work/freedoom.zip"
else
    echo ">> $URL"
    curl -fL --proto '=https' --tlsv1.2 --max-filesize 100000000 -o "$work/freedoom.zip" "$URL"
fi
size=$(stat -c %s "$work/freedoom.zip")
[ "$size" = "$SIZE" ] || { echo "!! $size bytes, expected $SIZE: not unpacked" >&2; exit 1; }
got=$(sha256sum "$work/freedoom.zip" | cut -d' ' -f1)
[ "$got" = "$SHA256" ] || { echo "!! SHA-256 $got, expected $SHA256: not unpacked" >&2; exit 1; }
# only the two files, by their names in the zip; nothing else of it is written
unzip -q -j -o "$work/freedoom.zip" "freedoom-$VERSION/freedoom2.wad" "freedoom-$VERSION/COPYING.txt" -d "$work/out"
mkdir -p "$OUT"
install -m 0644 "$work/out/freedoom2.wad" "$work/out/COPYING.txt" "$OUT/"
if [ "$OUT" = "$HERE/wad" ]; then
    cat > "$OUT/Android.bp" <<'BP'
// Written by scripts/fetch-freedoom.sh, not in git: Freedoom's WAD in the image, read-only, at
// doom_app's default /vendor/etc/doom/freedoom2.wad (the vendor partition: the game's SELinux
// domain may read it there).

package {
    default_applicable_licenses: ["doom_app_freedoom_license"],
}

// Freedoom's data: BSD-3-Clause (COPYING.txt of the release)
license {
    name: "doom_app_freedoom_license",
    visibility: [":__subpackages__"],
    license_kinds: ["SPDX-license-identifier-BSD-3-Clause"],
    license_text: ["COPYING.txt"],
}

prebuilt_etc {
    name: "doom_app_freedoom2_wad",
    src: "freedoom2.wad",
    sub_dir: "doom",
    filename: "freedoom2.wad",
    vendor: true,
}
BP
fi
ls -la "$OUT"/freedoom2.wad "$OUT"/COPYING.txt
