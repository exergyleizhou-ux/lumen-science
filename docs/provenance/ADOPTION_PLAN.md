# Open Science absorption plan

What to take from `aipoch/open-science` @ `d8f11e34`, what to rebuild instead of
taking, and what not to take at all.

Counts here are derived — module sizes from `git cat-file -s` at the pin,
reachability from `scripts/desktop-source-graph.mts`. Per-file provenance lands
in [`open-science-adoption.json`](open-science-adoption.json) automatically as
each item is absorbed.

## Where the gap actually is

We took 93% of their files, but almost all of it was UI. The engine is what we
stubbed, and the engine is where the value is.

| Module | Theirs | Ours reachable | Verdict |
|---|---:|---:|---|
| connectors | 53 | 48 | taken |
| settings | 27 | 27 | taken |
| skills | 11 | 10 | taken |
| update | 8 | 8 | taken, then rewritten (their feed was third-party) |
| **notebook** | **38** | **0** | **adopt — biggest gap, highest value** |
| **compute** | **16** | **0** | rebuild, do not copy |
| **reviewer** | **12** | **3** | adopt as adapter |
| **session-persistence** | 4 | 0 | adopt |
| **office-preview** | 4 | 0 | adopt |
| **uploads / project-files** | 5 | 0 | adopt |
| acp | 18 | 3 | **do not adopt** |
| agent-framework | 6 | 4 (stub) | **do not adopt** |

## The rule that decides each case

Upstream's engine modules are written as *their own execution authority*: they
decide what runs, spawn it, and report afterwards. Lumen has one authority, the
Rust `SessionActor`.

So the question for each module is not "is it good code" — most of it is — but
**does it decide, or does it execute?**

- **Executes** (mechanics: provisioning an environment, driving a kernel,
  rendering a document) → adopt. Mechanics are hard, well-tested here, and
  carry no authority.
- **Decides** (what to run, whether it is allowed, what the result means) →
  rebuild behind the actor. Copying these is how a second authority gets in,
  which is what LS5-D2 spent forty commits removing.

Adopting the mechanics does not mean rewriting them in Rust. 500 KB of
environment provisioning is sensible TypeScript and pointless Rust. The split
is: **decisions in Rust, mechanics in the adapter, outputs back through the
actor as artifacts.** An adapter may be driven; it may never self-direct.

## Priority

### P1 — makes reproducibility real

Our `check_kernel_admission` currently fabricates its answer: `exact_version:
"unknown"`, always `Admitted`. Reproducibility claims mean nothing on top of an
environment nobody identified.

| Take | From | For | Outcome |
|---|---|---|---|
| `environment-discovery.ts` (21 KB) | notebook | real interpreter discovery and identification | **taken** (LS5-K4) |
| `micromamba-cache.ts` (17 KB) | notebook | pinned, content-addressed toolchain | **taken** (LS5-K4) |
| `runtime-paths.ts` (19 KB) | notebook | path layout, env prefixes, ready markers | **taken** (LS5-R1-02, LS5-K4) |
| `bundle-manifest.ts` (12 KB) | notebook | content-addressed digests (`sha256File`) | **taken** (LS5-K4) |
| `provisioner.ts` (83 KB) | notebook | reproducible environment construction | **not taken** — see below |
| `package-manager.ts` (28 KB) | notebook | resolved, lockable dependency sets | **not taken** — see below |

Already done: `python_loop.py` → `lumen_python_loop.py`, hardened (LS5-K2).

#### What LS5-K4 actually landed

`src/main/environment/` is a driven adapter over the four adopted modules:
`identify` hashes the interpreter's bytes, runs it with a *fixed* version argv,
and records path/sha256/version/os/arch/lock digest; `requestAdmission` builds
`x.ai/science/kernel_admission` params and returns the SessionActor's verdict
verbatim. It has no `admitted` field of its own, and with no transport wired it
reports that it could not ask rather than answering.

Two decisions moved out of the adopted code:

* **Origin.** `runtime-paths.resolveRuntimeCdnBase` and
  `bundle-manifest.manifestUrl`/`packUrl` used to trust whatever base they were
  handed. Both now go through `shared/runtime-origin-policy.ts` (https +
  Lumen-owned host + the `update-policy` denylist), default disabled.
* **Pinning.** Discovery accepted a bare `python3` from the Settings catalog. A
  PATH-relative name is not an identity, so unpinned candidates are refused and
  reported with a reason instead of silently becoming an environment.

#### Why `provisioner.ts` was not taken

