/**
 * Truthful answers for capabilities Lumen deliberately did not absorb.
 *
 * The renderer came from Open Science and queries four of its subsystems on
 * mount. `ipc.ts` registers only the science + UI subset, so each invoke
 * rejected, the rejection escaped as an unhandled page error, and React never
 * rendered. The window opened blank: `#root` present, `childElementCount` 0.
 *
 * That state survived `npm run build`, `dist:full`, and 24 green authority
 * suites, because none of them launched the app. The headed E2E found it on its
 * first run.
 *
 * ## Why stubs rather than implementations
 *
 * Each of these fronts a subsystem that is intentionally absent:
 *
 *   notebook-env    Environments are provisioned by the engine, not by Electron.
 *                   `provisioner.ts` was explicitly NOT adopted (ADOPTION_PLAN):
 *                   it is a decision module that boots itself, i.e. a second
 *                   execution authority.
 *   sessions        Chat-session persistence belongs to the agent framework,
 *                   which is stubbed. Lumen's durable state lives in the Rust
 *                   SessionActor.
 *   storage         Open Science's data-root manager, superseded by the
 *                   engine's store roots.
 *   notifications   Its deep-link handoff, which has no counterpart here.
 *
 * ## Why the values are what they are
 *
 * Every answer states absence. Nothing reports readiness it does not have.
 *
 * `pythonReady: false` matters most: the UI branches on it, and a fabricated
 * `true` would advertise an environment that cannot run anything — a lie the
 * product would then act on. An empty-but-honest answer lets the UI render and
 * show the truth; a flattering one produces a working-looking app that fails at
 * the first real operation.
 */

import { homedir } from 'node:os'
import path from 'node:path'

import type { ProvisionStatus } from '../shared/notebook-env'
import {
  SESSION_MANIFEST_VERSION,
  type LoadAllSessionsResult,
} from '../shared/session-persistence'
import type { StorageInfo } from '../shared/storage'

type IpcMainLike = {
  handle(channel: string, handler: (event: unknown, ...args: unknown[]) => unknown): void
}

export type SafeHandleFn = (
  ipcMain: IpcMainLike,
  channel: string,
  handler: (event: unknown, ...args: unknown[]) => Promise<unknown>,
) => void

export type AbsentCapabilityDeps = {
  safeHandle: SafeHandleFn
  /** Where this installation keeps data, for the storage card. */
  dataRoot?: string
}

/**
 * Register the four channels the absorbed renderer needs to finish mounting.
 *
 * Returns the channels registered, so a test can assert the set rather than
 * trusting this comment to stay accurate.
 */
export function registerAbsentCapabilityIpc(
  ipcMain: IpcMainLike,
  deps: AbsentCapabilityDeps,
): string[] {
  const registered: string[] = []
  const handle = (channel: string, handler: () => unknown): void => {
    deps.safeHandle(ipcMain, channel, async () => handler())
    registered.push(channel)
  }

  const dataRoot = deps.dataRoot ?? path.join(homedir(), 'LumenScience')

  // No environment is provisioned by this process, and none is being
  // provisioned. Reporting ready would advertise something that cannot run.
  handle('notebook-env:status', (): ProvisionStatus => ({
    pythonReady: false,
    rReady: false,
    version: 0,
    provisioning: false,
  }))

  // No Open Science chat sessions exist here. An empty manifest is the
  // canonical "nothing persisted" answer, not an error.
  handle('sessions:load-all', (): LoadAllSessionsResult => ({
    sessions: [],
    manifest: { version: SESSION_MANIFEST_VERSION },
  }))

  // The storage card wants a shape it can render. Zero usage is accurate:
  // this process manages no data root of its own.
  handle('storage:get-info', (): StorageInfo => ({
    dataRoot,
    isDefault: true,
    defaultDataRoot: dataRoot,
    defaultParent: homedir(),
    dataRootMissing: false,
    legacyDataMovePrompt: false,
    usage: { categories: [], totalBytes: 0 },
    availableBytes: 0,
  }))

  // Nothing ever queues a deep-link session open, so there is never one to take.
  handle('notifications:take-pending-open-session', (): null => null)

  return registered
}
