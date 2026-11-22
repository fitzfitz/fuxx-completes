#!/bin/sh
# Run the fish adapter tests on every fish that builds here, from scripts/build-fish.sh into
# target/fish/. The plan's matrix is 3.0.2 (minimum), 3.7.1 (latest 3.x) and 4.9.3 (latest 4.x);
# 3.0.2 and 3.7.1 don't compile against this Mac's libc++ (2026-11-23, design §14), so a version
# that fails to build is reported and skipped. Exits non-zero if a run fails or nothing ran.
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root" || exit 1
fail=0
ran=0
for ver in 3.0.2 3.7.1 4.9.3; do
  if ! "$root/scripts/build-fish.sh" "$ver" >/dev/null 2>&1; then
    echo "=== fish $ver: does not build here (scripts/build-fish.sh $ver); skipped" >&2
    continue
  fi
  f="$root/target/fish/$ver/bin/fish"
  echo "=== $("$f" --version) ($f)"
  FUXX_TEST_FISH="$f" cargo test -p fuxx-completes --test fish_adapter || fail=1
  ran=$((ran + 1))
done
[ "$ran" -gt 0 ] || { echo "matrix: no fish built" >&2; exit 1; }
[ "$fail" -eq 0 ] && echo "matrix: all passed ($ran fish)" || echo "matrix: FAILED" >&2
exit "$fail"
