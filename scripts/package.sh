#!/bin/sh
# Release tool: build the fuxx-completes binary for both Mac chips and make the two fuxx plugin
# zips of design §8, with every check a release needs (M3).
# Usage: scripts/package.sh [--allow-dirty]   (--allow-dirty: dry runs only, never a release)
# Writes target/dist/v<version>/{fuxx-plugin-macos-arm64.zip,fuxx-plugin-macos-x86_64.zip,SHA256SUMS}
# and nothing else outside target/. Silent and exit 0 on success; one line and exit 1 on failure.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

allow_dirty=0
case "${1-}" in
'') ;;
--allow-dirty) allow_dirty=1 ;;
*) echo "usage: scripts/package.sh [--allow-dirty]" >&2; exit 2 ;;
esac

fail() {
  echo "package.sh: $*" >&2
  exit 1
}

limit=786432 # design §3, cli/tests/size.rs: 768 KB per binary
expected='LICENSE-APACHE
LICENSE-MIT
THIRD_PARTY_NOTICES.md
bin/
bin/fuxx-completes
fuxx-plugin.conf
shell/
shell/fuxx-autosuggestions.plugin.zsh'

# 1. A clean tree: a release is made from committed files only.
if [ "$allow_dirty" -eq 0 ] && [ -n "$(git status --porcelain)" ]; then
  fail "uncommitted changes (commit them, or --allow-dirty for a dry run)"
fi

# 2. One version everywhere: [workspace.package], the two pins, fuxx-plugin.conf.
version="$(awk '
  /^\[/ { inside = ($0 == "[workspace.package]"); next }
  inside && $1 == "version" && $2 == "=" { gsub(/"/, "", $3); print $3; exit }
' Cargo.toml)"
[ -n "$version" ] || fail "no version in Cargo.toml's [workspace.package]"
conf="$(awk '$1 == "version" && $2 == "=" { print $3; exit }' fuxx-plugin.conf)"
[ "$conf" = "$version" ] || fail "version mismatch: fuxx-plugin.conf ${conf:-none}, Cargo.toml $version"
for krate in fuxx-completes-core fuxx-autosuggestions; do
  pin="$(awk -v k="$krate" '
    /^\[/ { inside = ($0 == "[workspace.dependencies]"); next }
    inside && $1 == k { if (match($0, /version *= *"=[^"]*"/)) { v = substr($0, RSTART, RLENGTH); sub(/^version *= *"=/, "", v); sub(/"$/, "", v); print v }; exit }
  ' Cargo.toml)"
  [ "$pin" = "$version" ] || fail "version mismatch: $krate pin ${pin:-none}, Cargo.toml $version"
done

dist="$root/target/dist/v$version"
mkdir -p "$root/target/tmp"
work="$(mktemp -d "$root/target/tmp/package.XXXXXX")"
trap 'rm -rf "$work"' EXIT
trap 'exit 130' INT TERM

# 3. Build both chips from the lockfile. Cargo's output goes to a log, shown only on failure.
# Source paths under $HOME (the crates in ~/.cargo, this checkout) are written as `~` in the
# binary, so the builder's home folder isn't in a public asset. (CARGO_ENCODED_RUSTFLAGS: one
# flag, a path with spaces stays whole; it replaces any RUSTFLAGS for this build only.)
CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$HOME=~"
export CARGO_ENCODED_RUSTFLAGS
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  if ! cargo build --release --locked --quiet -p fuxx-completes --target "$target" >"$work/build.log" 2>&1; then
    tail -n 20 "$work/build.log" >&2
    fail "cargo build failed for $target"
  fi
done

rm -rf "$dist"
mkdir -p "$dist"
for chip in arm64 x86_64; do
  case "$chip" in
  arm64) target=aarch64-apple-darwin run="" ;;
  x86_64) target=x86_64-apple-darwin run="/usr/bin/arch -x86_64" ;;
  esac
  bin="target/$target/release/fuxx-completes"

  # 4. The binary is the chip the zip says, and fits.
  archs="$(lipo -archs "$bin" 2>/dev/null)" || fail "lipo can't read $bin"
  [ "$archs" = "$chip" ] || fail "$bin is '$archs', not $chip"
  size="$(stat -f %z "$bin")"
  [ "$size" -lt "$limit" ] || fail "$chip binary is $size bytes, over the $limit limit"
  if [ "$chip" = arm64 ]; then
    codesign -v "$bin" 2>/dev/null || fail "arm64 binary has no valid (ad-hoc) signature"
  fi
  # A public asset never carries the builder's home folder (panic locations hold source paths).
  if strings -a "$bin" | grep -qF "$HOME/"; then
    fail "$chip binary holds the builder's home path ($HOME)"
  fi

  # 5. Stage the zip's root: §8's three plus the licences and notices.
  stage="$work/stage-$chip"
  mkdir -p "$stage/bin" "$stage/shell"
  cp fuxx-plugin.conf LICENSE-MIT LICENSE-APACHE THIRD_PARTY_NOTICES.md "$stage/"
  cp "$bin" "$stage/bin/fuxx-completes"
  chmod 755 "$stage/bin/fuxx-completes"
  cp adapters/zsh/fuxx-autosuggestions.plugin.zsh "$stage/shell/"
  zip="$dist/fuxx-plugin-macos-$chip.zip"
  ditto -c -k --norsrc --noextattr "$stage" "$zip"

  # 6. Exactly the expected entries: no nested folder, no __MACOSX, no ._ files.
  entries="$(zipinfo -1 "$zip" | LC_ALL=C sort)"
  [ "$entries" = "$expected" ] || fail "$(basename "$zip") holds unexpected entries: $(echo "$entries" | tr '\n' ' ')"

  # 7. Unpacked, the binary is executable and runs (x86_64 under Rosetta) in a throwaway HOME.
  out="$work/unzip-$chip"
  home="$work/home-$chip"
  mkdir -p "$out" "$home"
  ditto -x -k "$zip" "$out"
  [ -x "$out/bin/fuxx-completes" ] || fail "$chip: bin/fuxx-completes lost its exec bit"
  first="$(env -i HOME="$home" PATH=/usr/bin:/bin $run "$out/bin/fuxx-completes" doctor 2>&1 | head -n 1)" || true
  [ "$first" = "fuxx-completes $version" ] || fail "$chip: doctor printed '$first', not 'fuxx-completes $version'"
done

(cd "$dist" && shasum -a 256 fuxx-plugin-macos-arm64.zip fuxx-plugin-macos-x86_64.zip >SHA256SUMS)
