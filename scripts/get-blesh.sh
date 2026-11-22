#!/bin/sh
# Maintainer tool: unpack a ble.sh release into target/blesh/<ver>/ (the release tarball needs no
# build; building ble.sh from git needs gawk). Usage: scripts/get-blesh.sh [<version>]
# (default 0.4.0-devel3). Skips if already there. Nothing is installed outside target/.
set -eu

ver="${1:-0.4.0-devel3}"
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/target/blesh"
dest="$out/$ver"

if [ -f "$dest/ble.sh" ]; then
  echo "ble.sh $ver already there: $dest/ble.sh"
  exit 0
fi

# SHA-256 recorded at the first download (2026-11-23); GitHub publishes no digest for this asset.
case "$ver" in
  0.4.0-devel3) sha=c8612ee612bc6b10dbfd6e85c6cbdfd7caf152a12d1f9de22ea0a9d735b3080c ;;
  *) sha="" ;;
esac

mkdir -p "$out/src/dl"
tarball="$out/src/dl/ble-$ver.tar.xz"
if [ ! -f "$tarball" ]; then
  curl -fsSL --retry 3 -o "$tarball.part" \
    "https://github.com/akinomyoga/ble.sh/releases/download/v$ver/ble-$ver.tar.xz"
  mv "$tarball.part" "$tarball"
fi
got="$(shasum -a 256 "$tarball" | cut -d' ' -f1)"
echo "sha256 ble-$ver.tar.xz: $got"
if [ -n "$sha" ]; then
  [ "$got" = "$sha" ] || { echo "sha256 mismatch for ble-$ver: $got != $sha" >&2; rm -f "$tarball"; exit 1; }
  echo "sha256 matches the pinned value"
fi

rm -rf "$dest" "$out/src/ble-$ver"
xz -dc "$tarball" | tar -x -C "$out/src" -f -
mv "$out/src/ble-$ver" "$dest"
[ -f "$dest/ble.sh" ] || { echo "ble.sh $ver: no ble.sh in the tarball" >&2; rm -rf "$dest"; exit 1; }
echo "unpacked: $dest/ble.sh"
