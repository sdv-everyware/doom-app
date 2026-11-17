#!/bin/bash
# fetch-freedoom.sh: the game's data, Freedoom's freedoom2.wad (Freedoom: Phase 2, a DOOM II IWAD,
# BSD licensed), from Freedoom's release on GitHub. The WAD is never in git (.gitignore): run this
# where the WAD is needed, then push it to the device (README.md, "Run").
#
#   scripts/fetch-freedoom.sh [dir]     # -> <dir>/freedoom2.wad and <dir>/COPYING.txt
#                                       #    (default: wad/ in this repository)
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
ls -la "$OUT/freedoom2.wad" "$OUT/COPYING.txt"
