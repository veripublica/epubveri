#!/usr/bin/env bash
#
# Pre-flight for a release: run everything CI will run, plus everything that
# has ever gone wrong here, BEFORE the tag exists.
#
# WHY THIS EXISTS. Every guard in the publish workflows fires *after* the tag
# is pushed. That is the wrong side of the irreversible step: crates.io never
# lets a version number be reused, and a half-failed release is noisy to
# unpick. On 0.9.2 the lockfile was left behind by the version bump and
# `cargo test --workspace --locked` — the exact command the crates.io publish
# guard runs — failed locally. Had it not been run by hand, it would have
# failed on the tagged commit, in public. Nothing here is new logic; it is the
# CI guards plus this project's own history, moved to before the tag.
#
# Usage:
#   scripts/preflight.sh              # check the version in Cargo.toml
#   scripts/preflight.sh 0.9.3        # check a version you are about to bump to
#   SKIP_SLOW=1 scripts/preflight.sh  # skip corpus/hostile/shelf
#
# Exit 0 = safe to `git push origin main vX.Y.Z`. Every check runs even after
# one fails, so a single pass gives the whole list rather than the first item.

set -uo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"

PASS=0
FAIL=0
declare -a FAILED

ok()   { printf '  \033[32mok\033[0m    %s\n' "$1"; PASS=$((PASS + 1)); }
bad()  { printf '  \033[31mFAIL\033[0m  %s\n' "$1"; FAIL=$((FAIL + 1)); FAILED+=("$1"); }
skip() { printf '  --    %s\n' "$1"; }
head_() { printf '\n\033[1m%s\033[0m\n' "$1"; }

# check <label> <command...> - runs it, keeps going either way.
check() {
  local label="$1"; shift
  local out
  if out=$("$@" 2>&1); then ok "$label"; else
    bad "$label"
    printf '%s\n' "$out" | tail -15 | sed 's/^/        /'
  fi
}

command -v jq >/dev/null || { echo "preflight needs jq (same as the release workflows)"; exit 2; }

# ---------------------------------------------------------------- versions --
head_ "Version"

META=$(cargo metadata --no-deps --format-version 1 2>/dev/null)
CRATE_V=$(jq -e -r '.packages[] | select(.name == "epubveri") | .version' <<< "$META" 2>/dev/null) || CRATE_V=""
WASM_V=$(jq -e -r '.packages[] | select(.name == "epubveri-wasm") | .version' <<< "$META" 2>/dev/null) || WASM_V=""
VERSION="${1:-$CRATE_V}"

if [ -z "$CRATE_V" ] || [ -z "$WASM_V" ]; then
  bad "cargo metadata reported both package versions"
else
  echo "  target version: $VERSION   (crate $CRATE_V, wasm $WASM_V)"
  # The same comparison publish-crate.yml makes against the tag - made here,
  # where the fix is still free.
  [ "$VERSION" = "$CRATE_V" ] && ok "Cargo.toml matches the target version" \
    || bad "Cargo.toml is $CRATE_V, expected $VERSION"
  [ "$VERSION" = "$WASM_V" ] && ok "epubveri-wasm/Cargo.toml matches" \
    || bad "epubveri-wasm/Cargo.toml is $WASM_V, expected $VERSION"
fi

grep -q "^## \[$VERSION\]" CHANGELOG.md \
  && ok "CHANGELOG.md has a [$VERSION] section" \
  || bad "CHANGELOG.md has no [$VERSION] section"

# Reminder rather than a check: no script can tell whether the notes are
# complete. 0.9.2's omitted the whole hostile suite, the largest thing in it.
echo "  note: read \`git log v<prev>..HEAD\` against the CHANGELOG section by eye —"
echo "        0.9.2 shipped with its biggest item (the hostile suite) unmentioned."

# ------------------------------------------------------------------ hygiene --
head_ "Tree hygiene"

if [ -z "$(git status --porcelain)" ]; then
  ok "working tree clean (build.rs stamps .dirty from untracked files too)"
  TREE_CLEAN=1
else
  bad "working tree dirty — 0.7.6 shipped as 0.7.6+45ae97a.dirty this way"
  git status --short | sed 's/^/        /'
  TREE_CLEAN=0
fi

