# Lumen Phase F release foundation

This is the Phase F release foundation, not the end-to-end update system from
Phase A. No current `lumen update` or `lumen upgrade` path consumes these assets
yet. Publishing a valid release therefore does not by itself complete the user
update loop.

The release workflow has one canonical version source: the repository-root
`VERSION` file. `scripts/release_version.py` requires the eight versioned Lumen
crates (including `xai-grok-version`) and `agent/Cargo.lock` to match it exactly.
A release tag must be exactly `v<version>`.

Release files are signed with Minisign's prehashed Ed25519 format under the
stable identity `lumen-release-v1`. The signed manifest binds the tag, full git
commit, four platform binaries, each binary's SPDX SBOM, SHA-256 digests, sizes,
and the release public-key fingerprint. A future updater must pin that public
key or fingerprint; downloading `lumen-release.pub` beside a release is not by
itself a trust decision.

## Required repository configuration

- Actions secret `LUMEN_MINISIGN_PRIVATE_KEY`: the complete **unencrypted**
  Minisign secret-key file. Generate it on an offline trusted machine with
  `minisign -G -W`; never commit it.
- Actions variable `LUMEN_MINISIGN_PUBLIC_KEY`: the complete matching two-line
  `minisign.pub` file. This is public and is emitted as `lumen-release.pub`.
- Protected GitHub environment `lumen-release`: configure required reviewers
  and restrict deployment branches/tags before enabling this workflow.
- Repository **Settings → Releases → Enable release immutability** must be on
  before the first real run. GitHub applies this only to future releases. The
  workflow cannot preflight that repository setting through a documented API,
  so it fails closed unless the published Release reports `isImmutable=true`.
- Protect release tags matching `v*` and require signed tags in repository or
  organization rules. The workflow can verify commit ancestry and tag/commit
  equality, but GitHub repository settings remain the authority for who may
  create or move a tag.

The environment-gated publish job does not check out or execute repository
scripts. It downloads only the immutable unsigned release bundle, installs a
checksum-pinned Minisign 0.12 binary, then derives a public key from the private
key and requires it to equal the repository variable before signing. Missing,
encrypted, or mismatched keys fail closed. GitHub's built-in token receives
`contents: write` only in this final job, and the private key exists only in the
signing step environment.

GitHub Actions permissions are job-scoped, not step-scoped. Consequently the
final job's built-in token has `contents: write` for the whole job even though
`GH_TOKEN` is exposed only to the release step. The protected environment,
protected/signed tag rules, default-branch ancestry check, no-checkout publish
job, pinned actions, and immutable artifact digest are the residual trust
controls; repository administrators must configure those GitHub-side gates.

The publisher validates every draft asset's remote GitHub `digest`, byte size,
and upload state before publishing. Publishing is the final mutation: it never
tries to move an immutable Release back to draft. If the publish command loses
its response, the workflow reconciles the authoritative Release state and only
accepts an exact, published, immutable tag/asset match. A rerun against that
exact immutable Release succeeds without upload or mutation; every other
published state fails closed for manual inspection.

Run the local contract fixtures without any GitHub secret or user installation:

```sh
./scripts/test-release-prep.sh
./scripts/test-release-contract.sh
```

For an actual release, run `scripts/release.sh BUMP` from a clean, up-to-date
`main` checkout. The script synchronizes versions, updates `CHANGELOG.md`, runs
the local release gates, commits the explicit release files, creates and
verifies a signed tag, and atomically pushes the commit and tag. The workflow
then verifies that the tag resolves to that commit and publishes only Lumen
assets.

## Remaining external Phase F gates

Four-platform native builds and the real GitHub Release still require a
repository run after the environment, tag rules, public key variable, and
private key secret are set.

The current macOS artifacts are not Apple Developer ID signed or notarized;
Minisign authenticates release bytes but does not replace Gatekeeper signing.
The `release-dist` profile intentionally retains debug information and symbols
(`debug=1`, `strip=false`) for later sidecar extraction, so a production policy
for stripping, publishing debug sidecars, Apple codesigning, and notarization
remains an external release gate. Until those gates and Phase A's updater are
implemented and exercised, this foundation must not be described as
release-ready or as a complete end-user update system.
