/**
 * Science IPC registration (testable without full Electron app bootstrap).
 *
 * Single registration site for ACP proxy + OSF-2 files + UI project catalog
 * + OSF-3 notebook plan/execute (ACP only).
 * installIpcGuard does NOT register channels — only this module does via safeHandle.
 */

import type { PreviewFileStore } from './preview-resolver'
import { loadArtifactPreview } from './preview-service'
import {
  bindTrustedSession,
  unbindTrustedSession,
  seedPreviewStoreFromList,
  type MembershipAsserter,
  type ArtifactListItem,
  type SeedableStore,
} from './session-binding'
import type { LocalProjectCatalog } from './local-project-catalog'
import { SCIENCE_STORE_DIR } from './acp-membership'
import {
  getTrustedPreviewContextForSender,
  requireSenderTrustedContext,
  trySenderTrustedContext,
  senderIdFromEvent,
  type TrustedIdentitySender,
} from './session-identity'
import { createNotebookService, type NotebookService } from './notebook-service'
import type { NotebookCellRequest } from './notebook-plan'
import { createReviewService, type ReviewService } from './review-service'
import type { ReviewRequest } from './review-plan'
import { createSkillService, type SkillService } from './skill-service'
import { createComputeService, type ComputeService } from './compute-service'
import type { ComputePlanRequest } from './compute-plan'
import {
  loadConnectorCatalog,
  rejectDesktopConnectorFetch,
} from './connector-catalog'
import {
  assertOfficePreviewAdmission,
  listOfficeAdmissions,
  type OfficePreviewOpenRequest,
} from './office-preview-admission'
import {
  createEnvironmentService,
  type AdmissionAsk,
  type EnvironmentService,
} from '../environment/service'
import {
  isGenericRendererScienceMethod,
  resolveScienceMethod,
} from '../science-method-registry'
import type { KernelKindName } from '../environment/interpreter-identity'
import { createHash, randomUUID } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'

/** Minimal surface — works with Electron IpcMain or a test double. */
export type IpcMainLike = {
  handle(
    channel: string,
    handler: (event: unknown, ...args: unknown[]) => unknown,
  ): void
}

export type SafeHandleFn = (
  ipcMain: IpcMainLike,
  channel: string,
  handler: (_event: unknown, ...args: unknown[]) => Promise<unknown>,
) => void

export type ListArtifactsFn = (args: {
  ownerId: string
  projectId: string
  runId: string
}) => Promise<ArtifactListItem[]>

export type ScienceIpcDeps = {
  safeHandle: SafeHandleFn
  getLumenBinaryHash: () => string | null
  /**
   * The session workspace the ENGINE resolves relative store paths against
   * (Electron userData in production). Needed to turn a workflow report's
   * relative artifact entries into absolute preview paths. Absent, execution
   * still works — the run's artifacts are simply not seeded for preview.
   */
  workspaceRoot?: string
  /**
   * Invoke one science method on the engine.
   *
   * Was `acpFetch(path, init)` — an HTTP shape kept from when this file POSTed
   * to a loopback port that no engine ever served. The transport is ACP over
   * stdio; modelling it as a fetch forced every call site to build a fake
   * Request and forced the bridge to expose a fake router to receive it.
   */
  callScienceTool?: (toolName: string, args: Record<string, unknown>) => Promise<unknown>
  /** Names the engine can serve, for `acp:list-tools`. */
  listScienceTools?: () => Promise<unknown>
  previewStore: PreviewFileStore
  assertMembership?: MembershipAsserter
  listArtifacts?: ListArtifactsFn
  /** UI-only project catalog (not science authority). */
  projectCatalog?: LocalProjectCatalog
  /** Default owner for UI projects when renderer omits (dev: local-user). */
  defaultOwnerId?: string
  /** Optional inject notebook service (tests). Default: ACP-backed. */
  notebookService?: NotebookService
  /** Optional inject review service (tests). Default: ACP-backed. */
  reviewService?: ReviewService
  /** Optional inject skill service (tests). */
  skillService?: SkillService
  /** Override path to Lumen skills registry.json */
  skillsRegistryPath?: string
  /** Read-only ecosystem skill catalog; entries remain quarantined. */
  skillsEcosystemCatalogPath?: string
  /** Complete set of read-only ecosystem catalogs; all must validate. */
  skillsEcosystemCatalogPaths?: string[]
  /** Machine-backed, exact-source Biomni capability admission dossier. */
  skillsAdmissionPath?: string
  /** Main-owned packaged UniProt fixture; never supplied by the renderer. */
  biomniUniprotFixtureBase64?: string
  /** Optional inject compute service (tests). */
  computeService?: ComputeService
  /** Path to docs/science/fusion-sources.lock.json */
  connectorLockPath?: string
  /**
   * `<storageRoot>/runtime` — LS5-K4 environment identity.
   *
   * Injected because resolving it needs `electron`, and this module is executed
   * by the authority scripts without one. Absent means the environment handlers
   * report that they have no runtime root, which is the honest answer; they
   * never guess a path and describe an installation that is not there.
   */
  runtimeRoot?: string
  /** Optional inject environment service (tests). */
  environmentService?: EnvironmentService
}