# iCloud conflict copies: shipped into the 0.7.2 npm tarball, and on
# 2026-08-03 one inside .git/refs broke `git fetch` outright.
CONFLICTS=$(find . -path ./target -prune -o \( -name "* [0-9]" -o -name "* [0-9].*" \) -print 2>/dev/null | grep -v '^./target' || true)
if [ -z "$CONFLICTS" ]; then
  ok "no iCloud sync-conflict copies"
else
  bad "iCloud sync-conflict copies present"
  printf '%s\n' "$CONFLICTS" | sed 's/^/        /'
fi

if git rev-parse -q --verify "refs/tags/v$VERSION" >/dev/null; then
  bad "tag v$VERSION already exists locally"
else
  ok "tag v$VERSION is free locally"
fi

# A broken ref makes this fail, which is how the .git conflict copy surfaced.
if git fetch --quiet origin 2>/dev/null; then
  ok "git fetch works"
  BEHIND=$(git rev-list --count HEAD..origin/main 2>/dev/null || echo "?")
  [ "$BEHIND" = "0" ] && ok "local main is not behind origin" \
    || bad "local main is $BEHIND commit(s) behind origin/main"
else
  bad "git fetch failed — check .git/refs for conflict copies"
fi

# ------------------------------------------------------------- already out? --
head_ "Registries"

UA="epubveri-release (github.com/veripublica/epubveri)"
if curl -sf -H "User-Agent: $UA" "https://crates.io/api/v1/crates/epubveri/$VERSION" >/dev/null 2>&1; then
  bad "crates.io already has $VERSION — a version number can never be reused"
else
  ok "crates.io does not have $VERSION yet"
fi

NPM_V=$(curl -s "https://registry.npmjs.org/@veripublica/epubveri-wasm" 2>/dev/null | jq -r '.versions | keys[]' 2>/dev/null | grep -Fx "$VERSION" || true)
[ -z "$NPM_V" ] && ok "npm does not have $VERSION yet" \
  || bad "npm already has $VERSION"

# ------------------------------------------------------------------- build --
head_ "Build gates (what CI runs)"

# --locked is the one that bit on 0.9.2: a version bump does not update
# Cargo.lock, and this is the publish guard's exact command.
check "cargo test --workspace --locked" cargo test --workspace --locked
check "cargo fmt --check" cargo fmt --check
check "cargo clippy --workspace --all-targets -- -D warnings" \
  cargo clippy --workspace --all-targets -- -D warnings
check "wasm32-unknown-unknown build" \
  cargo build --release -p epubveri-wasm --target wasm32-unknown-unknown
# Added 2026-08-22: nothing ran rustdoc, here or in CI, so a doc link to a
# private item or a bare URL failed silently and shipped. Six were found the
# first time this was run, one of them written the same day. It also compiles
# every ``` block as a doctest, which is how an indented shell snippet in a
# harness header got caught pretending to be Rust.
check "cargo doc (no broken links)" \
  env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# ----------------------------------------------------------------- package --
head_ "Package contents"

# `cargo package` refuses on a dirty tree, so without this guard one root
# cause (an uncommitted file) would raise two alarms and the second would
# read as an unrelated packaging problem.
if [ "$TREE_CLEAN" = "0" ]; then
  skip "package contents — needs a clean tree; fix the dirty-tree failure first"
elif PKG=$(cargo package --list --locked 2>/dev/null) && [ -n "$PKG" ]; then
  ok "cargo package --list succeeded ($(wc -l <<< "$PKG" | tr -d ' ') files)"
  STRAY=$(grep -E " [0-9]+(\.|$)|\.py$" <<< "$PKG" || true)
  [ -z "$STRAY" ] && ok "no conflict copies or Python in the package" || {
    bad "stray files would ship"
    printf '%s\n' "$STRAY" | sed 's/^/        /'
  }
else
  bad "cargo package --list failed"
  cargo package --list --locked 2>&1 | tail -8 | sed 's/^/        /'
fi

# ------------------------------------------------------------- instruments --
if [ "${SKIP_SLOW:-}" = "1" ]; then
  head_ "Instruments (skipped: SKIP_SLOW=1)"
