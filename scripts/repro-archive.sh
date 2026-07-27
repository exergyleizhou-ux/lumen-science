#!/usr/bin/env bash
# Build a byte-reproducible tar.gz or zip (LS5-R1-01).
#
# Plain `tar -czf` / `zip` embed per-file mtimes, uid/gid and (for gzip) a
# header timestamp, so two builds of the same commit produced different bytes
# and a release could not be independently reproduced.
#
# Determinism here rests on four things:
#   1. every input mtime normalised to SOURCE_DATE_EPOCH
#   2. entries in a stable (sorted) order
#   3. ownership zeroed
#   4. no timestamp in the gzip header (`gzip -n`)
#
# GNU tar and macOS bsdtar need different flags for (2) and (3), so both are
# supported rather than forcing one platform to install the other's tar.
#
# Usage:
#   SOURCE_DATE_EPOCH=<epoch> repro-archive.sh tar <out.tar.gz> <file...>
#   SOURCE_DATE_EPOCH=<epoch> repro-archive.sh zip <out.zip>    <file...>
#
# Run from the directory holding the inputs; globs are expanded by the caller.

set -euo pipefail

if [[ $# -lt 3 ]]; then
  echo "usage: repro-archive.sh <tar|zip> <output> <file...>" >&2
  exit 2
fi

kind="$1"
output="$2"
shift 2
files=("$@")

: "${SOURCE_DATE_EPOCH:?SOURCE_DATE_EPOCH must be set}"

if [[ ! "$SOURCE_DATE_EPOCH" =~ ^[0-9]+$ ]]; then
  echo "FAIL: SOURCE_DATE_EPOCH must be an integer, got '$SOURCE_DATE_EPOCH'" >&2
  exit 2
fi

for f in "${files[@]}"; do
  [[ -e "$f" ]] || { echo "FAIL: missing input $f" >&2; exit 1; }
done

# Sort for stable entry order. GNU tar has --sort=name; bsdtar does not, so the
# list is sorted here and both flavours consume the same order.
IFS=$'\n' read -r -d '' -a sorted < <(printf '%s\n' "${files[@]}" | LC_ALL=C sort && printf '\0')

# Normalise mtimes. `touch -d @epoch` is GNU-only and `touch -r` needs a
# reference file, so use python3 — it is already required by the release
# workflow and is present on macOS.
python3 - "$SOURCE_DATE_EPOCH" "${sorted[@]}" <<'PY'
import os, sys
epoch = int(sys.argv[1])
for path in sys.argv[2:]:
    os.utime(path, (epoch, epoch))
PY

rm -f "$output"

case "$kind" in
  tar)
    if tar --version 2>/dev/null | grep -q 'GNU tar'; then
      tar --sort=name --owner=0 --group=0 --numeric-owner \
          --mtime="@${SOURCE_DATE_EPOCH}" \
          -cf - "${sorted[@]}" | gzip -n > "$output"
    else
      # bsdtar: no --sort (handled above) and no --mtime (handled by os.utime),
      # but it can zero ownership, which is the remaining source of variance.
      tar --uid 0 --gid 0 --uname '' --gname '' \
          -cf - "${sorted[@]}" | gzip -n > "$output"
    fi
    ;;
  zip)
    # -X drops uid/gid and high-precision timestamps. zip reads mtimes from
    # disk, which os.utime already pinned. -o would set the archive mtime to
    # the newest entry; that is already SOURCE_DATE_EPOCH.
    zip -q -X -o "$output" "${sorted[@]}"
    ;;
  *)
    echo "FAIL: unknown archive kind '$kind' (expected tar or zip)" >&2
    exit 2
    ;;
esac

echo "repro-archive: $output ($(wc -c < "$output" | tr -d ' ') bytes, epoch ${SOURCE_DATE_EPOCH})"
