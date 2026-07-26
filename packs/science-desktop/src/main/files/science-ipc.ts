/**
 * Science IPC registration (testable without full Electron app bootstrap).
 *
 * Single registration site for ACP proxy + OSF-2 files preview.
 * installIpcGuard does NOT register channels — only this module does via safeHandle.
 */

import type { PreviewFileStore } from './preview-resolver'
import { loadArtifactPreview } from './preview-service'

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

export type ScienceIpcDeps = {
  safeHandle: SafeHandleFn
  getLumenBinaryHash: () => string | null
  /** Optional override for acp:call body (tests inject). Default: loopback fetch. */
  acpFetch?: (path: string, init?: RequestInit) => Promise<unknown>
  previewStore: PreviewFileStore
}

const DEFAULT_ACP_BASE = 'http://127.0.0.1:17000'

async function defaultAcpFetch(path: string, init?: RequestInit): Promise<unknown> {
  const resp = await fetch(`${DEFAULT_ACP_BASE}${path}`, init)
  return resp.json()
}

/**
 * Register ACP + files preview handlers exactly once per ipcMain.
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
  // Identity is set only by main-process project/session open (session-identity.ts),
  // never via a renderer self-attestation channel.
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
}
