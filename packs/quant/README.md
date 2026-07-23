# Lumen Quant Pack

Quantitative algorithm framework — attestation, backtesting, certificate generation.

## Quick Start (≤3 steps)

### Step 1: Verify library integrity
```bash
cd packs/quant && go test ./...
```

### Step 2: Scaffold a quant algorithm
```bash
cd packs/quant && go run . init myquant --template backtest
```

### Step 3: Run verification
```bash
cd myquant && go run ../. verify .
```

## More
- `go run . attest` — generate attestation
- `go run . backtest` — run backtest framework

## Source
Migrated from `~/lumen/internal/quant/` (legacy Go agent).
Go library package; invoke via `go run` or embed in MCP tool.
