#!/bin/sh
# Maintainer tool: build a fish from its release tarball into target/fish/<ver>/.
# Usage: scripts/build-fish.sh <version>     (3.0.2, 3.7.1: CMake and C++; 4.x: cargo). Skips if
# already built. Everything stays under the repo's git-ignored target/ (cargo's build folder
# too); nothing is installed system-wide.
set -eu

ver="${1:?usage: build-fish.sh <version>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/target/fish"
prefix="$out/$ver"
src="$out/src"

# "Built" means the binary starts with no config and knows the events the adapter uses.
complete_install() {
  [ -x "$prefix/bin/fish" ] &&
    HOME="$out" XDG_CONFIG_HOME="$out/.nocfg" XDG_DATA_HOME="$out/.nodata" \
      "$prefix/bin/fish" --no-config -c 'functions -q fish_prompt; or exit 1; function _t --on-event fish_postexec; end; status is-interactive; true' >/dev/null 2>&1
}
if complete_install; then
  echo "fish $ver already built: $prefix/bin/fish"
  exit 0
fi
rm -rf "$prefix"
trap 'rc=$?; [ "$rc" -eq 0 ] || rm -rf "$prefix"' EXIT

# 4.9.3: SHA-256 as published by GitHub for the release asset. 3.x: GitHub publishes no digest
# (only .asc signatures, and gpg was not available here), so the value recorded at the first
# download (2026-11-23) is pinned.
case "$ver" in
  3.0.2) file="fish-$ver.tar.gz"; sha=14728ccc6b8e053d01526ebbd0822ca4eb0235e6487e832ec1d0d22f1395430e ;;
  3.7.1) file="fish-$ver.tar.xz"; sha=614c9f5643cd0799df391395fa6bbc3649427bb839722ce3b114d3bbc1a3b250 ;;
  4.9.3) file="fish-$ver.tar.xz"; sha=20998a25f73217ddcc19f499055fd587e9912d1ad6e7109120fbcf2871f0b98c ;;
  *) case "$ver" in 3.0.*) file="fish-$ver.tar.gz" ;; *) file="fish-$ver.tar.xz" ;; esac; sha="" ;;
esac

mkdir -p "$src/dl"
tarball="$src/dl/$file"
if [ ! -f "$tarball" ]; then
  curl -fsSL --retry 3 -o "$tarball.part" \
    "https://github.com/fish-shell/fish-shell/releases/download/$ver/$file"
  mv "$tarball.part" "$tarball"
fi
got="$(shasum -a 256 "$tarball" | cut -d' ' -f1)"
echo "sha256 $file: $got"
if [ -n "$sha" ]; then
  [ "$got" = "$sha" ] || { echo "sha256 mismatch for $file: $got != $sha" >&2; rm -f "$tarball"; exit 1; }
  echo "sha256 matches the pinned value"
else
  echo "(no pinned value for $ver)"
fi

rm -rf "$src/fish-$ver"
case "$file" in
  *.tar.gz) tar -xzf "$tarball" -C "$src" ;;
  *) xz -dc "$tarball" | tar -x -C "$src" -f - ;;
esac
cd "$src/fish-$ver"
jobs="$(sysctl -n hw.ncpu 2>/dev/null || nproc)"

case "$ver" in
  3.*)
    # fish 3.x's cmake/Tests.cmake sets policy CMP0037 to OLD (its `test` target), which CMake 4
    # refuses. So 3.x configures with a CMake 3.31 of its own, unpacked under target/fish/ (macOS
    # universal binary; SHA-256 as published by GitHub for the release asset).
    cmver=3.31.12
    cmsha=799af7fd545db9bf1b9cfe72f8095880e727a2d4e0df0e3dffc3bc7b95c2d3b0
    cmdir="$out/cmake-$cmver"
    if [ ! -x "$cmdir/CMake.app/Contents/bin/cmake" ]; then
      cmtar="$src/dl/cmake-$cmver-macos-universal.tar.gz"
      [ -f "$cmtar" ] || { curl -fsSL --retry 3 -o "$cmtar.part" \
        "https://github.com/Kitware/CMake/releases/download/v$cmver/cmake-$cmver-macos-universal.tar.gz" \
        && mv "$cmtar.part" "$cmtar"; }
      [ "$(shasum -a 256 "$cmtar" | cut -d' ' -f1)" = "$cmsha" ] \
        || { echo "sha256 mismatch for cmake $cmver" >&2; rm -f "$cmtar"; exit 1; }
      rm -rf "$cmdir" && mkdir -p "$cmdir"
      tar -xzf "$cmtar" -C "$cmdir" --strip-components 1
    fi
    cmake="$cmdir/CMake.app/Contents/bin/cmake"
    "$cmake" -S . -B "$src/build-$ver" -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$prefix" \
      -DBUILD_DOCS=OFF -DWITH_GETTEXT=OFF \
      >"$src/configure-$ver.log" 2>&1 || { tail -30 "$src/configure-$ver.log" >&2; exit 1; }
    "$cmake" --build "$src/build-$ver" -j "$jobs" >"$src/make-$ver.log" 2>&1 \
      || { tail -30 "$src/make-$ver.log" >&2; exit 1; }
    "$cmake" --install "$src/build-$ver" >"$src/install-$ver.log" 2>&1 \
      || { tail -30 "$src/install-$ver.log" >&2; exit 1; }
    ;;
  *)
    # fish 4 is Rust; its build folder stays under target/fish/build.
    CARGO_TARGET_DIR="$out/build" cargo build --release --locked >"$src/make-$ver.log" 2>&1 \
      || { tail -30 "$src/make-$ver.log" >&2; exit 1; }
    mkdir -p "$prefix/bin"
    cp "$out/build/release/fish" "$prefix/bin/fish"
    ;;
esac
complete_install || { echo "fish $ver built but does not start" >&2; exit 1; }
echo "built: $("$prefix/bin/fish" --version) at $prefix/bin/fish"