// No default transport. There is no fallback engine to reach, and the previous
// default silently pointed every unwired caller at a loopback port nothing
// serves — which is how this pack shipped for so long looking connected.
/**
 * How long the engine will hold a mutation open for approval.
 *
 * The permission prompt must not outlive this, or the desk accepts a click for
 * an operation the engine has already given up on.
 */
export const ENGINE_APPROVAL_TIMEOUT_MS = 110_000
// ACP uses a 64 MiB newline-delimited JSON frame. Canonical base64 for a
// 32 MiB archive is ~42.7 MiB, leaving bounded room for the request envelope.
const MAX_SKILL_QUARANTINE_ARCHIVE_BYTES = 32 * 1024 * 1024

function senderHandleFromEvent(event: unknown): TrustedIdentitySender | undefined {
  const sender = (event as { sender?: TrustedIdentitySender } | null)?.sender
  if (!sender || typeof sender.id !== 'number' || typeof sender.on !== 'function') {
    return undefined
  }
  return sender
}

function decodeSkillQuarantineArchive(dataBase64: unknown): Buffer {
  if (typeof dataBase64 !== 'string' || dataBase64.length === 0) {
    throw new Error('skill archive must be non-empty base64')
  }
  const maxEncoded = Math.ceil(MAX_SKILL_QUARANTINE_ARCHIVE_BYTES / 3) * 4 + 4
  if (
    dataBase64.length > maxEncoded ||
    dataBase64.length % 4 !== 0 ||
    !/^[A-Za-z0-9+/]*={0,2}$/.test(dataBase64)
  ) {
    throw new Error('skill archive base64 is malformed or exceeds the quarantine cap')
  }
  const bytes = Buffer.from(dataBase64, 'base64')
  if (
    bytes.length === 0 ||
    bytes.length > MAX_SKILL_QUARANTINE_ARCHIVE_BYTES ||
    bytes.toString('base64') !== dataBase64
  ) {
    throw new Error('skill archive base64 is non-canonical or exceeds the quarantine cap')
  }
  return bytes
}

const noTransport = async (): Promise<never> => {
  throw new Error('no science engine transport wired: callScienceTool was not provided')
}

