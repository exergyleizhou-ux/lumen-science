# M4 acceptance fix — 2026-07-16

## Gaps found in verification
1. `eval-coding.sh` used GNU `timeout` → macOS false BUILD-ERR / fake green
2. Tasks `14-go-error-wrap` and `20-flaky-to-stable` unexpectedly PASSed when "broken"
3. `doctor-verticals` only scanned pack root `*.go` → science false "no sources"
4. No one-shot Go verify demo (bad→fail / good→ok)

## Fixes
- Portable `eval-coding.sh` (no timeout); exit 1 if any unexpected PASS / skip / <20 broken
- Rewrote T14/T20 workspaces + tests so initial state is deterministically red
- Recursive Go count in `doctor-verticals.sh`; require science ≥10 go files
- `lumen-verify` CLI bin + `scripts/smoke-verify.sh`

## Verify
```bash
./scripts/eval-coding.sh
./scripts/doctor-verticals.sh
./scripts/smoke-verify.sh
./scripts/smoke-security.sh && ./scripts/smoke-m2.sh && ./scripts/parity-run.sh
```
