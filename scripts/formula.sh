#!/bin/sh
# Release tool: print the Homebrew formula (Formula/fuxx-completes.rb of the tap
# fitzfitz/homebrew-fuxx) for one release, with checksums of the real bytes (M3).
# Usage: scripts/formula.sh <version> [--local]
#   default  hashes the GitHub release's two zips and GitHub's tarball of tag v<version>
#   --local  for the pre-release check: file:// URLs to target/dist/v<version>/ (the zips of
#            scripts/package.sh, and a `git archive` of HEAD made here)
# The formula installs the prebuilt binary from the release zip for this Mac's chip and the zsh,
# bash and fish adapters from the tagged source. It refuses a version other than Cargo.toml's.
# Writes only under target/ (the --local tarball; downloads go to a temp folder removed at exit).
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

fail() {
  echo "formula.sh: $*" >&2
  exit 1
}

version="${1-}"
mode="${2-}"
[ -n "$version" ] && { [ -z "$mode" ] || [ "$mode" = --local ]; } && [ "$#" -le 2 ] ||
  { echo "usage: scripts/formula.sh <version> [--local]" >&2; exit 2; }

cargo="$(awk '
  /^\[/ { inside = ($0 == "[workspace.package]"); next }
  inside && $1 == "version" && $2 == "=" { gsub(/"/, "", $3); print $3; exit }
' Cargo.toml)"
[ "$version" = "$cargo" ] || fail "version mismatch: asked for $version, Cargo.toml ${cargo:-none}"

repo="https://github.com/fitzfitz/fuxx-completes"
dist="$root/target/dist/v$version"

sha() { shasum -a 256 "$1" | awk '{ print $1 }'; }

if [ "$mode" = --local ]; then
  for chip in arm64 x86_64; do
    [ -f "$dist/fuxx-plugin-macos-$chip.zip" ] || fail "no $dist/fuxx-plugin-macos-$chip.zip (run scripts/package.sh first)"
  done
  tarball="$dist/fuxx-completes-$version.tar.gz"
  git archive --format=tar.gz --prefix="fuxx-completes-$version/" -o "$tarball" HEAD
  src_url="file://$tarball"
  arm_url="file://$dist/fuxx-plugin-macos-arm64.zip"
  intel_url="file://$dist/fuxx-plugin-macos-x86_64.zip"
  src_sha="$(sha "$tarball")"
  arm_sha="$(sha "$dist/fuxx-plugin-macos-arm64.zip")"
  intel_sha="$(sha "$dist/fuxx-plugin-macos-x86_64.zip")"
else
  mkdir -p "$root/target/tmp"
  work="$(mktemp -d "$root/target/tmp/formula.XXXXXX")"
  trap 'rm -rf "$work"' EXIT
  trap 'exit 130' INT TERM
  src_url="$repo/archive/refs/tags/v$version.tar.gz"
  arm_url="$repo/releases/download/v$version/fuxx-plugin-macos-arm64.zip"
  intel_url="$repo/releases/download/v$version/fuxx-plugin-macos-x86_64.zip"
  curl -fsSL -o "$work/src.tar.gz" "$src_url" || fail "can't download $src_url"
  curl -fsSL -o "$work/arm64.zip" "$arm_url" || fail "can't download $arm_url"
  curl -fsSL -o "$work/x86_64.zip" "$intel_url" || fail "can't download $intel_url"
  src_sha="$(sha "$work/src.tar.gz")"
  arm_sha="$(sha "$work/arm64.zip")"
  intel_sha="$(sha "$work/x86_64.zip")"
fi

cat <<EOF
class FuxxCompletes < Formula
  desc "Ghost-text suggestions for zsh and bash from your own history"
  homepage "$repo"
  url "$src_url"
  sha256 "$src_sha"
  license any_of: ["MIT", "Apache-2.0"]

  depends_on :macos

  # The prebuilt binary from the release's fuxx plugin zip for this Mac's chip (the same bytes
  # fuxx installs); the adapters come from the tagged source above.
  resource "binary" do
    on_arm do
      url "$arm_url"
      sha256 "$arm_sha"
    end
    on_intel do
      url "$intel_url"
      sha256 "$intel_sha"
    end
  end

  def install
    resource("binary").stage { bin.install "bin/fuxx-completes" }
    pkgshare.install "adapters/zsh", "adapters/bash", "adapters/fish"
  end

  def caveats
    <<~EOS
      Nothing is turned on for you. Add the line for your shell:

        zsh, in ~/.zshrc:
          source #{HOMEBREW_PREFIX}/share/fuxx-completes/zsh/fuxx-autosuggestions.plugin.zsh

        bash 4.0+, at the end of ~/.bashrc:
          source #{HOMEBREW_PREFIX}/share/fuxx-completes/bash/fuxx-autosuggestions.bash

        fish (reports your commands; fish keeps its own suggestions):
          ln -s #{HOMEBREW_PREFIX}/share/fuxx-completes/fish/fuxx-autosuggestions.fish ~/.config/fish/conf.d/

      Then open a new shell. \`fuxx-completes doctor\` shows what is running.
      Before uninstalling, \`fuxx-completes forget\` wipes its small context file.
    EOS
  end

  test do
    assert_match "fuxx-completes #{version}", shell_output("#{bin}/fuxx-completes doctor")
    assert_match "usage: fuxx-completes", shell_output("#{bin}/fuxx-completes nope 2>&1", 2)
    assert_empty shell_output("zsh -fc 'source #{pkgshare}/zsh/fuxx-autosuggestions.plugin.zsh'")
  end
end
EOF
