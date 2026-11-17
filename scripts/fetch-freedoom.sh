#!/bin/bash
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# fetch-freedoom.sh: the game's data, Freedoom's freedoom2.wad (Freedoom: Phase 2, a DOOM II IWAD,
# BSD licensed), from Freedoom's release on GitHub. The WAD is never in git (.gitignore).
#
#   scripts/fetch-freedoom.sh [dir]     # -> <dir>/freedoom2.wad and <dir>/COPYING.txt
#                                       #    (default: wad/ in this repository)
#
# Into wad/ (the default) it also writes wad/Android.bp: the module doom_app_freedoom2_wad, which
# installs the WAD read-only at /product/etc/doom/freedoom2.wad, doom_app's default. So the WAD is
# in the image only when it was fetched before the build, and only when the product lists the
# module (PRODUCT_PACKAGES) or it is built by name (m doom_app_freedoom2_wad).
#
# The zip is checked against SHA256 below before anything is unpacked: the engine reads the WAD
# with C code of the 1990s, so only the released file goes in. An empty SHA256 (not pinned yet):
# the script prints the zip's hash and stops; compare it with the release's page, then set it.
set -euo pipefail
VERSION=0.13.0
URL="https://github.com/freedoom/freedoom/releases/download/v$VERSION/freedoom-$VERSION.zip"
SHA256=${FREEDOOM_SHA256:-}
HERE=$(cd "$(dirname "$0")/.." && pwd)
OUT=${1:-$HERE/wad}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
echo ">> $URL"
curl -fL --proto '=https' --tlsv1.2 --max-filesize 100000000 -o "$work/freedoom.zip" "$URL"
got=$(sha256sum "$work/freedoom.zip" | cut -d' ' -f1)
if [ -z "$SHA256" ]; then
    echo "!! the zip's SHA-256 is $got"
    echo "!! not pinned: check it against the release page, then set SHA256 in this script (or FREEDOOM_SHA256) and run again"
    exit 1
fi
[ "$got" = "$SHA256" ] || { echo "!! SHA-256 $got, expected $SHA256: not unpacked" >&2; exit 1; }
# only the two files, by their names in the zip; nothing else of it is written
unzip -q -j -o "$work/freedoom.zip" "freedoom-$VERSION/freedoom2.wad" "freedoom-$VERSION/COPYING.txt" -d "$work/out"
mkdir -p "$OUT"
install -m 0644 "$work/out/freedoom2.wad" "$work/out/COPYING.txt" "$OUT/"
if [ "$OUT" = "$HERE/wad" ]; then
    cat > "$OUT/Android.bp" <<'BP'
// Written by scripts/fetch-freedoom.sh, not in git: Freedoom's WAD in the image, read-only, at
// doom_app's default /product/etc/doom/freedoom2.wad.

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
    product_specific: true,
}
BP
fi
ls -la "$OUT"/freedoom2.wad "$OUT"/COPYING.txt
