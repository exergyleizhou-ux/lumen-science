#!/usr/bin/env bash
# Lumen cache-impact guard (Reasonix check-cache-impact semantics, Lumen paths).
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/check-cache-impact.sh [changed-file ...]

When cache-sensitive paths change, the PR body (CACHE_IMPACT_PR_BODY / PR_BODY)
must include non-placeholder Cache-impact and Cache-guard lines.

Env:
  CACHE_IMPACT_PR_BODY or PR_BODY
  CACHE_IMPACT_PR_BODY_FILE
  CACHE_IMPACT_CHANGED_FILES / CACHE_IMPACT_CHANGED_FILES_FILE
  CACHE_IMPACT_BASE_SHA / BASE_SHA, CACHE_IMPACT_HEAD_SHA / HEAD_SHA
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

body="${CACHE_IMPACT_PR_BODY:-${PR_BODY:-}}"
if [[ -n "${CACHE_IMPACT_PR_BODY_FILE:-}" ]]; then
  body="$(cat "$CACHE_IMPACT_PR_BODY_FILE")"
fi

changed_input=""
if [[ "$#" -gt 0 ]]; then
  changed_input="$(printf '%s\n' "$@")"
elif [[ -n "${CACHE_IMPACT_CHANGED_FILES_FILE:-}" ]]; then
  changed_input="$(cat "$CACHE_IMPACT_CHANGED_FILES_FILE")"
elif [[ -n "${CACHE_IMPACT_CHANGED_FILES:-}" ]]; then
  changed_input="$CACHE_IMPACT_CHANGED_FILES"
else
  base="${CACHE_IMPACT_BASE_SHA:-${BASE_SHA:-}}"
  head="${CACHE_IMPACT_HEAD_SHA:-${HEAD_SHA:-HEAD}}"
  if [[ -z "$base" ]]; then
    base="$(git merge-base origin/main HEAD 2>/dev/null || git rev-parse HEAD~1 2>/dev/null || true)"
  fi
  if [[ -n "$base" ]]; then
    changed_input="$(git diff --name-only "$base" "$head" 2>/dev/null || true)"
  fi
fi

is_sensitive() {
  local f="$1"
  case "$f" in
    agent/crates/codegen/lumen-discipline/*|\
    agent/crates/codegen/xai-grok-models/default_models.json|\
    agent/crates/codegen/xai-grok-sampler/*|\
    agent/crates/codegen/xai-grok-sampling-types/*|\
    agent/crates/codegen/xai-grok-shell/src/session/acp_session_impl/sampler_turn.rs|\
    agent/crates/codegen/xai-grok-shell/src/session/acp_session_impl/turn.rs|\
    agent/crates/codegen/xai-grok-shell/src/extensions/notification.rs|\
    agent/crates/codegen/xai-grok-shell/src/agent/mvp_agent/mod.rs|\
    agent/crates/codegen/xai-grok-pager/src/app/dispatch/prompt.rs|\
    agent/crates/codegen/xai-grok-pager/src/ui_contract.rs|\
    agent/crates/codegen/xai-grok-pager/src/views/truth_bar.rs|\
    policy/LUMEN_CACHE.md|\
    config/lumen.example.toml)
      return 0
      ;;
  esac
  return 1
}

sensitive=()
while IFS= read -r file; do
  [[ -z "$file" ]] && continue
  if is_sensitive "$file"; then
    sensitive+=("$file")
  fi
done <<< "$changed_input"

if [[ "${#sensitive[@]}" -eq 0 ]]; then
  echo "check-cache-impact: no cache-sensitive paths in diff (ok)"
  exit 0
fi

echo "check-cache-impact: sensitive files:"
printf '  %s\n' "${sensitive[@]}"

if [[ -z "${body// }" ]]; then
  echo "FAIL: cache-sensitive changes require PR body with Cache-impact / Cache-guard lines" >&2
  echo "Set CACHE_IMPACT_PR_BODY or pass files only for listing." >&2
  exit 1
fi

# Reject placeholders (Reasonix-style)
reject_re='(?i)Cache-impact:\s*(n/a|none|todo|tbd)\s*$|Cache-guard:\s*(n/a|none|todo|tbd)\s*$'
if echo "$body" | grep -Eiq 'Cache-impact:\s*(n/a|todo|tbd)\b'; then
  echo "FAIL: Cache-impact must be descriptive (not n/a/todo/tbd). Use 'none — reason' if truly none." >&2
  exit 1
fi

if ! echo "$body" | grep -Eq 'Cache-impact:\s*\S'; then
  echo "FAIL: missing 'Cache-impact: <level> — <reason>' in PR body" >&2
  exit 1
fi
if ! echo "$body" | grep -Eq 'Cache-guard:\s*\S'; then
  echo "FAIL: missing 'Cache-guard: <test or rationale>' in PR body" >&2
  exit 1
fi

# "none" alone is ok if it has a reason after em-dash/hyphen
if echo "$body" | grep -Eiq 'Cache-impact:\s*none\s*$'; then
  echo "FAIL: Cache-impact: none must include a reason (e.g. none — docs only)" >&2
  exit 1
fi

echo "check-cache-impact: PR body notes present (ok)"
exit 0
