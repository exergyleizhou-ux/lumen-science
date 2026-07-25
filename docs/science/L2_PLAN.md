# Lumen Science Level 2: Offline Product Loop — Execution Plan

**Date:** 2026-07-25
**Authority:** Rust Lumen TUI (sole execution/approval/verification)
**Repo:** `/Users/lei/code/lumen/`

## Status

| Level | Scope | Status |
|-------|-------|--------|
| L1 (DS-0~38) | 42 connector full admission | ✅ Complete |
| L2 (DS-39~47) | Offline product loop | 🚧 In Progress |
| L3 (DS-48~58) | CI / cross-platform / release | ❌ Pending |

## Architecture Decision: Go MCP + Rust Core

All L2 MCP servers are **Go stdio MCP servers** in `packs/science/mcp/`, following the
existing pattern (c2d, chembl, geo, oasis, pubmed). They communicate with the Rust
core via the existing loopback API (`agent/crates/codegen/xai-grok-science/src/api.rs`).

Rationale:
1. The Go MCP framework already exists and is battle-tested with 5 servers
2. The Rust core holds durable storage, connector registry, review, and transport
3. Go MCP servers are independent binaries, buildable with `go build`
4. This avoids adding network/process management deps to the Rust core

---

## Phase 1: Foundation (parallel)

### DS-39: Artifacts MCP Server

**Location:** `packs/science/mcp/artifacts/`

**Tools:**
- `artifact_write` — Register a generated file to the current run (safe, agent can't write workspace directly)
- `artifact_list` — List registered artifacts for a project/run
- `artifact_read` — Read artifact bytes through ownership guard
- `artifact_preview` — Get content-sniffed preview metadata

**Integration:** Calls Rust loopback API (existing `api.rs`) to read/write artifacts through the durable store.
Starts its own loopback API on ephemeral port, discovered by the Go server.

**Files to create:**
- `tools.go` — Tool definitions + handlers
- `server.go` — Main entry, connects to Rust API
- `server_test.go` — Fixture-backed tests
- `standalone/cmd/artifacts/main.go` — Binary entry point

### DS-40: Python Notebook MCP Server

**Location:** `packs/science/mcp/notebook/`

**Tools:**
- `notebook_execute` — Execute a Python code cell, return stdout/stderr/result
- `notebook_restart` — Restart the kernel (fresh Python interpreter)
- `notebook_state` — Get current kernel state, active packages, running cells
- `notebook_shutdown` — Graceful shutdown
- `manage_packages` — Install/remove packages via pip in the notebook environment
- `manage_environments` — List/create conda environments for isolation

**Integration:** Uses existing `lab/runtime/python.go` for Python resolution and `lab/runtime/conda.go`
for conda management. Kernel is a long-running Python subprocess with JSON-RPC over stdin/stdout.

**Files to create:**
- `tools.go` — Tool definitions + handlers
- `kernel.go` — Python subprocess manager (start, execute, restart, shutdown)
- `kernel_test.go`
- `server.go` — Main entry
- `standalone/cmd/notebook/main.go` — Binary entry point

---

## Phase 2: Quality & Bridge (sequential after Phase 1)

### DS-41: Reviewer MCP Server

**Location:** `packs/science/mcp/reviewer/`

**Tools:**
- `start_review` — Begin a host-verification review against a run
- `review_status` — Get structured pass/warn/fail report
- `approve_fix` — Accept a fix and re-verify

**Integration:** Calls Rust `review.rs` verification logic. Uses the loopback API.

### DS-42: HTTP Bridge

**Location:** `packs/science/mcp/http_bridge/`

A thin HTTP server that wraps any stdio MCP server behind Bearer-auth'd HTTP.
For external tools that can't use stdio.

**Tools:** (exposed as HTTP endpoints)
- POST `/tools/call` — Forward to underlying MCP server

---

## Phase 3: Content & UI (parallel)

### DS-43: Skills Migration

Port ~370 MIT/BSD/Apache skills from synsci into ACP extension descriptors.
Exclude GPL, unknown-license, and commercial-database-restricted skills.

### DS-44: Science Renderers (9)

Port the 9 renderers from synsci SolidJS frontend:
- ProteinStructure (Mol*), Chem2D (RDKit.js), GenomeTrack (IGV.js)
- KaTeX/LaTeX, PdfViewer, SequenceViewer, MsaViewer, ImageView

### DS-45: Motif Integration

Integrate Motif molecular biology workbench as 10th renderer (self-contained HTML).

### DS-46: ArtifactRenderer Framework

Registration system + lightweight web view for rendering artifacts by kind.

---

## Phase 4: Integration

### DS-47: End-to-End Integration Tests

Full pipeline test: connector search → notebook analysis → artifact write → reviewer verify.

---

## Phase 5 (Level 3): CI / Cross-Platform / Release

### DS-48~58: CI pipeline, macOS signed binary, Windows/Linux, SBOM, live proof, release

---

## Build Commands

```bash
# Build individual MCP servers
cd packs/science
go build -C standalone -o ../lumen-mcp-artifacts ./cmd/artifacts
go build -C standalone -o ../lumen-mcp-notebook ./cmd/notebook

# Test
go test -C standalone ./...
go vet -C standalone ./...
```
