#!/usr/bin/env bash
# Compatibility entrypoint for the product-level DeepSeek agent smoke.
# The E0 harness owns the evidence contract: it always builds this checkout,
# requires a real tool side effect, verifies JSONL persistence, and resumes the
# same session in a second Lumen process. Model text containing a marker alone
# is never accepted as tool execution proof.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
exec "$ROOT/scripts/smoke-deepseek-v4-e0.sh" --live
