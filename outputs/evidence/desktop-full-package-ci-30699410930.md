# macOS full Desktop package — GitHub CI evidence

## Exact CI result

- Repository: `exergyleizhou-ux/lumen-science`
- Workflow: `Desktop CI`
- Run: [`30699410930`](https://github.com/exergyleizhou-ux/lumen-science/actions/runs/30699410930)
- Job: `Desktop macOS full package (unsigned)` (`91367614251`)
- Job result: `success`
- PR head: `67ed5c7446d2e0c5c9fca56c1be47859106d0135`
- Tested GitHub merge candidate: `c7dc2eeaffa2e00baa99071c910103b82e1ef857`

GitHub Actions tests a pull request's merge candidate rather than blindly
trusting its branch head. The evidence therefore records both identifiers. The
status receipt is invalidated if `packs/science-desktop` or
`.github/workflows/desktop-ci.yml` changes after the anchored PR head.

## Artifact receipt

The job built `npm run dist:full`, verified the full main/preload/renderer
inputs, verified the bundle identifier and display name, and ran strict deep
`codesign --verify` before uploading `desktop-full-package-provenance`.

| Item | SHA-256 |
| --- | --- |
| Downloaded provenance text | `073ca28bd1158017857c78a2435b81ff4da61194ad06c9ea58d3a824d6ea2005` |
| `Contents/MacOS/Lumen Science Desktop` | `1cadd395ec07264693deb570216270eafc2130a44c4ce1553b809545f1e48afb` |
| `Contents/Resources/app.asar` | `1845b50233c1a31199627b9e65e90bf97adbff6306942ce14f579c4252b04071` |

## Non-claims

The bundle is ad-hoc signed with no Team Identifier. This is a CI package
integrity proof, not Developer ID signing, notarization, a clean-machine
installation proof, an installer, a release asset, or a published product.
