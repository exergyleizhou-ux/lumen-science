/**
 * Science IPC registration (testable without full Electron app bootstrap).
 *
 * Single registration site for ACP proxy + OSF-2 files + UI project catalog.
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
  projectId: string
  runId: string
}) => Promise<ArtifactListItem[]>

export type ScienceIpcDeps = {
  safeHandle: SafeHandleFn
  getLumenBinaryHash: () => string | null
  acpFetch?: (path: string, init?: RequestInit) => Promise<unknown>
  previewStore: PreviewFileStore
  assertMembership?: MembershipAsserter
  listArtifacts?: ListArtifactsFn
  /** UI-only project catalog (not science authority). */
  projectCatalog?: LocalProjectCatalog
  /** Default owner for UI projects when renderer omits (dev: local-user). */
  defaultOwnerId?: string
}

const DEFAULT_ACP_BASE = 'http://127.0.0.1:17000'

async function defaultAcpFetch(path: string, init?: RequestInit): Promise<unknown> {
  const resp = await fetch(`${DEFAULT_ACP_BASE}${path}`, init)
  return resp.json()
}

export function registerScienceIpcHandlers(ipcMain: IpcMainLike, deps: ScienceIpcDeps): void {
  const acpFetch = deps.acpFetch ?? defaultAcpFetch
  const { safeHandle, getLumenBinaryHash, previewStore } = deps
  const defaultOwner = deps.defaultOwnerId ?? 'local-user'

  safeHandle(ipcMain, 'acp:call', async (_event, toolName: unknown, args: unknown) => {
    try {
      return await acpFetch('/tools/call', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          name: toolName,
          arguments: (args as Record<string, unknown>) ?? {},
        }),
      })
    } catch (e: unknown) {
      return { _lumenError: true, message: (e as Error).message || String(e) }
    }
  })

  safeHandle(ipcMain, 'acp:list-tools', async () => {
    try {
      return await acpFetch('/tools/list')
    } catch {
      return { tools: [], _lumenUnavailable: true }
    }
  })

  safeHandle(ipcMain, 'app:get-lumen-hash', async () => getLumenBinaryHash())

  safeHandle(ipcMain, 'files:preview-by-artifact', async (_event, payload: unknown) => {
    const req = (payload ?? {}) as {
      artifactId?: string
      expectedSha256?: string
      mimeType?: string
    }
    return loadArtifactPreview(
      {
        artifactId: req.artifactId ?? '',
        expectedSha256: req.expectedSha256,
        mimeType: req.mimeType,
      },
      { store: previewStore },
    )
  })

  safeHandle(ipcMain, 'files:bind-session', async (_event, payload: unknown) => {
    const p = (payload ?? {}) as {
      ownerId?: string
      projectId?: string
      runId?: string
    }
    const assertMembership = deps.assertMembership
    if (!assertMembership) {
      return { ok: false, reason: 'no membership asserter configured — fail closed' }
    }
    const bound = await bindTrustedSession(
      { ownerId: p.ownerId ?? '', projectId: p.projectId ?? '' },
      { assertMembership },
    )
    if (!bound.ok) return bound

    let seeded = 0
    if (deps.listArtifacts && p.runId && 'put' in previewStore) {
      try {
        const items = await deps.listArtifacts({
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

  safeHandle(ipcMain, 'files:unbind-session', async () => {
    unbindTrustedSession()
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
    try {
      const project = deps.projectCatalog.create({
        name: p.name ?? '',
        description: p.description,
        ownerId: p.ownerId || defaultOwner,
      })
      return { ok: true, project, authority: 'ui-local' }
    } catch (e: unknown) {
      return { ok: false, reason: (e as Error).message || String(e) }
    }
  })

  /**
   * Open workspace: catalog lookup → membership bind → artifact seed.
   * Single product action for renderer (Question/Plan shell entry).
   */
  safeHandle(ipcMain, 'files:open-ui-project', async (_event, payload: unknown) => {
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
    const bound = await bindTrustedSession(
      { ownerId, projectId: project.id },
      { assertMembership },
    )
    if (!bound.ok) return bound

    const runId = p.runId || project.defaultRunId
    let seeded = 0
    let seedError: string | undefined
    if (deps.listArtifacts && 'put' in previewStore) {
      try {
        const items = await deps.listArtifacts({
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

    return {
      ok: true,
      project,
      ownerId: bound.ownerId,
      projectId: bound.projectId,
      runId,
      seeded,
      seedError,
      authority: 'ui-local+lumen-bind',
    }
  })

  safeHandle(ipcMain, 'files:delete-ui-project', async (_event, payload: unknown) => {
    if (!deps.projectCatalog) {
      return { ok: false, reason: 'project catalog not configured' }
    }
    const p = (payload ?? {}) as { projectId?: string }
    const ok = deps.projectCatalog.delete(p.projectId ?? '')
    return { ok, authority: 'ui-local' }
  })
}
