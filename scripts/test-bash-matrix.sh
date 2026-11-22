#!/bin/sh
# Run the bash adapter tests on every supported bash: 4.0 (minimum) and 5.3 (latest), built into
# target/bash/ by scripts/build-bash.sh (built here if missing), with ble.sh 0.4.0-devel3 from
# scripts/get-blesh.sh. The bash 3.2 stay-off cases run on macOS's /bin/bash inside each run.
# Exits non-zero if any run fails.
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root" || exit 1
fail=0
for ver in 4.0 5.3; do
  "$root/scripts/build-bash.sh" "$ver" || exit 1
done
"$root/scripts/get-blesh.sh" 0.4.0-devel3 || exit 1
for ver in 4.0 5.3; do
  b="$root/target/bash/$ver/bin/bash"
  echo "=== bash $("$b" --norc -c 'echo $BASH_VERSION') ($b)"
  FUXX_TEST_BASH="$b" FUXX_TEST_BLESH="$root/target/blesh/0.4.0-devel3/ble.sh" \
    cargo test -p fuxx-completes --test bash_adapter || fail=1
done
[ "$fail" -eq 0 ] && echo "matrix: all passed" || echo "matrix: FAILED" >&2
exit "$fail"
