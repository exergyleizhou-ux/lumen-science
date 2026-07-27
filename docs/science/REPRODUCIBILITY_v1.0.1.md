# Reproducibility attempt — v1.0.1 (LS5-R1-01)

**Result: NOT reproducible.** Recorded because a failed reproduction attempt is
evidence, and because the reasons are specific and fixed.

`docs/science/status/current.json` listed "reproducibility is implemented but no
published tag has been independently rebuilt and compared" as an open gap. This
is that comparison.

## Method

```bash
gh release download v1.0.1 --repo exergyleizhou-ux/lumen-science \
  --pattern 'lumen-science-linux-amd64*' --pattern 'SHA256SUMS' --pattern 'MANIFEST.json'

git worktree add --detach /tmp/wt 4f75d7a2915363bea09a053ee77a563372bc6631
cd /tmp/wt/packs/science
GOOS=linux GOARCH=amd64 CGO_ENABLED=0 \
  go build -trimpath -ldflags="-s -w -X main.version=1.0.1" \
  -o /tmp/rebuilt ./standalone/cmd/science
```

## Result

| | digest |
|---|---|
| Published `lumen-science-linux-amd64` | `32bb9da1411b67d6be69e692b9ed6a90bbcc162c03daf1dc30adc3bcb478ff09` |
| Local rebuild at the same commit | `252bdf00919750e8fd3e22573ed0b451e1e5af8a8b61dae02c2f00e86e289c4d` |

The published digest **does** match both `SHA256SUMS` and the `MANIFEST.json`
asset entry, so the release is internally consistent. It is the independent
rebuild that diverges.

## Diagnosis

`go version -m` on both binaries isolates three differences:

| Property | Published | Rebuild |
|---|---|---|
| Go toolchain | `go1.23.12` | `go1.23.4` |
| `CGO_ENABLED` | `1` | `0` |
| vcs stamping | `vcs.revision`, `vcs.time`, `vcs.modified` present | absent |

**The toolchain difference is the significant one.** The workflow pinned
`go-version: '1.23'`, which `actions/setup-go` resolves to the newest available
1.23.x *at build time*. So the compiler was a moving target: the same commit
built today and in three months would use different compilers and produce
different bytes, with nothing in the release recording which one was used.

The other two are narrower. `CGO_ENABLED=1` was the hosted-runner default
rather than a decision, and it is recorded in the binary. The missing vcs stamp
is an artefact of building from a detached worktree.

## Fixed

- `science-release.yml` pins `go-version: '1.23.12'` — the exact patch version,
  in both jobs.
- `CGO_ENABLED` is pinned explicitly on the release build rather than inherited.
- Archive creation was already made deterministic in LS5-R1-01
  (`scripts/repro-archive.sh`: normalised mtimes, sorted entries, zeroed
  ownership, `gzip -n`), which is a separate layer from the binaries above.

## What this does and does not establish

It does **not** retroactively make v1.0.1 reproducible. That release was built
by a pipeline that did not pin its compiler, and no change here alters the
bytes already published.

It establishes that the *next* release is built by a pipeline whose inputs are
pinned. Whether the output is actually reproducible is still unproven and stays
an open gap until someone rebuilds a tag published *after* these fixes and
compares. Pinning the inputs is necessary, not sufficient — Go's linker can
still embed environment-dependent state, and only a real comparison settles it.

The honest current claim is therefore: **release archives are deterministic,
release binaries are not yet proven so.**