It is the decision module, not a mechanic. `planStartupAction` (l.1468) tells
the app-startup gate to `upgrade` / `repair` / build `fresh`, and the gate then
provisions on its own initiative — a second execution authority booting itself.
Its closure also drags the third-party download path (`language-pack-fetch.ts`
→ `net/resilient-download.ts`), which can only function against a runtime CDN
that is now disabled by default. Rebuild the *job model* behind the actor; do
not import the gate.

#### Why `package-manager.ts` was not taken

It is clean of IPC — the entanglement is not the problem. Its whole public
surface is `installPackages` plus `defaultSpawn`, which launches
micromamba/pip/R. It writes no lock file, so it contributes nothing to the
identity facts P1 exists to produce. Adopting it would place a
package-installing spawner in the reachable graph with no caller: capability
without purpose. Take it when something drives it.

### P2 — makes execution durable

| Take | From | For |
|---|---|---|
| `operation-journal.ts` (28 KB) | notebook | append-only operation record |
| `operation-recovery.ts` (12 KB) | notebook | interrupted-operation recovery |
| `session-persistence/**` (4 files) | main | durable session state |

Their journal and recovery are close to what the 5.0 plan specifies
independently. Study before writing ours — arriving at the same design twice is
evidence it is the right one.

### P3 — makes results usable

| Take | From | For |
|---|---|---|
| `kernel-executor.ts` (36 KB) | notebook | driver for the adopted loop |
| `ipynb-export.ts` (14 KB) | notebook | notebook interchange |
| `office-preview/**` | main | document rendering |
| `uploads/**`, `project-files/**` | main | file ingest |

### P4 — rebuild, do not copy

**`compute/**` (16 files).** Lumen already routes SSH/SCP through the
SessionActor with durable admission, approval timeout and terminal
acknowledgement (`session/science_connector.ts` and the Rust side). Theirs is a
parallel dispatcher with its own approval broker. Take the *job model* — poller,
harvest,状态 machine — and drive it from ours. Do not take the broker.

**`reviewer/**` (12 files).** The orchestrator and fix-loop decide what to
change. As an adapter it may propose; the actor records. Their independence
model (author ≠ reviewer) is worth copying as a *rule*, not as a runtime.

### A correction: the three unserved tools

`artifact_list`, `notebook_execute` and `start_review` are Go MCP tools, and the
desktop's method registry refuses them (AUTH-7). The obvious fix — give the
desktop an HTTP client for the Go bridge — is wrong, and worth writing down
before someone does it.

It would give the desktop two engines to talk to. That is the two-authority
shape this branch spent forty commits removing, only with the second authority
reached over a different transport. Nothing about it being Go rather than
TypeScript makes it safe.

Under the rule at the top of this document, the Go pack EXECUTES: it holds real
science capability and no authority. So it belongs behind the actor, invoked by
Rust as a typed adapter, exactly like `PythonLoopRunner` drives the exec-loop.
The desktop keeps one transport and one authority.

Until that routing exists those three call sites fail explicitly, which is the
honest state: the capability is unreachable, and saying so is better than
reaching it by a path that dissolves the invariant.

### Never

**`acp/**` (18 files).** Their ACP client drives Claude Code / Codex / OpenCode
as agent backends. We have our own ACP client for our own engine
(`acp-stdio-transport.ts`), and `agent-framework/index.ts` already states no
such backend is admitted as a peer authority. Adopting this would re-import the
exact model that was removed.

**`agent-framework/**`.** Same reason, deliberately stubbed.

**Their update feed.** Already removed (LS5-R1-02); `statics.aipoch.com` is
third-party infrastructure serving code to our users.

## Obligations, per item

Every adoption goes through the machinery already built, so this costs nothing
per file:

1. Land the file, adapted.
2. Add the Apache-2.0 §4(b) statement of changes at the top.
3. If it is outside `packs/science-desktop/src`, add it to `EXTRA_ADOPTIONS` in
   `scripts/generate-adoption-provenance.py`.
4. Regenerate the ledger. It refuses to write if it cannot fetch the upstream
   blob to substantiate the claim.
5. `verify-adoption-provenance.py` runs in CI.

## What absorbing all of this does and does not buy

It closes the capability gap with upstream: environments, kernels, notebooks,
previews, job dispatch.

It does not by itself make us better than upstream. Their engine already works;
copying it well reaches parity. The difference is what the actor wraps around
it — every execution carrying its environment identity, input digests, policy
hash and permission decision into an evidence chain a third party can re-verify.

Upstream does not do that, and adopting their code does not give it to us. That
part is ours to build, and is the only reason to prefer this product.
