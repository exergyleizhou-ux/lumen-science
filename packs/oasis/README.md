# Lumen Oasis Pack

C2D (Compute-to-Data) author toolchain — write, verify, and publish privacy-preserving algorithms.

## Quick Start (≤3 steps)

### Step 1: Verify library integrity
```bash
cd packs/oasis && go test ./...
```

### Step 2: Use via CLI wrapper
```bash
cd packs/oasis && go run . templates
```

### Step 3: Scaffold an algorithm
```bash
cd packs/oasis && go run . init myalgo --template stats
cd myalgo && go run ../. check .
```

## More
- `go run . verify .` — source ⇄ provenance lockfile match
- `go run . publish .` — build → check → deploy + register

## Source
Migrated from `~/lumen/internal/oasis/` (legacy Go agent).
Go library package; invoke via `go run` or embed in MCP tool.
