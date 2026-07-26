/**
 * Science IPC registration (testable without full Electron app bootstrap).
 *
 * Single registration site for ACP proxy + OSF-2 files preview + session bind.
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
} from './session-binding'
import type { SeedableStore } from './session-binding'

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
  /** Optional override for acp:call body (tests inject). Default: loopback fetch. */
  acpFetch?: (path: string, init?: RequestInit) => Promise<unknown>
  previewStore: PreviewFileStore
  /**
   * Required for files:bind-session. Production uses ACP membership asserter;
   * tests inject fixtures. Without it, bind always fails closed.
   */
  assertMembership?: MembershipAsserter
  /**
   * Optional artifact_list for post-bind seed. When omitted, bind still works
   * but seeded count is 0.
   */
  listArtifacts?: ListArtifactsFn
}

const DEFAULT_ACP_BASE = 'http://127.0.0.1:17000'

async function defaultAcpFetch(path: string, init?: RequestInit): Promise<unknown> {
  const resp = await fetch(`${DEFAULT_ACP_BASE}${path}`, init)
  return resp.json()
}

/**
 * Register ACP + files preview + session bind handlers exactly once per ipcMain.
 * Throws if the same channel is registered twice (mock or Electron).
 */
export function registerScienceIpcHandlers(ipcMain: IpcMainLike, deps: ScienceIpcDeps): void {
  const acpFetch = deps.acpFetch ?? defaultAcpFetch
  const { safeHandle, getLumenBinaryHash, previewStore } = deps

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

  // OSF-2 product surface: preview by artifact_id under trusted session identity.
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

  // Bind only after membership assertion — never raw self-attestation.
  safeHandle(ipcMain, 'files:bind-session', async (_event, payload: unknown) => {
    const p = (payload ?? {}) as {
      ownerId?: string
      projectId?: string
      runId?: string
    }
    const assertMembership = deps.assertMembership
    if (!assertMembership) {
      return {
        ok: false,
        reason: 'no membership asserter configured — fail closed',
      }
    }
    const bound = await bindTrustedSession(
      { ownerId: p.ownerId ?? '', projectId: p.projectId ?? '' },
      { assertMembership },
    )
    if (!bound.ok) {
      return bound
    }

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
}
