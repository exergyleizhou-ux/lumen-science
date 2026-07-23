#!/usr/bin/env bash
# M6 productivity gate: count real journal/YYYY-MM-DD.md productivity days.
# Exit 0 only when ≥15 days marked as productivity days (是).
# Does NOT fabricate journals.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
JOURNAL="$ROOT/journal"
MIN="${PRODUCTIVITY_MIN_DAYS:-15}"
ART="$ROOT/artifacts/readiness"
mkdir -p "$ART"

count=0
days_file="$(mktemp)"
trap 'rm -f "$days_file"' EXIT

shopt -s nullglob
for f in "$JOURNAL"/[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9].md; do
  base=$(basename "$f")
  # Marked productivity day: checked "- [x] 是" near 生产力日 section, or explicit "是"
  if grep -Eq '^\s*-\s*\[[xX]\]\s*是' "$f"; then
    count=$((count + 1))
    echo "$base" >>"$days_file"
  elif grep -Eqi '今日算生产力日[？?]?\s*是|算生产力日[？?]?\s*是' "$f"; then
    count=$((count + 1))
    echo "$base" >>"$days_file"
  fi
done

echo "=== productivity-gate ==="
echo "journal_dir=$JOURNAL"
echo "count=$count"
echo "min=$MIN"
if [[ $count -gt 0 ]]; then
  echo "days:"
  sed 's/^/  /' "$days_file"
fi

python3 - "$ART/M6-productivity.json" "$count" "$MIN" "$days_file" <<'PY'
import json, sys
from datetime import datetime, timezone
from pathlib import Path
out, count, mn, days_path = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
days = Path(days_path).read_text().splitlines() if Path(days_path).stat().st_size else []
art = {
    "schema_version": 1,
    "check_id": "M6_15_day_self_use",
    "pass": count >= mn,
    "count": count,
    "min": mn,
    "days": days,
    "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
}
Path(out).write_text(json.dumps(art, indent=2) + "\n")
print("wrote", out, "pass=", art["pass"])
PY

if [[ "$count" -ge "$MIN" ]]; then
  echo "OK: productivity gate passed ($count ≥ $MIN)"
  exit 0
fi
echo "BLOCKED: productivity days $count < $MIN (use journal/TEMPLATE-productivity-day.md — do not fabricate)"
exit 1