else
  head_ "Instruments"

  # Non-empty is asserted, not assumed: an early "corpus identical before and
  # after" once compared two empty files.
  if CORPUS=$(cargo run --release -q -p epubveri-harness --bin corpus 2>&1) && [ -n "$CORPUS" ]; then
    ok "corpus ran and produced output"
    grep -E "recall|FALSE POSITIVES" <<< "$CORPUS" | sed 's/^/        /'
    echo "        ^ compare against the previous release by eye"
  else
    bad "corpus produced nothing — it cannot answer 'no change' from an empty run"
  fi

  # W3C's conformance suite. Cheap (~9 s, no JVM) and it sees a part of the
  # format the other two instruments do not reach at all: no book on the shelf
  # carries a media overlay, a rendition:layout spine override, or a viewport
  # meta in more than one file, and the corpus fixtures each trip exactly one
  # rule. Seven defects on its first run were invisible to both.
  #
  # The clone is gitignored, so its absence is a setup gap and not a release
  # blocker - but a *silent* absence would be the same mistake the shelf block
  # above exists to avoid.
  EPUB_TESTS="${EPUB_TESTS_DIR:-corpus/epub-tests}"
  if [ -d "$EPUB_TESTS/tests" ]; then
    if ET=$(cargo run --release -q -p epubveri-harness --bin epubtests 2>&1) && [ -n "$ET" ]; then
      ok "epub-tests ran and produced output"
      grep -E "^packaged|^  (VALID|INVALID) " <<< "$ET" | sed 's/^/        /'
      echo "        ^ these are conformance tests: most are meant to be valid,"
      echo "        so a rise in INVALID is more likely ours than W3C's"
    else
      bad "epub-tests produced nothing - an empty run cannot answer anything"
    fi
  else
    skip "epub-tests not cloned ($EPUB_TESTS) - W3C's conformance suite is absent"
    echo "        git clone --depth 1 https://github.com/w3c/epub-tests.git corpus/epub-tests"
  fi

  check "hostile (no abort, panic or timeout)" \
    cargo run --release -q -p epubveri-harness --bin hostile

  # The shelf is local-only and machine-specific, so its absence is not a
  # failure - but a silent absence would be, since it is the only instrument
  # that sees real books.
  # Shared with epubsana since 2026-08-05, hence the neutral name and the
  # override: the shelf is one canonical corpus for both projects, and only
  # this repo's scripts should assume where it sits.
  SHELF="${SHELF:-$HOME/Documents/Projects/ebook-shelf/diff-shelf.sh}"
  SHELF_BASE="${SHELF_BASE:-baseline}"
  if [ -x "$SHELF" ]; then
    cargo build --release --bin epubveri >/dev/null 2>&1
    OUT=$("$SHELF" "$SHELF_BASE" 2>&1)
    # A *missing* snapshot is a setup gap, not a regression. Failing the
    # release for it would be wrong and would teach you to ignore the verdict;
    # passing quietly would hide that the only real-book check never ran.
    if grep -qi "no snapshot" <<< "$OUT"; then
      skip "shelf: no '$SHELF_BASE' snapshot — run '$SHELF save' on the LAST"
      echo "        released build, so the next release has something to diff against"
    elif grep -q "no change" <<< "$OUT"; then
      ok "shelf: no change per book vs '$SHELF_BASE'"
    else
      bad "shelf changed vs '$SHELF_BASE' — inspect per book"
      printf '%s\n' "$OUT" | tail -20 | sed 's/^/        /'
    fi
  else
    skip "shelf not on this machine ($SHELF) — the only real-book check is absent"
  fi

  # docs/COVERAGE.md is generated; a stale one is a published document that
  # disagrees with the code.
  cargo run --release -q -p epubveri-harness --bin coverage >/dev/null 2>&1
  if git diff --quiet docs/COVERAGE.md; then
    ok "docs/COVERAGE.md is up to date"
  else
    bad "docs/COVERAGE.md regenerated differently — commit the regenerated file"
  fi
fi

# ------------------------------------------------------------------ verdict --
printf '\n\033[1m%s\033[0m\n' "Verdict"
echo "  $PASS passed, $FAIL failed"
if [ "$FAIL" -gt 0 ]; then
  printf '\033[31m  NOT READY\033[0m — fix these, then run again:\n'
  for f in "${FAILED[@]}"; do echo "    - $f"; done
  exit 1
fi
printf '\033[32m  READY\033[0m — git push origin main v%s\n' "$VERSION"
