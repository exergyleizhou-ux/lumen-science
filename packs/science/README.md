# Lumen Science Pack

Science vertical for [Lumen](https://github.com/exergyleizhou-ux/lumen) —
42 scientific database connectors, offline product loop (MCP servers),
and web-based artifact renderers.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                  Rust Lumen SessionActor                 │
│              (sole execution & permission authority)     │
├─────────────────────────────────────────────────────────┤
│  42 Connectors  │  Artifacts  │  Notebook  │  Reviewer  │
│  (pubmed,       │  MCP        │  MCP        │  MCP        │
│   chembl, …)    │             │             │             │
├─────────────────────────────────────────────────────────┤
│           HTTP Bridge  │  9 Science Renderers            │
└─────────────────────────────────────────────────────────┘
```

## MCP Servers

| Server | Description | Tests |
|--------|-------------|-------|
| `lumen-mcp-artifacts` | Durable artifact storage with SHA-256 integrity | 17 |
| `lumen-mcp-notebook` | Persistent Python kernel (JSON-RPC) | 13 |
| `lumen-mcp-reviewer` | Artifact verification and review workflow | 9 |
| `lumen-mcp-http_bridge` | HTTP→stdio MCP proxy with Bearer auth | 9 |

## Quick Start

```bash
# Build all MCP servers
cd packs/science
make all

# Run tests
make test

# Cross-compile for all platforms
make cross

# Package release
make release
```

## Individual MCP Server Usage

```bash
# Artifacts MCP — persistent storage
./build/lumen-mcp-artifacts
# Tools: artifact_write, artifact_list, artifact_read, artifact_preview

# Notebook MCP — Python kernel
./build/lumen-mcp-notebook
# Tools: notebook_execute, notebook_restart, notebook_state,
#        notebook_shutdown, manage_packages, manage_environments

# Reviewer MCP — integrity verification
./build/lumen-mcp-reviewer
# Tools: start_review, review_status, approve_fix

# HTTP Bridge — expose MCP over HTTP
BRIDGE_TARGET_COMMAND=./build/lumen-mcp-artifacts \
BRIDGE_BEARER_TOKEN=secret \
BRIDGE_PORT=9090 \
  ./build/lumen-mcp-http_bridge
```

## Science Renderers

Self-contained HTML pages served via embed.FS:

| Renderer | MIME Types | Library |
|----------|-----------|---------|
| Protein 3D | `chemical/x-pdb` | Mol* |
| Chem 2D | `chemical/x-smiles` | RDKit.js |
| Genome Browser | `application/x-bed` | IGV.js |
| LaTeX | `application/x-latex` | KaTeX |
| PDF Viewer | `application/pdf` | pdfjs-dist |
| Sequence Viewer | `text/x-fasta` | Canvas |
| MSA Viewer | `application/x-stockholm` | Canvas |
| Image Viewer | `image/png`, `image/jpeg` | Native |
| Motif | `application/x-motif` | Self-contained |

## Connector Status

```
42 total: 40 implemented, 2 rejected, 0 unresolved

Rejected:
  BioGRID — credential in URL (rejected-unsafe-or-duplicate)
  KEGG    — commercial license required (rejected-license-or-terms)
```

## Development

```bash
go test ./mcp/... -count=1        # Unit tests (49 tests)
go vet ./mcp/... ./renderers/...  # Lint
go build ./standalone/cmd/...      # Build all commands
```

## CI/CD

GitHub Actions (`.github/workflows/science-ci.yml`):
- Go test on ubuntu + macos
- Cross-compile (darwin/linux/windows, amd64/arm64)
- E2E pipeline test
- Release artifact build with checksums

## License

Apache 2.0 derivative. See `../LICENSE` and `../NOTICE`.
