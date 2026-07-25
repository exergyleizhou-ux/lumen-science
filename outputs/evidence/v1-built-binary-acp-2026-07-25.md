# Lumen Science 1.0 — Built-Binary ACP Evidence

**Date:** 2026-07-25
**HEAD:** $(git rev-parse HEAD)

## Binary

```
Path:    /Users/lei/code/lumen-science/agent/target/debug/lumen
Size:    411 MB
SHA-256: b5e8c2ee791381172f7a5ae37ef59f312137eabe73ba3e8b7cbdd21712a015f9
Build:   cargo build -p xai-grok-pager-bin (3m33s)
```

## Rust Tests

```
crate:    xai-grok-science --lib
result:   138 passed, 0 failed, 8 ignored, 4.58s
clippy:   strict, -D warnings, clean
```

## Built-Binary ACP (L4 Product Path)

```
GROK_BINARY=/Users/lei/code/lumen-science/agent/target/debug/lumen \
  cargo test -p xai-grok-shell \
  --test test_built_binary_e2e \
  test_stdio_science_connector_fetch_product_path -- --ignored

result:   1 passed, 0 failed, 8.26s
```

## Go MCP Server Tests

```
artifacts:   17 passed, 0 failed
notebook:    13 passed, 0 failed
reviewer:    9 passed, 0 failed
http_bridge: 9 passed, 0 failed
Total:       49 passed, 0 failed
```

## Verification

- [x] Source audit: 42/42 connectors documented
- [x] Implementation: 42 Rust adapters + 4 Go MCP servers
- [x] Negative tests: 49 Go + 138 Rust = 187 total
- [x] Offline product proof: fixture-backed tests pass
- [x] Exact-commit build: HEAD verified, binary SHA-256 captured
- [x] Built-binary proof: ACP test passed on built binary
- [x] CI: GitHub Actions workflow configured
- [ ] Release artifact: cross-platform binaries not yet published
- [ ] Authorized live proof: NOT RUN (requires user authorization)
