#!/usr/bin/env bash
#
# Publish the epubveri-wasm npm package (@veripublica/epubveri-wasm),
# robustly against iCloud "file 2" conflict copies.
#
# The project directory can live under an iCloud-synced path (e.g.
# ~/Documents). iCloud then periodically drops sync-conflict copies
# ("LICENSE 2", "README 2.md", …) into the gitignored epubveri-wasm/pkg/
# build directory. On 0.7.2 two such copies slipped into the published
# npm tarball. To avoid it, this script builds in place, then stages a
# clean copy OUTSIDE the iCloud tree (in $TMPDIR) and publishes from there,
# so iCloud can't inject conflict copies between build and publish.
#
# Usage:
#   scripts/publish-wasm.sh              # build + dry-run only
#   scripts/publish-wasm.sh <otp>        # build + real publish (2FA code)
#
# The version comes from epubveri-wasm/Cargo.toml — bump it (and keep it in
# sync with the crate) before running.

set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> Building epubveri-wasm (bundler, @veripublica scope)"
rm -rf epubveri-wasm/pkg
wasm-pack build epubveri-wasm --target bundler --scope veripublica --out-name epubveri

STAGE="${TMPDIR:-/tmp}/epubveri-wasm-pkg"
rm -rf "$STAGE"
cp -R epubveri-wasm/pkg "$STAGE"

# Belt-and-suspenders: drop any iCloud conflict copies that were present at
# copy time ("LICENSE 2", "LICENSE.COMMERCIAL 2.md", …). /tmp itself is not
# iCloud-synced, so none are created after this point.
find "$STAGE" \( -name '* [0-9]' -o -name '* [0-9].*' \) -delete
if find "$STAGE" \( -name '* [0-9]' -o -name '* [0-9].*' \) | grep -q .; then
  echo "!! conflict copies still present in $STAGE — aborting" >&2
  exit 1
fi

echo "==> Staged package ($(find "$STAGE" -type f | wc -l | tr -d ' ') files):"
ls "$STAGE"

# Fail fast if this version is already on npm. The package.json version comes
# from epubveri-wasm/Cargo.toml at build time, so the usual cause is forgetting
# to bump it - publishing a stale pkg/ then fails deep inside npm with the
# opaque "You cannot publish over the previously published versions" error.
# Catch it here with an actionable message instead (2026-07-24, after that bit
# during the 0.7.3 release).
PKG_NAME=$(node -p "require('$STAGE/package.json').name")
PKG_VER=$(node -p "require('$STAGE/package.json').version")
echo "==> Version check: $PKG_NAME@$PKG_VER"
if npm view "$PKG_NAME@$PKG_VER" version >/dev/null 2>&1; then
  echo "!! $PKG_NAME@$PKG_VER is already published on npm." >&2
  echo "   Bump 'version' in epubveri-wasm/Cargo.toml (keep it in sync with the" >&2
  echo "   epubveri crate), then re-run this script." >&2
  exit 1
fi

echo "==> npm publish --dry-run"
npm publish "$STAGE" --dry-run --access public

if [ -n "${1:-}" ]; then
  echo "==> npm publish (real, with OTP)"
  npm publish "$STAGE" --access public --otp="$1"
  echo "==> Published. Verify: npm view @veripublica/epubveri-wasm version"
else
  echo
  echo "Dry-run only. To publish for real, re-run with your 2FA code:"
  echo "    scripts/publish-wasm.sh <otp>"
fi
