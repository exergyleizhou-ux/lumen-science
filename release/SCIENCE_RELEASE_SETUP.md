# Science release setup — what only the repository owner can do

Everything in this file requires credentials or GitHub settings that live
outside the source tree, so it cannot be automated from a pull request. Until
these are configured, science release assets carry **checksums but no
signatures**: a user can verify the bytes match what the release publishes, but
cannot verify *who published them*. That gap is recorded honestly in
[`docs/science/status/current.json`](../docs/science/status/current.json) under
`release.openGaps`.

Work through the tiers in order. **Tier 1 costs nothing** and closes the
publisher-identity gap. Tiers 2 and 3 cost money and are only needed for
desktop distribution.

---

## Tier 1 — free, do this first

### 1.1 Generate the science signing key

Use minisign, matching the scheme Lumen Core already uses (see
[`release/README.md`](README.md)). Keep it a **separate key** from the core
release key: the two lines ship independently, and a compromise of one should
not invalidate the other.

```bash
brew install minisign   # or: apt install minisign
```

```bash
minisign -G -p lumen-science.pub -s lumen-science.key -c "lumen-science-v1"
```

You will be asked for a passphrase. **Leave it empty** — CI cannot type one,
and the private key is protected by GitHub's secret storage rather than by a
passphrase the workflow would have to hold anyway.

Store `lumen-science.key` somewhere durable and offline (a password manager
entry, not a repo, not a cloud drive that syncs to a shared machine). Losing it
means every future release changes identity and users must re-trust.

### 1.2 Load the key into GitHub

The **private** key goes in a secret; the **public** key goes in a variable.
The workflow derives the public key from the secret and compares it against the
variable, so a mismatched or swapped key fails the build instead of silently
signing with the wrong identity.

```bash
gh secret set LUMEN_SCIENCE_MINISIGN_PRIVATE_KEY \
  --repo exergyleizhou-ux/lumen-science < lumen-science.key
```

```bash
gh variable set LUMEN_SCIENCE_MINISIGN_PUBLIC_KEY \
  --repo exergyleizhou-ux/lumen-science < lumen-science.pub
```

### 1.3 Create the protected environment

`science-release.yml` already declares `environment: lumen-science-release` on
the publish job. The environment itself must exist, or publishing fails.

Create it in **Settings → Environments → New environment**, named exactly
`lumen-science-release`, then set:

- **Required reviewers** — add yourself. This is what makes a release a
  deliberate act rather than a side effect of pushing a tag.
- **Deployment branches and tags** — restrict to `main` and the tag pattern
  `v*`.

Only this environment holds the signing secret, so no other job can reach it.

### 1.4 Make published releases immutable

**Settings → General → Releases → enable "Immutable releases".**

The workflow already refuses to overwrite a published release, but that is a
check in our code. This makes it a platform guarantee that holds even if the
workflow is later changed.

### 1.5 Protect the tags

**Settings → Rules → Rulesets → New tag ruleset**, targeting `v*`:

- Restrict creation to repository administrators
- Block force pushes and deletion

A release is bound to the commit its tag points at. If a tag can be moved after
publication, that binding means nothing.

### 1.6 Turn on the free security features

**Settings → Code security**, enable:

- Private vulnerability reporting — this is the channel
  [`SECURITY.md`](../SECURITY.md) tells reporters to use
- Secret scanning, **and push protection**
- Dependabot alerts and security updates —
  [`.github/dependabot.yml`](../.github/dependabot.yml) is already committed
- CodeQL default setup for Go, JavaScript/TypeScript and (if offered) Rust

### 1.7 Verify it worked

```bash
gh workflow run "Science Release" --repo exergyleizhou-ux/lumen-science -f dry_run=true
```

A dry run builds and signs without publishing. It must fail loudly if the
secret is missing — a dry run that "passes" without signing material means the
gate is not wired.

---

## Tier 2 — Apple notarization (~$99/year)

Needed only to ship a macOS desktop build users can open without right-click →
Open. **Not** needed for the CLI.

1. Enrol in the Apple Developer Program.
2. Create a **Developer ID Application** certificate; export it as `.p12`.
3. Create an app-specific password for notarytool.
4. Add as repository secrets:

```
APPLE_CERTIFICATE_P12        base64 of the .p12
APPLE_CERTIFICATE_PASSWORD   the .p12 export password
APPLE_TEAM_ID
APPLE_ID
APPLE_APP_PASSWORD           the app-specific password
```

Today `packs/science-desktop/build/adhoc-sign.cjs` ad-hoc signs and the builder
config sets `notarize: false`. That is deliberate and honest — do not flip
`notarize: true` before the certificate exists, or the build will claim
something untrue.

## Tier 3 — Windows Authenticode (~$200–400/year)

Needed only for a Windows desktop installer that does not trigger SmartScreen.

An **EV** certificate requires a hardware token and cannot be used from hosted
CI without a cloud signing service (Azure Trusted Signing, SSL.com eSigner). A
standard OV certificate works in CI but still accumulates SmartScreen
reputation from zero.

---

## What is still not covered after all three tiers

Be precise about this when describing the release to anyone:

| Property | After Tier 1 | Notes |
|---|---|---|
| Bytes match what was published | yes | SHA256SUMS, verified by the installer |
| Tag is bound to the built commit | yes | remote tag peel + manifest check |
| Published assets cannot be replaced | yes | immutable publish + platform setting |
| Archives are byte-reproducible | yes | `scripts/repro-archive.sh` |
| Publisher identity is provable | yes | minisign signature |
| **Per-asset SBOM** | **no** | `scripts/generate-release-sbom.sh` is hard-coded to the four core assets; it needs extending to the science targets |
| **Build provenance attestation** | **no** | needs `actions/attest-build-provenance`; a signature proves who signed, not what built it |
| **Independent rebuild comparison** | **no** | reproducibility is implemented but nobody has rebuilt a published tag and compared |

The first two are code changes, not credentials — they can be done in a PR and
are tracked as follow-on work. The third needs someone to actually do it once.
