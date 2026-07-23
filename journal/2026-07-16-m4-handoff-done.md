# M4 Handoff Done — 2026-07-16

## What was done

### 1. lumen-verify crate
- Agent crate: `agent/crates/codegen/lumen-verify/`
- 11 unit tests green (cargo test -p lumen-verify)
- Language detection (Go/Python/TypeScript from file extensions)
- Step generation (build → vet → test per language)
- Step execution (runs commands, skips missing tools)
- Diagnostics parsing (Go compiler, ruff/pytest, tsc/jest)
- Repair state machine (max 3 cycles, feedback formatting)
- Wired into workspace (agent/Cargo.toml members)

### 2. Coding eval ≥20 tasks
- 20 task directories under evals/tasks/
  - T01-T08: migrated from ~/lumen (Go: average, stack, reverse, binary-search, counter-race, stringer, nilmap, multifile)
  - T09-T12: new (Python: divzero, json-merge; TypeScript: optional-chain, async-race)
  - T13-T20: new (Go: context-cancel, error-wrap, http-timeout, multi-pkg, fix-only-regression, readme-driven, flaky-to-stable; Python: path-traversal-fix)
- scripts/eval-coding.sh: harness runner with 30s per-task timeout
- evals/BASELINE.md: task index + baseline run table (harness-only for now)

### 3. Vertical packs
- packs/science/: 180+ Go files (proxy, gui, lab, mcp, native, etc.) + README.md
- packs/oasis/: 20 Go files (check, lock, verify, templates) + README.md
- packs/quant/: 15 Go files (attest, backtest, cert) + README.md
- scripts/doctor-verticals.sh: exits 0, detects all 3 packs

### 4. Regression
All 4 smoke scripts still green:
- assert-defaults.sh ✅
- smoke-security.sh ✅ (lumen-guard 11 tests)
- smoke-m2.sh ✅ (lumen-discipline 12 tests)
- parity-run.sh ✅ (CC_PARITY 41/41, 100%)

## How to verify (验收清单)

```bash
cd /Users/lei/code/lumen
export PROTOC=/opt/homebrew/bin/protoc
export PATH="/opt/homebrew/bin:$HOME/.local/bin:$HOME/.cargo/bin:$PATH"

# A. Regression
./scripts/assert-defaults.sh
./scripts/smoke-security.sh
./scripts/smoke-m2.sh
./scripts/parity-run.sh

# B. M4 exit checks
ls evals/tasks | wc -l               # ≥20
./scripts/eval-coding.sh             # exit 0
cat evals/BASELINE.md                # exists
cargo test -p lumen-verify           # 11 tests green
cat packs/science/README.md          # ≤3 steps
cat packs/oasis/README.md            # ≤3 steps
cat packs/quant/README.md            # ≤3 steps
./scripts/doctor-verticals.sh        # exit 0

# C. lumen-verify Go demo (optional, requires Go)
cargo test -p lumen-verify --lib
```

## Not done (outside scope)
- Live DeepSeek agent eval run (harness-only verified)
- M5 polish / 10-min install video
- M6 v1.0 release / 15-day journal
