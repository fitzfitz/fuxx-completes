#!/bin/sh
# Maintainer tool: build a bash from the GNU source tarball into target/bash/<ver>/.
# Usage: scripts/build-bash.sh <version>     (e.g. 4.0, 5.3). Skips if already built.
# Everything stays under the repo's git-ignored target/; nothing is installed system-wide.
set -eu

ver="${1:?usage: build-bash.sh <version>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/target/bash"
prefix="$out/$ver"
src="$out/src"

# "Built" means the binary runs what the adapter needs: `bind -x` (in an interactive shell's
# readline) and a fractional `read -t`.
complete_install() {
  [ -x "$prefix/bin/bash" ] &&
    "$prefix/bin/bash" --norc --noprofile -c 'read -t 0.01 x </dev/zero; declare -f >/dev/null; help bind | grep -q -- "-x"' >/dev/null 2>&1
}
if complete_install; then
  echo "bash $ver already built: $prefix/bin/bash"
  exit 0
fi
rm -rf "$prefix"
trap 'rc=$?; [ "$rc" -eq 0 ] || rm -rf "$prefix"' EXIT

# SHA-256 of the GNU tarballs, recorded at their first download (2026-11-23). GNU publishes only
# .sig files, and gpg was not available here to check them, so a mismatch is what is caught.
case "$ver" in
  4.0) sha=9793d394f640a95030c77d5ac989724afe196921956db741bcaf141801c50518 ;;
  5.3) sha=0d5cd86965f869a26cf64f4b71be7b96f90a3ba8b3d74e27e8e9d9d5550f31ba ;;
  *) sha="" ;;
esac

mkdir -p "$src/dl"
tarball="$src/dl/bash-$ver.tar.gz"
if [ ! -f "$tarball" ]; then
  curl -fsSL --retry 3 -o "$tarball.part" "https://ftp.gnu.org/gnu/bash/bash-$ver.tar.gz"
  mv "$tarball.part" "$tarball"
fi
got="$(shasum -a 256 "$tarball" | cut -d' ' -f1)"
echo "sha256 bash-$ver.tar.gz: $got"
if [ -n "$sha" ]; then
  [ "$got" = "$sha" ] || { echo "sha256 mismatch for bash-$ver: $got != $sha" >&2; rm -f "$tarball"; exit 1; }
  echo "sha256 matches the pinned value"
else
  echo "(no pinned value for $ver)"
fi

rm -rf "$src/bash-$ver"
tar -xzf "$tarball" -C "$src"
cd "$src/bash-$ver"

# Old configure probes and sources fail on current clang (implicit declarations, implicit int,
# old-style prototypes); relax those errors, as scripts/build-zsh.sh does.
CFLAGS="-O2 -Wno-error=implicit-function-declaration -Wno-error=implicit-int -Wno-error=incompatible-function-pointer-types -Wno-error=int-conversion -std=gnu89"
case "$ver" in 5.*) CFLAGS="-O2 -Wno-error=implicit-function-declaration -Wno-error=implicit-int" ;; esac
export CFLAGS
./configure --prefix="$prefix" --without-bash-malloc --disable-nls >"$src/configure-$ver.log" 2>&1 \
  || { tail -30 "$src/configure-$ver.log" >&2; exit 1; }
jobs="$(sysctl -n hw.ncpu 2>/dev/null || nproc)"
# bash 4.0's build failed under -j here (2026-11-23) and succeeds serially: build 4.x serially.
case "$ver" in 4.*) jobs=1 ;; esac
make -j"$jobs" >"$src/make-$ver.log" 2>&1 \
  || { tail -30 "$src/make-$ver.log" >&2; exit 1; }
mkdir -p "$prefix/bin"
cp bash "$prefix/bin/bash"
complete_install || { echo "bash $ver built but does not run bind -x / read -t" >&2; exit 1; }
echo "built: $("$prefix/bin/bash" --norc -c 'echo $BASH_VERSION') at $prefix/bin/bash"
