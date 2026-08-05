#!/usr/bin/env bash
#
# Run epubveri over a directory of real EPUBs and summarise what it found —
# a false-positive hunt, not a coverage measurement.
#
# The epubcheck corpus answers "do we catch what epubcheck catches". It cannot
# answer "do we invent errors on books that are actually fine", because its
# fixtures are synthetic and each one is built to trip exactly one rule. Real
# books are where false positives live, and false positives are what make a
# tool untrustworthy to integrate against.
#
# Usage:
#   scripts/scan-shelf.sh ~/Documents/Projects/ebook-shelf            # every .epub under it
#   scripts/scan-shelf.sh ~/Documents/Projects/ebook-shelf --advisory # include ADV-*
#
# Reading the output:
#
#   - **Errors and fatals are the headline.** They change the verdict, so a
#     book that a human considers fine appearing here is a false-positive
#     candidate and worth opening individually.
#   - **A single message ID dominating the count usually means one producer's
#     quirk, not one book's.** The first real-shelf run had 3768 OPF-088s from
#     a single `epub:type="normal"` that Project Gutenberg's generator writes
#     into every book. That was genuine (the term is in no vocabulary), but the
#     shape — one ID, one value, every book — is the signature to look for.
#   - **A shelf from one producer tests one toolchain, N times.** Mix sources
#     (Sigil-authored, Calibre-converted, InDesign exports, publisher output)
#     before drawing conclusions from a clean run.

set -euo pipefail

SHELF="${1:-}"
if [ -z "$SHELF" ] || [ ! -d "$SHELF" ]; then
  echo "usage: $(basename "$0") <shelf-directory> [--advisory]" >&2
  exit 2
fi
shift || true
EXTRA=()
if [ "$#" -gt 0 ]; then EXTRA=("$@"); fi

BIN="$(dirname "$0")/../target/release/epubveri"
if [ ! -x "$BIN" ]; then
  echo "==> building epubveri (release)"
  cargo build --release --bin epubveri
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# macOS still ships bash 3.2, which has no `mapfile` - keep this portable.
find "$SHELF" -type f -name '*.epub' | sort > "$WORK/books.txt"
COUNT=$(wc -l < "$WORK/books.txt" | tr -d ' ')
if [ "$COUNT" -eq 0 ]; then
  echo "no .epub files under $SHELF" >&2
  exit 1
fi

echo "==> scanning $COUNT book(s) under $SHELF"
: > "$WORK/all.txt"
while IFS= read -r book; do
  # Never let one unreadable book stop the scan; a crash or non-zero exit is
  # itself a finding, so record it rather than aborting.
  if [ ${#EXTRA[@]} -gt 0 ]; then
    out="$("$BIN" -i "$book" "${EXTRA[@]}" 2>&1 || true)"
  else
    out="$("$BIN" -i "$book" 2>&1 || true)"
  fi
  printf '%s\n' "$out" | sed "s|^|$(basename "$book")\t|" >> "$WORK/all.txt"
done < "$WORK/books.txt"

# Verdict line is the last line of each book's report.
grep -oE '(VALID|INVALID)$' "$WORK/all.txt" | sort | uniq -c > "$WORK/verdicts.txt" || true

echo
echo "--- verdicts ---"
cat "$WORK/verdicts.txt"

echo
echo "--- findings by severity ---"
grep -oE '\t(FATAL|ERROR|WARNING|INFO|USAGE) ' "$WORK/all.txt" | tr -d '\t ' | sort | uniq -c | sort -rn || echo "  (none)"

echo
echo "--- findings by ID (most frequent first) ---"
grep -oE '(FATAL|ERROR|WARNING|INFO|USAGE) [A-Z]{3}-[0-9]+' "$WORK/all.txt" \
  | sort | uniq -c | sort -rn | head -30 || echo "  (none)"

# The thing worth a human's attention: anything that changes a verdict.
echo
echo "--- ERROR/FATAL findings (false-positive candidates) ---"
if grep -E '\t(FATAL|ERROR) ' "$WORK/all.txt" > "$WORK/errors.txt"; then
  echo "  $(wc -l < "$WORK/errors.txt" | tr -d ' ') finding(s), in $(cut -f1 "$WORK/errors.txt" | sort -u | wc -l | tr -d ' ') book(s):"
  echo
  cut -f1,2 "$WORK/errors.txt" | sort | uniq -c | sort -rn | head -40
  echo
  echo "  books to open individually:"
  cut -f1 "$WORK/errors.txt" | sort -u | head -20 | sed 's/^/    /'
else
  echo "  none — no book on this shelf was rejected."
fi

# One ID with one distinct message across many books is a producer quirk.
echo
echo "--- distinct message text per ID (a single row = one producer's quirk) ---"
grep -oE '(FATAL|ERROR|WARNING|INFO|USAGE) [A-Z]{3}-[0-9]+: [^[]*' "$WORK/all.txt" \
  | sed -E 's/^[A-Z]+ //' | sort -u \
  | awk -F': ' '{c[$1]++} END {for (k in c) printf "  %-10s %d distinct message(s)\n", k, c[k]}' \
  | sort -k2 -rn || true
