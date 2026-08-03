#!/usr/bin/env bash
#
# Run the validator over the hostile corpus and decide whether any of it still
# wins — the half of the exercise that produces a verdict.
#
# What counts as a failure here is not "reported an error". Every one of these
# files *should* draw a finding; several are supposed to be INVALID. The
# failures are:
#
#   - **abort** — the process died (SIGABRT/SIGSEGV, or "stack overflow" on
#     stderr). This is the one that matters most: a Rust stack overflow is not
#     a catchable panic, so an embedder cannot defend against it downstream. A
#     library that aborts is unusable in the ingestion pipelines and browsers
#     this project is aimed at.
#   - **panic** — a caught panic. Recoverable for an embedder, still a bug.
#   - **timeout** — no answer inside the limit, which for a hostile input is
#     the same denial of service as a crash.
#
# **What this deliberately does not catch.** Two of the six bugs it was built
# from would pass it. `zip-bomb.epub` against v0.8.6 exits 0 and reports VALID
# — it merely consumes 1.3 GB doing so, and neither memory nor a wrong verdict
# is checked here. Peak RSS is not portable to measure and the correct verdict
# for these files is not obvious enough to assert, so the script covers the
# crash classes only. Watch memory yourself (`/usr/bin/time -l` on macOS) when
# adding a shape whose failure mode is exhaustion rather than a signal.
#
# A clean run prints one line per file and exits 0. Any abort/panic/timeout
# exits 1 and names the file.
#
# Usage:
#   scripts/gen-hostile.py --scale && scripts/check-hostile.sh
#   scripts/check-hostile.sh target/hostile 120
#
# The scale-*.epub files additionally print their wall time. Read those as a
# *ladder*, not as absolute numbers: validation is linear in manifest size
# since 0.9.1, so doubling the items should roughly double the time. A
# regression shows up as the ladder bending, which is visible on any machine,
# where a single threshold would not be.

set -uo pipefail

DIR="${1:-target/hostile}"
TIMEOUT="${2:-120}"
# Overridable so this can be pointed at an older build — which is the only way
# to know the alarm still rings. It was verified against v0.8.6 (pre-guards),
# where xml-depth and all four css-* files come back ABORT.
BIN="${EPUBVERI_BIN:-target/release/epubveri}"

[ -x "$BIN" ] || { echo "build first: cargo build --release" >&2; exit 2; }
[ -d "$DIR" ] || { echo "no corpus at $DIR — run scripts/gen-hostile.py" >&2; exit 2; }

# `timeout` is GNU; macOS ships it only via coreutils. Fall back to running
# without one rather than skipping the check, and say so.
TO=""
if command -v timeout >/dev/null 2>&1; then TO="timeout $TIMEOUT"
elif command -v gtimeout >/dev/null 2>&1; then TO="gtimeout $TIMEOUT"
else echo "note: no timeout(1); hangs will block rather than be reported" >&2; fi

failed=0
shopt -s nullglob
errfile=$(mktemp)
trap 'rm -f "$errfile"' EXIT

for f in "$DIR"/*.epub; do
    name=$(basename "$f")
    # `/usr/bin/time -p` rather than `date` arithmetic: whole-second
    # resolution cannot show the ladder bending now that a 4,000-item book
    # validates in half a second, and the bend is the entire signal.
    # One stream for both: `time` writes to the same stderr as the command,
    # and separating them portably is more trouble than telling the three
    # `real`/`user`/`sys` lines apart from a panic message afterwards.
    /usr/bin/time -p $TO "$BIN" -i "$f" >/dev/null 2>"$errfile"
    code=$?
    elapsed=$(awk '/^real/{print $2}' "$errfile")
    err=$(grep -Ev '^(real|user|sys)[[:space:]]' "$errfile" || true)

    verdict=""
    case "$code" in
        124) verdict="TIMEOUT (>${TIMEOUT}s)" ;;
        134|139) verdict="ABORT (signal $((code - 128)))" ;;
        101) verdict="PANIC" ;;
    esac
    if [ -z "$verdict" ] && printf '%s' "$err" | grep -q "stack overflow\|panicked"; then
        verdict="ABORT (stack overflow)"
    fi

    if [ -n "$verdict" ]; then
        printf '  FAIL  %-22s %s\n' "$name" "$verdict"
        failed=1
    elif [[ "$name" == scale-* ]]; then
        printf '  ok    %-22s %ss\n' "$name" "$elapsed"
    else
        printf '  ok    %-22s\n' "$name"
    fi
done

if [ "$failed" -ne 0 ]; then
    echo "hostile corpus: FAILURES above" >&2
    exit 1
fi
echo "hostile corpus: no aborts, panics or timeouts"
