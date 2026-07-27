/**
 * Bridges an engine permission ask to the renderer and back.
 *
 * The main process OWNS the request id. The renderer only ever answers an id it
 * was given, so a reply naming an id nobody issued is discarded and the real
 * request goes on waiting for its own answer — or times out and denies. A
 * renderer cannot manufacture an approval by guessing an id.
 *
 * There is no `permission:request` channel in the other direction: the renderer
 * cannot originate a permission ask. Only the engine can, and it does so over
 * ACP.
 */

import type { BrowserWindow } from 'electron'

import type { AskHuman, PermissionAsk, PermissionDecision } from './permission-broker'

type IpcMainLike = {
  handle(channel: string, handler: (event: unknown, ...args: unknown[]) => unknown): void
}

export type SafeHandleFn = (
  ipcMain: IpcMainLike,
  channel: string,
  handler: (event: unknown, ...args: unknown[]) => Promise<unknown>,
) => void

export type PermissionIpcDeps = {
  safeHandle: SafeHandleFn
  /** The window to ask. Resolved per-request: it may close between asks. */
  getWindow: () => BrowserWindow | null
}

const isDecision = (value: unknown): value is PermissionDecision =>
  value === 'allow_once' || value === 'reject'

/**
 * Register the response channel and return the `ask` the broker will call.
 */
export function registerPermissionIpc(
  ipcMain: IpcMainLike,
  deps: PermissionIpcDeps,
): AskHuman {
  const waiting = new Map<string, (decision: PermissionDecision) => void>()

  deps.safeHandle(ipcMain, 'permission:respond', async (_event, ...args: unknown[]) => {
    const [requestId, decision] = args
    if (typeof requestId !== 'string' || !isDecision(decision)) {
      // Malformed reply. Not an error to the caller, and deliberately NOT a
      // denial of the pending request either: resolving someone else's ask from
      // a bad payload would be worse than letting it time out.
      return { ok: false, reason: 'malformed permission response' }
    }
    const resolve = waiting.get(requestId)
    if (!resolve) {
      // An id the main process never issued, or one already settled.
      return { ok: false, reason: 'unknown or already-settled permission request' }
    }
    waiting.delete(requestId)
    resolve(decision)
    return { ok: true }
  })

  return (ask: PermissionAsk): Promise<PermissionDecision> => {
    const window = deps.getWindow()
    if (!window || window.isDestroyed()) {
      // Throwing reaches the broker's catch, which denies. Returning 'reject'
      // here would be indistinguishable from a human declining, and the log
      // should say which happened.
      throw new Error('no window is available to show the permission prompt')
    }

    return new Promise<PermissionDecision>((resolve) => {
      waiting.set(ask.requestId, resolve)
      window.webContents.send('permission:ask', ask)

      // If the window goes away mid-ask, stop waiting for an answer that can no
      // longer arrive. The broker's timeout would eventually deny, but a closed
      // window is a definite answer now rather than in five minutes.
      const onGone = (): void => {
        if (waiting.delete(ask.requestId)) resolve('reject')
      }
      window.once('closed', onGone)
      window.webContents.once('destroyed', onGone)
    })
  }
}
