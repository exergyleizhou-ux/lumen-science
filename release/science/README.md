# Lumen Science release builds

## Produce multi-platform artifacts

```bash
cd packs/science
make release
# reads VERSION from repo root (currently 1.0.0-rc.1)
```

Output: `packs/science/dist/science-release/`

| Artifact | Platforms |
|----------|-----------|
| `lumen-science-{os}-{arch}` | darwin/linux arm64+amd64, windows amd64 |
| `lumen-mcp-*` | same matrix (artifacts, notebook, reviewer, http_bridge, skill-migrate) |
| archives | `.tar.gz` / `.zip` per platform |
| `SHA256SUMS` | all binaries + archives |

Binaries are **gitignored** (large). Checksums for the last release candidate:

```text
outputs/release/1.0.0-rc.1/SHA256SUMS
outputs/release/1.0.0-rc.1/MANIFEST.json
```

## Verify

```bash
cd packs/science/dist/science-release
shasum -a 256 -c SHA256SUMS
```

## Install one platform (example macOS arm64)

```bash
tar -xzf lumen-science-1.0.0-rc.1-darwin-arm64.tar.gz
install -m 755 lumen-science-darwin-arm64 ~/.local/bin/lumen-science
lumen-science version
```

Or from source:

```bash
./scripts/install-science.sh
```
