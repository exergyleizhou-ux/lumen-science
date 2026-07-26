# 1.0.1 Release checklist (P0 honesty)

Audit finding: `v1.0.0` has SHA256SUMS for ~35 assets but GitHub Release only hosts metadata.

## Must ship for ACCEPT “downloadable install”

1. CI builds platform archives (darwin-arm64/amd64, linux-amd64/arm64, windows-amd64).
2. Upload every path listed in SHA256SUMS to the GitHub Release.
3. Upload matching GPG/minisign signature.
4. Smoke: download → verify hash → run `lumen-science --version` on each platform.
5. Desktop (when certs available): electron-builder DMG/AppImage/Setup + update feed.

## Desktop pack (local)

```bash
cd packs/science-desktop
npm run pack:check   # authority tests + typecheck
npm run dist         # electron-builder --dir (unsigned scaffold)
```

Notarization / Developer ID: deferred until org certs exist (documented, not fake-green).

## Do not claim

- 5.0 GA
- Full OSF-3…9 complete
- Device / HIL ready
