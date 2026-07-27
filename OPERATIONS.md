# Operations

How to release, verify, diagnose and roll back Lumen Science. Written for
whoever is on the hook when something is wrong, so it states what is *not*
covered as plainly as what is.

Machine-readable current state:
[`docs/science/status/current.json`](docs/science/status/current.json). When
this document and that file disagree, the file is right.

---

## Components and what breaking each one means

Three independently versioned components
([`docs/VERSIONING.md`](docs/VERSIONING.md)):

| Component | Blast radius of a bad release |
|---|---|
| Science CLI/MCP (`packs/science/VERSION`) | Users' installed `lumen-science` binary. Rollback is a one-line command. |
| Lumen Core (`agent/.../xai-grok-pager`) | The `lumen` agent binary. |
| Desktop (`packs/science-desktop`) | **Pre-GA.** No auto-update channel exists, so a bad build reaches only people who download it manually. |

---

## Releasing the science CLI

### Preconditions

1. `packs/science/VERSION` bumped, committed to `main`.
2. Signing configured — see
   [`release/SCIENCE_RELEASE_SETUP.md`](release/SCIENCE_RELEASE_SETUP.md).
   Without it the release job **fails** rather than silently publishing
   unsigned.
3. `main` green: `science-ci` (gates, Go tests, Rust science) and
   `supply-chain`.

### Cut it

```bash
git tag -a v1.0.2 -m "Lumen Science CLI/MCP 1.0.2" && git push origin v1.0.2
```

The tag push triggers `science-release.yml`. The publish job waits on the
`lumen-science-release` protected environment, so it will sit pending until
approved.

### What the pipeline enforces

You do not need to check these by hand; they fail the build:

- tag → commit → `MANIFEST.json` `git_commit` all agree (remote tag peeled up
  to 8 levels, so annotated tags resolve)
- every asset re-hashed against `SHA256SUMS` in the publish job, not trusted
  from the build job
- an existing release is **never** overwritten — a re-run either creates it or
  verifies it is byte-identical and exits 0
- assets, `SHA256SUMS` and `MANIFEST.json` are signed, and each signature is
  verified against the *pinned* public key
- per-asset SPDX SBOM, and build provenance attestation

### After publishing

```bash
gh attestation verify lumen-science-1.0.2-linux-amd64.tar.gz \
  --repo exergyleizhou-ux/lumen-science
minisign -Vm SHA256SUMS -p lumen-science.pub
sha256sum -c SHA256SUMS
```

Then **rebuild the tag and compare** — see Known gaps below. This is the one
verification nobody has completed yet.

---

## Installing and rolling back

```bash
scripts/install-science.sh install          # latest release
scripts/install-science.sh install 1.0.2    # specific
scripts/install-science.sh verify           # what is installed, and its digest
scripts/install-science.sh rollback         # back to previous
scripts/install-science.sh uninstall
```

`install` verifies the digest against `SHA256SUMS`, checks the manifest names
the same tag and commit, and smoke-tests the extracted binary **before** it
becomes current — so a broken build never replaces a working install.

`verify` inspects the *installed product*. Do not use the CLI's own `doctor` for
this: it stats a source checkout and will report health on a machine with no
install at all.

### Verified against the real release

Exercised end to end against the published `v1.0.1`, not fixtures:

| Case | Result |
|---|---|
| Clean install of `v1.0.1` | digest ok, manifest binding ok (commit `4f75d7a29153`), smoke ok — reports `lumen-science 1.0.1` |
| Archive tampered (+4 bytes) | **rejected** at the digest check, before extraction |
| Archive tampered **and** `SHA256SUMS` rewritten to match | **rejected** by the manifest binding check |
| Either failure | nothing installed; no partial version directory left behind |

The second and third rows are the point of having two layers. An attacker who
can replace a release asset can usually replace the checksum file beside it, so
`SHA256SUMS` alone proves only internal consistency. The manifest carries the
digest independently, alongside the tag and commit.

`verify` also correctly warned when another `lumen-science` earlier on `PATH`
shadowed the install — a state that otherwise looks healthy while the user runs
a different binary entirely.

---

## Incidents

### A released binary is bad

1. Tell users to roll back: `scripts/install-science.sh rollback`.
2. Do **not** attempt to replace the assets. Published releases are immutable
   and the workflow refuses to overwrite them. Cut a new patch version.
3. If the release is dangerous rather than merely broken, mark it a
   pre-release in the GitHub UI so `install` stops selecting it as latest.

### Suspected key compromise

1. Rotate: generate a new key, update the secret **and** the repository
   variable. The workflow derives the public key from the secret and compares
   it to the variable, so a half-rotation fails closed rather than signing with
   an untrusted identity.
2. Publish the new public key and say which releases were signed with the old
   one.
3. Signatures do not expire and there is no revocation list. Rotation is an
   announcement, not a technical revocation — say so plainly rather than
   implying old signatures stop working.

### CI is red on `main`

Check the gate that failed; each is designed to name its own cause:

| Gate | What a failure means |
|---|---|
| `verify-science-status.py` | Status file is stale or contradicted by prose. Regenerate: `python3 scripts/generate-science-status.py` |
| `test-science-status.py` | The status *checker itself* stopped detecting corruption. Fix before trusting any green. |
| `verify-pinned-digests.py` | A workflow gained a digest with no recorded origin. |
| `desktop-source-graph.mts --check` | An excluded file became reachable — exclusion may now be hiding real type errors. |
| `science-machine-gates.sh` | Connector/skill lock integrity. |

---

## Data and backups

Project stores are **local files on the user's machine**. There is no server,
no hosted database and no backup service — so there is nothing for us to
restore on a user's behalf, and users must be told to back up their own project
directories.

`ScienceStore` writes are atomic (unique temp + `fsync` + rename) and it can
recover interrupted runs, marking pending approvals `Interrupted` rather than
silently re-running them.

---

## Telemetry

None. The desktop sends no usage data, and updating is disabled unless
Lumen-owned signing material is explicitly configured
(`packs/science-desktop/src/shared/update-policy.ts`). If telemetry is ever
added it must be opt-in and must never carry prompts, artifacts or secrets.

---

## Known gaps

Do not describe these as solved:

- **No published tag has been independently rebuilt and byte-compared.**
  Build inputs are now pinned (exact Go patch version, `CGO_ENABLED`), and
  archives are proven deterministic, but binary reproducibility is unproven.
  See [`docs/science/REPRODUCIBILITY_v1.0.1.md`](docs/science/REPRODUCIBILITY_v1.0.1.md)
  — the v1.0.1 attempt **failed**, and why.
- **v1.0.1 and earlier are unsigned**, with no SBOM or provenance. Those
  controls start at v1.0.2.
- **Desktop has no signed installer, no notarization, no update channel.**
- **No SLO, RTO or RPO.** This is a local-first tool with no service to have
  them for; adding numbers would be theatre.
- **Single maintainer.** CODEOWNERS marks the sensitive paths but cannot
  provide a second reviewer.
