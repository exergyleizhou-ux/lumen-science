# 1.0.1 Release checklist (P0 honesty)

**Goal:** Clean **Science CLI/MCP** release from a fully green commit with
workflow-built assets. Do **not** re-decorate `v1.0.0` provenance.

**Automation (repo):**

```bash
# version source
cat packs/science/VERSION   # 1.0.1

# preflight (no publish)
bash scripts/science-release-preflight.sh
python3 scripts/science_release_contract.py preflight --tag v1.0.1

# publish path: push annotated tag v1.0.1 → workflow "Science Release"
# Core "Release Lumen" no-ops for science-only tags (see release.yml classify job)
```

**Status note (2026-07-26):** `v1.0.0` may now list many GitHub assets; the
remaining gap is **tag → protected CI → builder provenance**, not mere file
presence. Desktop remains `1.1.0-dev` and is **out of scope** for `v1.0.1`.

**Automation (scaffold only):**

```bash
cd packs/science-desktop
npm run check:release   # osf8-release-check.mts — config + honesty
npm run pack:check      # full authority suite (OSF-2…8)
npm run smoke:live      # optional lumen-science binary on PATH
```

IPC honesty surface: `release:checklist-status` always reports  
`binariesUploaded=false` and `notarizationComplete=false` until release-ops flips process (not code).

## Must ship for ACCEPT “downloadable install”

1. CI builds platform archives (darwin-arm64/amd64, linux-amd64/arm64, windows-amd64).
2. Upload every path listed in SHA256SUMS to the GitHub Release.
3. Upload matching GPG/minisign signature.
4. Smoke: download → verify hash → run `lumen-science --version` on each platform.
5. Desktop (when certs available): electron-builder DMG/AppImage/Setup + update feed.

## Desktop pack (local)

```bash
cd packs/science-desktop
npm run pack:check
npm run dist         # electron-builder --dir (unsigned scaffold)
```

Notarization / Developer ID: deferred until org certs exist (documented, not fake-green).

## Do not claim

- 5.0 GA
- Full OSF-0…9 product GA
- Device / HIL ready
- “Notarization complete” without stapled tickets
- “All platforms released” without uploaded assets