export function registerScienceIpcHandlers(ipcMain: IpcMainLike, deps: ScienceIpcDeps): void {
  const callTool = deps.callScienceTool ?? noTransport
  const listTools = deps.listScienceTools ?? noTransport
  const { safeHandle, getLumenBinaryHash, previewStore } = deps
  const defaultOwner = deps.defaultOwnerId ?? 'local-user'

  const notebook =
    deps.notebookService ??
    createNotebookService({
      acpCall: async (toolName, args) => {
        const raw = await callTool(toolName, args)
        return raw
      },
      defaultOwnerId: defaultOwner,
      storeRoot: SCIENCE_STORE_DIR,
      approvalTimeoutMs: ENGINE_APPROVAL_TIMEOUT_MS,
      // Resolves lazily at execute time (`environment` is declared further
      // down this function; by the time a cell runs, it is initialised). The
      // first runnable Python is taken in DISCOVERY order, which is
      // deterministic: manual settings entries, then PATH, then well-known
      // install dirs — so a user's explicit choice wins over a system default.
      resolveInterpreter: async () => {
        if (!environment) {
          return {
            ok: false as const,
            reason:
              'no runtime root configured for this process — cannot name the interpreter ' +
              'this cell would run on, so nothing was executed',
          }
        }
        const report = await environment.discover('python')
        const usable = report.interpreters.find((i) => i.runnable)
        if (!usable) {
          const pinned = report.interpreters.filter((i) => i.interpreterPath.startsWith('/'))
          const unpinned = report.interpreters.length - pinned.length
          const probed = pinned.filter((i) => i.version !== undefined || i.detail !== undefined)
          return {
            ok: false as const,
            reason:
              `no runnable Python interpreter was discovered on this machine ` +
              `(total=${report.interpreters.length} pinned=${pinned.length} ` +
              `unpinned=${unpinned} versionProbed=${probed.length}). ` +
              'Add a Python 3 interpreter in Settings, install python3, or check PATH.',
          }
        }
        return { ok: true as const, interpreterPath: usable.interpreterPath }
      },
    })

  safeHandle(ipcMain, 'acp:call', async (_event, toolName: unknown, args: unknown) => {
    // The renderer supplies this name. Under the old `acpFetch` signature it was
    // JSON.stringify'd into a request body, so a non-string reached the wire as
    // whatever it serialised to. The typed transport surfaced that; validate it
    // here rather than casting the check away. The method registry still has the
    // final say on WHICH names are permitted — this only establishes it is a name.
    if (typeof toolName !== 'string' || toolName.length === 0) {
      return { _lumenError: true, message: 'acp:call requires a non-empty tool name' }
    }
    try {
      const method = resolveScienceMethod(toolName)
      if (!isGenericRendererScienceMethod(method.name)) {
        return {
          _lumenError: true,
          message:
            `${method.name} requires a sender-bound Desktop IPC route; generic acp:call cannot carry trusted identity`,
        }
      }
      return await callTool(toolName, (args as Record<string, unknown>) ?? {})
    } catch (e: unknown) {
      return { _lumenError: true, message: (e as Error).message || String(e) }
    }
  })

  safeHandle(ipcMain, 'acp:list-tools', async () => {
    try {
      return await listTools()
    } catch {
      return { tools: [], _lumenUnavailable: true }
    }
  })

  safeHandle(ipcMain, 'app:get-lumen-hash', async () => getLumenBinaryHash())

  safeHandle(ipcMain, 'files:preview-by-artifact', async (event, payload: unknown) => {
    const req = (payload ?? {}) as {
      artifactId?: string
      expectedSha256?: string
      mimeType?: string
      // Renderer may try to spoof ownership — ignore entirely.
      ownerId?: unknown
      projectId?: unknown
    }
    const identity = trySenderTrustedContext(event)
    if (!identity.ok) {
      return {
        access: {
          ok: false,
          reason: identity.reason,
        },
      }
    }
    return loadArtifactPreview(
      {
        artifactId: req.artifactId ?? '',
        expectedSha256: req.expectedSha256,
        mimeType: req.mimeType,
      },
      { store: previewStore },
      identity.trusted,
    )
  })

  safeHandle(ipcMain, 'files:bind-session', async (event, payload: unknown) => {
    const p = (payload ?? {}) as {
      ownerId?: string
      projectId?: string
      runId?: string
    }
    const assertMembership = deps.assertMembership
    if (!assertMembership) {
      return { ok: false, reason: 'no membership asserter configured — fail closed' }
    }
    const senderId = senderIdFromEvent(event)
    if (senderId === null) {
      return { ok: false, reason: 'bind requires a real IPC sender identity' }
    }
    const bound = await bindTrustedSession(
      { ownerId: p.ownerId ?? '', projectId: p.projectId ?? '' },
      {
        assertMembership,
        senderId,
        sender: senderHandleFromEvent(event),
      },
    )
    if (!bound.ok) return bound

    let seeded = 0
    if (deps.listArtifacts && p.runId && 'put' in previewStore) {
      try {
        const items = await deps.listArtifacts({
          ownerId: bound.ownerId,
          projectId: bound.projectId,
          runId: p.runId,
        })
        seeded = seedPreviewStoreFromList(
          previewStore as unknown as SeedableStore,
          items,
          { ownerId: bound.ownerId, projectId: bound.projectId },
        )
      } catch (e: unknown) {
        return {
          ok: true,
          ownerId: bound.ownerId,
          projectId: bound.projectId,
          seeded: 0,
          seedError: (e as Error).message || String(e),
        }
      }
    }

    return {
      ok: true,
      ownerId: bound.ownerId,
      projectId: bound.projectId,
      seeded,
    }
  })

  safeHandle(ipcMain, 'files:unbind-session', async (event) => {
    const senderId = senderIdFromEvent(event)
    // Only clear this sender — never wipe every window's binding.
    if (senderId === null) {
      return { ok: false, reason: 'unbind requires a real IPC sender identity' }
    }
    unbindTrustedSession(senderId)
    return { ok: true, cleared: true }
  })

  // ── UI project catalog (not Rust ProjectStore authority) ─────
  safeHandle(ipcMain, 'files:list-ui-projects', async () => {
    if (!deps.projectCatalog) return { projects: [], authority: 'ui-local' }
    return { projects: deps.projectCatalog.list(), authority: 'ui-local' }
  })

  safeHandle(ipcMain, 'files:create-ui-project', async (_event, payload: unknown) => {
    if (!deps.projectCatalog) {
      return { ok: false, reason: 'project catalog not configured' }
    }
    const p = (payload ?? {}) as {
      name?: string
      description?: string
      ownerId?: string
    }
    const ownerId = p.ownerId || defaultOwner
    const title = (p.name ?? '').trim()
    if (!title) {
      return { ok: false, reason: 'a project needs a name' }
    }

    // Create in the ENGINE first. Projects used to be written only to the local
    // catalog, so the engine had never heard of them and correctly denied
    // membership on open — the workspace was unreachable for every project the
    // UI could make.
    //
    // This is an actor-gated mutation: it asks permission, carries an operation
    // id for idempotency, and binds to this session and owner. The catalog is
    // updated only AFTER the engine accepts, so a refusal cannot leave a row
    // pointing at a project that does not exist.
    let engineProjectId: string
    try {
      const raw = (await callTool('project_create', {
        ownerId,
        storeRoot: SCIENCE_STORE_DIR,
        title,
        researchQuestion: p.description?.trim() || title,
        // Idempotency key. A retried IPC must not create a second project, and
        // the engine dedupes on this rather than on the title.
        operationId: randomUUID(),
        // Explicit, and shorter than the broker's patience. The engine defaults
        // to 120s while the prompt waits 300s, so a user who took three minutes
        // would click Allow, see it accepted, and the engine would already have
        // abandoned the run. Sending the window makes the two agree rather than
        // coincide.
        approvalTimeoutMs: ENGINE_APPROVAL_TIMEOUT_MS,
      })) as { projectId?: string; project_id?: string }
      const id = raw?.projectId ?? raw?.project_id
      if (typeof id !== 'string' || id.length === 0) {
        return { ok: false, reason: 'engine accepted the project but returned no id' }
      }
      engineProjectId = id
    } catch (e: unknown) {
      // Includes a denied permission. No catalog row is written, so the list
      // never shows a project the engine will refuse to open.
      return { ok: false, reason: (e as Error).message || String(e) }
    }

    try {
      const project = deps.projectCatalog.create({
        id: engineProjectId,
        name: title,
        description: p.description,
        ownerId,
      })
      return { ok: true, project, authority: 'session-actor' }
    } catch (e: unknown) {
      return { ok: false, reason: (e as Error).message || String(e) }
    }
  })

  safeHandle(ipcMain, 'files:update-question', async (event, payload: unknown) => {
    const p = (payload ?? {}) as {
      researchQuestion?: string
      ownerId?: unknown
      projectId?: unknown
    }
    const question = (p.researchQuestion ?? '').trim()
    if (!question) {
      return { ok: false, reason: 'a research question cannot be empty' }
    }
    const identity = trySenderTrustedContext(event)
    if (!identity.ok) {
      return { ok: false, reason: identity.reason }
    }
    const bound = identity.trusted
    // The question is part of the durable record, so refining it is a record
    // MUTATION: SessionActor route, permission prompt, idempotent operation
    // id. The alternative — keeping edits in renderer state — is how the
    // question a user spent an hour on vanished on tab switch while the
    // engine's record silently said something else.
    try {
      const raw = (await callTool('project_update_question', {
        ownerId: bound.ownerId,
        storeRoot: SCIENCE_STORE_DIR,
        projectId: bound.projectId,
        researchQuestion: question,
        operationId: randomUUID(),
        approvalTimeoutMs: ENGINE_APPROVAL_TIMEOUT_MS,
      })) as { revision?: string }
      return { ok: true, revision: raw?.revision, authority: 'session-actor' }
    } catch (e: unknown) {
      return { ok: false, reason: (e as Error).message || String(e) }
    }
  })

  /**
   * Open workspace: catalog lookup → membership bind → artifact seed.
   * Single product action for renderer (Question/Plan shell entry).
   */
  safeHandle(ipcMain, 'files:open-ui-project', async (event, payload: unknown) => {
    if (!deps.projectCatalog) {
      return { ok: false, reason: 'project catalog not configured' }
    }
    const p = (payload ?? {}) as { projectId?: string; ownerId?: string; runId?: string }
    const project = deps.projectCatalog.get(p.projectId ?? '')
    if (!project) {
      return { ok: false, reason: 'ui project not found' }
    }
    const ownerId = p.ownerId || project.ownerId
    const assertMembership = deps.assertMembership
    if (!assertMembership) {
      return { ok: false, reason: 'no membership asserter configured — fail closed' }
    }
    const senderId = senderIdFromEvent(event)
    if (senderId === null) {
      return { ok: false, reason: 'open-ui-project requires a real IPC sender identity' }
    }
    const bound = await bindTrustedSession(
      { ownerId, projectId: project.id },
      {
        assertMembership,
        senderId,
        sender: senderHandleFromEvent(event),
      },
    )
    if (!bound.ok) return bound

    const runId = p.runId || project.defaultRunId
    let seeded = 0
    let seedError: string | undefined
    if (deps.listArtifacts && 'put' in previewStore) {
      try {
        const items = await deps.listArtifacts({
          ownerId: bound.ownerId,
          projectId: bound.projectId,
          runId,
        })
        seeded = seedPreviewStoreFromList(
          previewStore as unknown as SeedableStore,
          items,
          { ownerId: bound.ownerId, projectId: bound.projectId },
        )
      } catch (e: unknown) {
        seedError = (e as Error).message || String(e)
      }
    }

    // The recorded question, read from the engine's durable bundle — the tab
    // must show what IS recorded, not a blank local scratchpad. Absence is
    // non-fatal: an older record without one simply shows empty.
    let researchQuestion: string | undefined
    try {
      const bundle = (await callTool('project_get', {
        storeRoot: SCIENCE_STORE_DIR,
        projectId: bound.projectId,
        ownerId: bound.ownerId,
      })) as { project?: { research_question?: string } }
      researchQuestion = bundle?.project?.research_question
    } catch {
      researchQuestion = undefined
    }

    return {
      ok: true,
      project,
      ownerId: bound.ownerId,
      projectId: bound.projectId,
      runId,
      seeded,
      seedError,
      researchQuestion,
      authority: 'ui-local+lumen-bind',
    }
  })

  /**
   * Remove a project from THIS list. Not a delete.
   *
   * The engine has no project_delete route, and it should not gain one on the
   * desktop's initiative: destroying a research record is irreversible and
   * belongs to whoever owns the store, not to a window that indexes it. So
   * this drops the local catalog row and nothing else — the project, its runs
   * and its artifacts all remain in the engine.
   *
   * The `authority: 'ui-local'` is the honest label, and the UI says "Remove
   * from list" rather than "Delete" so the two are not confused.
   */
  safeHandle(ipcMain, 'files:delete-ui-project', async (event, payload: unknown) => {
    if (!deps.projectCatalog) {
      return { ok: false, reason: 'project catalog not configured' }
    }
    const p = (payload ?? {}) as { projectId?: string }
    const projectId = p.projectId ?? ''
    const ok = deps.projectCatalog.delete(projectId)
    // Remove-from-list only clears THIS sender if it is bound to the removed
    // project — never a process-wide identity wipe.
    const senderId = senderIdFromEvent(event)
    if (ok && senderId !== null) {
      const bound = getTrustedPreviewContextForSender(senderId)
      if (bound?.projectId === projectId) {
        unbindTrustedSession(senderId)
      }
    }
    return {
      ok,
      reason: ok ? undefined : 'no such project in this list',
      removedFromListOnly: true,
      authority: 'ui-local',
    }
  })

  // ── OSF-3 Notebook (plan/dry-run local; execute via ACP only) ──
  safeHandle(ipcMain, 'notebook:plan-cell', async (_event, payload: unknown) => {
    const req = normalizeCellRequest(payload)
    return notebook.plan(req)
  })

  safeHandle(ipcMain, 'notebook:dry-run-cell', async (_event, payload: unknown) => {
    const req = normalizeCellRequest(payload)
    return notebook.dryRun(req)
  })

  safeHandle(ipcMain, 'notebook:execute-cell', async (event, payload: unknown) => {
    const req = normalizeCellRequest(payload)
    const identity = trySenderTrustedContext(event)
    const trusted = identity.ok ? identity.trusted : null
    const out = await notebook.execute(req, trusted)

    // Register the run's committed artifacts for preview and review.
    //
    // Workflow outputs use the executor's commit store, not ScienceStore's
    // run-artifact registry served by `artifact_list`. Seed them directly from
    // the engine's commit report: those are the authoritative hashes and paths
    // for this just-completed workflow.
    //
    // Artifact ids are the content hashes themselves. Content addressing means
    // a re-run that produces identical output re-seeds the same id — no
    // duplicates, and the id a user quotes in a review IS the hash the review
    // verifies.
    const bound = trusted
    const result = (out as { ok?: boolean; result?: Record<string, unknown> })?.result
    let artifactsSeeded = 0
    if ((out as { ok?: boolean })?.ok && bound && deps.workspaceRoot && result && previewStore.put) {
      const runId = typeof result.runId === 'string' ? result.runId : ''
      const commits = Array.isArray(result.commits)
        ? (result.commits as {
            stepId?: string
            committedByAttempt?: string
            outputManifest?: Record<string, string>
          }[])
        : []
      for (const commit of commits) {
        if (!runId || !commit.stepId || !commit.committedByAttempt) continue
        for (const [rel, sha256] of Object.entries(commit.outputManifest ?? {})) {
          if (!/^[0-9a-f]{64}$/.test(sha256)) continue
          previewStore.put!(sha256, {
            // Mirrors the executor's per-attempt layout:
            // <workspace>/<store>/workflow-outputs/<run>/<step>/<attempt>/<rel>
            path: path.join(
              deps.workspaceRoot,
              SCIENCE_STORE_DIR,
              'workflow-outputs',
              runId,
              commit.stepId,
              commit.committedByAttempt,
              rel,
            ),
            sha256,
            ownerId: bound.ownerId,
            projectId: bound.projectId,
          })
          artifactsSeeded += 1
        }
      }
    }
    return { ...(out as Record<string, unknown>), artifactsSeeded }
  })

  safeHandle(ipcMain, 'notebook:history', async () => ({
    cells: notebook.history(),
    authority: 'ui-history-only',
  }))

  safeHandle(ipcMain, 'notebook:export-ipynb', async (event) => {
    const identity = trySenderTrustedContext(event)
    return notebook.exportIpynb(identity.ok ? identity.trusted : null)
  })

  // ── OSF-4 Reviewer (plan/submit; no fix-loop authority) ──────
  const review =
    deps.reviewService ??
    createReviewService({
      acpCall: async (toolName, args) => {
        const raw = await callTool(toolName, args)
        return raw
      },
      previewStore,
    })

  safeHandle(ipcMain, 'review:plan', async (_event, payload: unknown) => {
    const req = (payload ?? {}) as ReviewRequest
    return review.plan(req)
  })

  safeHandle(ipcMain, 'review:submit', async (event, payload: unknown) => {
    const req = (payload ?? {}) as ReviewRequest
    const identity = trySenderTrustedContext(event)
    return review.submit(req, identity.ok ? identity.trusted : null)
  })

  safeHandle(ipcMain, 'review:history', async () => ({
    verdicts: review.history(),
    authority: 'in-memory-projection-only',
  }))

  safeHandle(ipcMain, 'review:latest', async () => ({
    verdict: review.latest(),
    authority: 'in-memory-projection-only',
  }))

  safeHandle(ipcMain, 'review:export-dossier', async (event) => {
    const identity = trySenderTrustedContext(event)
    return review.exportDossier(identity.ok ? identity.trusted : null)
  })

  // ── OSF-5 Skills (quarantine import; no bulk auto-approve) ───
  const skills =
    deps.skillService ??
    createSkillService({
      registryPath:
        deps.skillsRegistryPath ||
        path.resolve(process.cwd(), '../../packs/science/skills/registry.json'),
      ecosystemCatalogPath:
        deps.skillsEcosystemCatalogPath,
      ecosystemCatalogPaths:
        deps.skillsEcosystemCatalogPaths ?? [
          path.resolve(
            process.cwd(),
            '../../packs/science/skills/ecosystem/scp-catalog.json',
          ),
          path.resolve(
            process.cwd(),
            '../../packs/science/skills/ecosystem/biomni-tool-catalog.json',
          ),
          path.resolve(
            process.cwd(),
            '../../packs/science/skills/ecosystem/biomni-resource-catalog.json',
          ),
        ],
      admissionPath: deps.skillsAdmissionPath,
    })

  const quarantineSkillBundle = async (
    event: unknown,
    payload: unknown,
    items: { subPath: string; replaceId?: string }[],
  ): Promise<{ operationId: string; runId: string }> => {
    // Sender-scoped only — never process-global identity.
    const trusted = requireSenderTrustedContext(event)
    if (
      items.length === 0 ||
      items.some((item) => typeof item.subPath !== 'string')
    ) {
      throw new Error('skill quarantine requires at least one explicit previewed subPath')
    }
    if (items.some((item) => item.replaceId !== undefined)) {
      throw new Error('quarantine cannot replace or auto-enable an installed skill')
    }
    // Renderer payload may not supply owner/project/session/workspace/path.
    const request = (payload ?? {}) as {
      dataBase64?: unknown
      ownerId?: unknown
      projectId?: unknown
      sessionId?: unknown
      workspaceRoot?: unknown
      storeRoot?: unknown
      path?: unknown
    }
    if (
      request.ownerId !== undefined ||
      request.projectId !== undefined ||
      request.sessionId !== undefined ||
      request.workspaceRoot !== undefined ||
      request.storeRoot !== undefined ||
      request.path !== undefined
    ) {
      throw new Error(
        'renderer may not supply ownerId/projectId/sessionId/workspace/storeRoot/path for skill quarantine',
      )
    }
    const bytes = decodeSkillQuarantineArchive(request.dataBase64)
    const archiveSha256 = createHash('sha256').update(bytes).digest('hex')
    const selectedSubPaths = items.map((item) => item.subPath).sort()
    const operationId = `skillq-${createHash('sha256')
      .update(
        JSON.stringify({
          ownerId: trusted.ownerId,
          projectId: trusted.projectId,
          archiveSha256,
          selectedSubPaths,
        }),
      )
      .digest('hex')
      .slice(0, 40)}`
    const result = (await callTool('skill_quarantine_import', {
      ownerId: trusted.ownerId,
      projectId: trusted.projectId,
      storeRoot: SCIENCE_STORE_DIR,
      operationId,
      archiveBase64: bytes.toString('base64'),
      archiveSha256,
      archiveBytes: bytes.length,
      items: selectedSubPaths.map((subPath) => ({ subPath })),
      approvalTimeoutMs: ENGINE_APPROVAL_TIMEOUT_MS,
    })) as {
      operationId?: string
      run?: { context?: { run_id?: string } }
    }
    const runId = result.run?.context?.run_id
    if (result.operationId !== operationId || !runId) {
      throw new Error('Rust skill quarantine returned an invalid durable result')
    }
    return { operationId, runId }
  }

  safeHandle(ipcMain, 'settings:import-skill-zip', async (event, payload: unknown) => {
    const request = (payload ?? {}) as { subPath?: string; replaceId?: string }
    if (typeof request.subPath !== 'string') {
      throw new Error('preview and select the skill root before quarantine import')
    }
    const result = await quarantineSkillBundle(event, payload, [
      { subPath: request.subPath, replaceId: request.replaceId },
    ])
    return {
      status: 'quarantined',
      id: result.runId,
      operationId: result.operationId,
      skills: [],
    }
  })

  safeHandle(ipcMain, 'settings:import-skill-zip-batch', async (event, payload: unknown) => {
    const request = (payload ?? {}) as {
      items?: { subPath: string; replaceId?: string }[]
    }
    const items = request.items ?? []
    const result = await quarantineSkillBundle(event, payload, items)
    return {
      results: items.map((item) => ({
        subPath: item.subPath,
        status: 'quarantined',
        id: result.runId,
      })),
      operationId: result.operationId,
      skills: [],
    }
  })

  safeHandle(ipcMain, 'skills:list', async () => skills.listInventory())

  safeHandle(ipcMain, 'skills:quarantine-list', async () => ({
    skills: skills.quarantineList(),
  }))

  safeHandle(ipcMain, 'skills:bulk-admit', async (_event, payload: unknown) => {
    const p = (payload ?? {}) as { skillIds?: string[] }
    return skills.bulkAdmit(p.skillIds ?? [])
  })

  /**
   * Admitted ecosystem capability entry (currently Biomni query_uniprot only).
   * Identity from sender binding only. connector_id is fixed server-side by
   * capability_run → uniprot; renderer cannot choose a connector.
   */
  safeHandle(ipcMain, 'skills:run-capability', async (event, payload: unknown) => {
    const identity = trySenderTrustedContext(event)
    if (!identity.ok) {
      return { ok: false, reason: identity.reason }
    }
    const request = (payload ?? {}) as {
      capabilityId?: unknown
      prompt?: unknown
      maxResults?: unknown
      fixturePaths?: unknown
      ownerId?: unknown
      projectId?: unknown
      sessionId?: unknown
      connectorId?: unknown
      endpoint?: unknown
      url?: unknown
    }
    if (
      request.ownerId !== undefined ||
      request.projectId !== undefined ||
      request.sessionId !== undefined ||
      request.connectorId !== undefined ||
      request.endpoint !== undefined ||
      request.url !== undefined ||
      request.fixturePaths !== undefined
    ) {
      return {
        ok: false,
        reason:
          'renderer may not supply ownerId/projectId/sessionId/connectorId/endpoint/url/fixturePaths for capability run',
      }
    }
    if (request.capabilityId !== 'ecosystem/biomni/query_uniprot') {
      return {
        ok: false,
        reason:
          'capability is not admitted for execution (Biomni catalog: 1 of 224 executable; rest quarantined)',
      }
    }
    const prompt = typeof request.prompt === 'string' ? request.prompt : ''
    const maxResults = request.maxResults === undefined ? 5 : request.maxResults
    if (
      typeof maxResults !== 'number' ||
      !Number.isInteger(maxResults) ||
      maxResults < 1 ||
      maxResults > 50
    ) {
      return {
        ok: false,
        reason: 'maxResults must be an integer in 1..=50',
      }
    }
    if (!deps.biomniUniprotFixtureBase64) {
      return { ok: false, reason: 'packaged UniProt offline fixture is unavailable' }
    }
    // Engine session id is the ACP session, not a renderer-forged identity.
    // Desktop callScienceTool must bind session; owner/project come from sender.
    try {
      const raw = await callTool('capability_run', {
        // sessionId filled by the ACP bridge from the live engine session when
        // the bridge injects it; if the tool surface requires it in-args, the
        // bridge is responsible. We never take it from the renderer payload.
        projectId: identity.trusted.projectId,
        ownerId: identity.trusted.ownerId,
        storeRoot: SCIENCE_STORE_DIR,
        artifactRoot: path.join(SCIENCE_STORE_DIR, 'runs'),
        capabilityId: 'ecosystem/biomni/query_uniprot',
        input: { prompt, maxResults },
        fixtureDataBase64: [deps.biomniUniprotFixtureBase64],
        approvalTimeoutMs: ENGINE_APPROVAL_TIMEOUT_MS,
      })
      return {
        ok: true,
        result: raw,
        authority: 'SessionActor/capability_run',
        source: 'Biomni',
        executor: 'Rust Lumen',
        dataSource: 'UniProt',
        mode: 'fixture/offline',
      }
    } catch (e: unknown) {
      return { ok: false, reason: (e as Error).message || String(e) }
    }
  })

  // ── OSF-6 Remote Compute (dry-run plan only) ──────────────────
  const compute =
    deps.computeService ??
    createComputeService({
      acpCall: async (toolName, args) => {
        const raw = await callTool(toolName, args)
        return raw
      },
    })

  safeHandle(ipcMain, 'compute:plan', async (event, payload: unknown) => {
    const identity = trySenderTrustedContext(event)
    return compute.plan(
      (payload ?? {}) as ComputePlanRequest,
      identity.ok ? identity.trusted : null,
    )
  })

  safeHandle(ipcMain, 'compute:submit-plan', async (event, payload: unknown) => {
    const identity = trySenderTrustedContext(event)
    return compute.submitPlan(
      (payload ?? {}) as ComputePlanRequest,
      identity.ok ? identity.trusted : null,
    )
  })

  safeHandle(ipcMain, 'compute:execute-live', async (_event, payload: unknown) => {
    const p = (payload ?? {}) as { planId?: string }
    return compute.executeLive(p.planId ?? '')
  })

  safeHandle(ipcMain, 'compute:history', async () => ({
    plans: compute.history(),
    authority: 'dry-run-only',
  }))

  // ── OSF-7 Connector catalog (read-only) ───────────────────────
  const lockPath =
    deps.connectorLockPath ||
    path.resolve(process.cwd(), '../../docs/science/fusion-sources.lock.json')

  safeHandle(ipcMain, 'connectors:list', async () => {
    try {
      const cat = loadConnectorCatalog(lockPath)
      return {
        ok: true,
        ...cat,
        authority: 'catalog-only',
        note: 'Fetch only via SessionActor Rust adapters — not desktop',
      }
    } catch (e: unknown) {
      return {
        ok: false,
        reason: (e as Error).message || String(e),
      }
    }
  })

  safeHandle(ipcMain, 'connectors:fetch', async (_event, payload: unknown) => {
    const p = (payload ?? {}) as { connectorId?: string }
    return rejectDesktopConnectorFetch(p.connectorId || 'unknown')
  })

  // ── Office preview admission (fail-closed) ───────────────────
  safeHandle(ipcMain, 'office:admission-list', async () => ({
    admissions: listOfficeAdmissions(),
    note: 'hostile-doc suite required before admitted=true',
  }))

  safeHandle(ipcMain, 'office:preview-open', async (_event, payload: unknown) => {
    const req = (payload ?? {}) as OfficePreviewOpenRequest
    const gate = assertOfficePreviewAdmission(req)
    if (!gate.ok) {
      return { ok: false, reason: gate.reason }
    }
    return {
      ok: true,
      admission: gate.admission,
      note: 'admission passed — isolated renderer open may proceed',
    }
  })

  // ── LS5-K4 Environment identity (facts only; admission is the engine's) ──
  //
  // These three channels are the driven surface of the environment adapter.
  // They return observations and, for admission, the engine's own answer. None
  // of them can produce an "admitted" of its own: `environment:request-admission`
  // either forwards to the SessionActor or reports that it could not ask.
  const environment: EnvironmentService | null =
    deps.environmentService ??
    (deps.runtimeRoot
      ? createEnvironmentService({
          runtimeRoot: deps.runtimeRoot,
          acpCall: async (method, args) => callTool(method, args),
        })
      : null)

  const noRuntimeRoot = {
    ok: false as const,
    reason:
      'no runtime root configured for this process — environment identity is unavailable, ' +
      'not empty',
  }

  safeHandle(ipcMain, 'environment:discover', async (_event, payload: unknown) => {
    if (!environment) return noRuntimeRoot
    const p = (payload ?? {}) as { language?: string }
    const report = await environment.discover(p.language === 'r' ? 'r' : 'python')
    return { ok: true, ...report, authority: 'observation-only' }
  })

  safeHandle(ipcMain, 'environment:identify', async (_event, payload: unknown) => {
    if (!environment) return noRuntimeRoot
    const p = (payload ?? {}) as { kind?: string; interpreterPath?: string; packageLockPath?: string }
    const result = await environment.identify({
      kind: normalizeKernelKind(p.kind),
      interpreterPath: typeof p.interpreterPath === 'string' ? p.interpreterPath : '',
      packageLockPath: typeof p.packageLockPath === 'string' ? p.packageLockPath : undefined,
    })
    return { ok: true, ...result, authority: 'SessionActor-required' }
  })

  safeHandle(ipcMain, 'environment:request-admission', async (_event, payload: unknown) => {
    if (!environment) return noRuntimeRoot
    const p = (payload ?? {}) as Partial<AdmissionAsk> & { kind?: string }
    const outcome = await environment.requestAdmission({
      sessionId: typeof p.sessionId === 'string' ? p.sessionId : '',
      ownerId: typeof p.ownerId === 'string' ? p.ownerId : '',
      projectId: typeof p.projectId === 'string' ? p.projectId : '',
      storeRoot: typeof p.storeRoot === 'string' ? p.storeRoot : '',
      kernelId: typeof p.kernelId === 'string' ? p.kernelId : '',
      kind: normalizeKernelKind(p.kind),
      interpreterPath: typeof p.interpreterPath === 'string' ? p.interpreterPath : '',
      packageLockPath: typeof p.packageLockPath === 'string' ? p.packageLockPath : undefined,
      allowedRoot: typeof p.allowedRoot === 'string' ? p.allowedRoot : undefined,
      probeTimeoutMs: typeof p.probeTimeoutMs === 'number' ? p.probeTimeoutMs : undefined,
      approvalTimeoutMs: typeof p.approvalTimeoutMs === 'number' ? p.approvalTimeoutMs : undefined,
    })
    return { ok: true, ...outcome, authority: 'SessionActor/kernel_admission' }
  })

  // ── Release honesty (no fake binary upload claims) ───────────
  safeHandle(ipcMain, 'release:checklist-status', async () => {
    const checklistPath = path.resolve(
      process.cwd(),
      '../../docs/science/RELEASE_1.0.1_CHECKLIST.md',
    )
    const exists = fs.existsSync(checklistPath)
    return {
      ok: true,
      checklistPath: exists ? checklistPath : null,
      checklistPresent: exists,
      // Honest defaults — only release-ops can flip these after upload
      binariesUploaded: false,
      notarizationComplete: false,
      productVersion: process.env.npm_package_version || '1.1.0-dev',
      note: 'P0: upload assets listed in SHA256SUMS to GitHub Release before claiming installable',
    }
  })
}

// The renderer supplies this string. Unknown values become 'python' rather than
// being passed through, because the engine's parameter parser also defaults
// unknown kinds to Python — narrowing here keeps the two ends agreeing instead
// of letting a typo silently change which argv probes the binary.
function normalizeKernelKind(kind: unknown): KernelKindName {
  return kind === 'r' || kind === 'julia' ? kind : 'python'
}

function normalizeCellRequest(payload: unknown): NotebookCellRequest {
  const p = (payload ?? {}) as Partial<NotebookCellRequest>
  return {
    language: p.language === 'r' ? 'r' : 'python',
    code: typeof p.code === 'string' ? p.code : '',
    cellId: p.cellId,
    dryRun: p.dryRun,
  }
}
