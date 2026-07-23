#!/usr/bin/env bash
# L0 compatibility entrypoint. The canonical E0 harness owns current-checkout
# build validation and formal DeepSeek routing; L0 does not claim L2/L3 scope.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
exec "$ROOT/scripts/smoke-deepseek-v4-e0.sh" --live
