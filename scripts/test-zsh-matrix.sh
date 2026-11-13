#!/bin/sh
# Run the zsh adapter tests on every supported zsh: 5.3.1 (minimum), 5.8.1, and the system zsh (5.9).
# Old versions come from scripts/build-zsh.sh (built here if missing). Exits non-zero if any run fails.
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root" || exit 1
fail=0
for ver in 5.3.1 5.8.1; do
  "$root/scripts/build-zsh.sh" "$ver" || exit 1
done
for z in "$root/target/zsh/5.3.1/bin/zsh" "$root/target/zsh/5.8.1/bin/zsh" /bin/zsh; do
  echo "=== zsh $("$z" -fc 'print $ZSH_VERSION') ($z)"
  FUXX_TEST_ZSH="$z" cargo test -p fuxx-completes --test zsh_adapter || fail=1
done
[ "$fail" -eq 0 ] && echo "matrix: all passed" || echo "matrix: FAILED" >&2
exit "$fail"
