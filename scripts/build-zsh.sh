#!/bin/sh
# Maintainer tool: build an old zsh from the official source tarball into target/zsh/<ver>/.
# Usage: scripts/build-zsh.sh <version>     (e.g. 5.3.1, 5.8.1). Skips if already built.
# Everything stays under the repo's git-ignored target/; nothing is installed system-wide.
set -eu

ver="${1:?usage: build-zsh.sh <version>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/target/zsh"
prefix="$out/$ver"
src="$out/src"

# "Built" means the install is complete: the binary runs and loads every module the adapter and
# its tests need. A half-installed prefix (binary there, modules missing) is wiped and rebuilt.
# (The prefix is compiled into zsh, so it can't be installed elsewhere and renamed.)
complete_install() {
  [ -x "$prefix/bin/zsh" ] &&
    "$prefix/bin/zsh" -fc 'zmodload zsh/zleparameter zsh/parameter zsh/datetime zsh/system && zmodload -F zsh/files b:zf_rm b:zf_rmdir b:zf_mkdir && autoload -Uz is-at-least add-zsh-hook add-zle-hook-widget && whence -w is-at-least >/dev/null' >/dev/null 2>&1
}
if complete_install; then
  echo "zsh $ver already built: $prefix/bin/zsh"
  exit 0
fi
rm -rf "$prefix"
# Any failure from here on leaves no prefix behind, so a failed install is never taken as built.
trap 'rc=$?; [ "$rc" -eq 0 ] || rm -rf "$prefix"' EXIT

# MD5 as published by the zsh project (https://www.zsh.org/pub/old/MD5SUM); compared in addition
# to printing the SHA-256. gpg was not available when this was written, so the .asc is not checked.
case "$ver" in
  5.3.1) md5=67d3e5ed99de68340c16aef613b0ecf7 ;;
  5.8.1) md5=063a98c256ee0584b3c115924e41a332 ;;
  *) md5="" ;;
esac

mkdir -p "$src/dl"
tarball="$src/dl/zsh-$ver.tar.xz"
if [ ! -f "$tarball" ]; then
  curl -fsSL --retry 3 -o "$tarball.part" \
    "https://sourceforge.net/projects/zsh/files/zsh/$ver/zsh-$ver.tar.xz/download"
  mv "$tarball.part" "$tarball"
fi
echo "sha256 zsh-$ver.tar.xz: $(shasum -a 256 "$tarball" | cut -d' ' -f1)"
got_md5="$(md5 -q "$tarball" 2>/dev/null || md5sum "$tarball" | cut -d' ' -f1)"
if [ -n "$md5" ]; then
  [ "$got_md5" = "$md5" ] || { echo "md5 mismatch for zsh-$ver: $got_md5 != $md5" >&2; rm -f "$tarball"; exit 1; }
  echo "md5 matches the zsh project's published MD5SUM"
else
  echo "md5 $got_md5 (no published value pinned for $ver)"
fi

rm -rf "$src/zsh-$ver"
xz -dc "$tarball" | tar -x -C "$src" -f -
cd "$src/zsh-$ver"

# 5.3.1's configure probes fail on current clang (implicit declarations); relax those errors.
# --with-tcsetpgrp: configure otherwise aborts ("no controlling tty") when run without a terminal.
CFLAGS="-O2 -Wno-error=implicit-function-declaration -Wno-error=implicit-int"
export CFLAGS
./configure --prefix="$prefix" --disable-etcdir --disable-gdbm --with-tcsetpgrp >"$src/configure-$ver.log" 2>&1 \
  || { tail -30 "$src/configure-$ver.log" >&2; exit 1; }
make -j"$(sysctl -n hw.ncpu 2>/dev/null || nproc)" >"$src/make-$ver.log" 2>&1 \
  || { tail -30 "$src/make-$ver.log" >&2; exit 1; }
make install.bin install.modules install.fns >"$src/install-$ver.log" 2>&1 \
  || { tail -30 "$src/install-$ver.log" >&2; exit 1; }
complete_install || { echo "zsh $ver installed but incomplete" >&2; exit 1; }
echo "built: $("$prefix/bin/zsh" -fc 'print $ZSH_VERSION') at $prefix/bin/zsh"
