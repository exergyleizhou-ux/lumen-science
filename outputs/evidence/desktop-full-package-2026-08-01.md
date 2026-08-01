# macOS full Desktop package — local evidence (2026-08-01)

## Scope and source

- Repository: `lumen-science`
- Source commit: `07310811b0def9a4a36b768b88c325aaca995d9b`
- Host: local macOS arm64
- Command: `cd packs/science-desktop && npm run dist:full`
- Command result: exit `0`

This was an offline local package proof. It is not an exact-head CI proof, a
clean-machine installation proof, a signed release, a notarization result, or
an authorization to publish a release.

## What the command actually built

`electron-vite build` completed for the real Desktop graph, including the main
process, preload and 5,955 transformed renderer modules. `electron-builder
--dir` then produced:

`packs/science-desktop/dist/mac-arm64/Lumen Science Desktop.app`

The packaged ASAR contains all required full-build entries:

- `/out/main/index.js`
- `/out/preload/index.js`
- `/out/renderer/index.html`

The bundle's `Info.plist` identifies the product as `Lumen Science Desktop`
with bundle identifier `com.exergyleizhou-ux.lumen-science-desktop`.

## Reproducible local artifact observations

| Item | SHA-256 |
| --- | --- |
| `Contents/MacOS/Lumen Science Desktop` | `6b6039bbb0134a7f641a3d5039d45227dce2772f0cd6539a4c346444952bf576` |
| `Contents/Resources/app.asar` | `2accd4900d40ca9e4444f0a7930dc65ca4d35b0f0cbbe2eb543a25a94d9b1dd6` |

## Deliberate non-claims and observed rejection

`codesign -dv` reports `Signature=adhoc` and `TeamIdentifier=not set`.
`spctl -a -vv` rejects the application. This is expected for this development
bundle and proves neither Developer ID signing nor notarization. The builder
also reported its default Electron icon and duplicate-dependency diagnostics;
they are release-quality follow-ups, not evidence of a signed installable
product.

The next package gate is to make this exact full-build proof a CI artifact,
then separately establish signing, notarization, clean-machine installation,
rollback and release provenance. None of those gates is passed by this record.
