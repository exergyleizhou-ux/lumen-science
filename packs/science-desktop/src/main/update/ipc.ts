import { app, ipcMain } from 'electron'

import { APP } from '../../shared/app-config'
import type { AppInfo, UpdateStatus } from '../../shared/update'
import { resolveUpdatePolicy, type UpdatePolicy } from '../../shared/update-policy'
import { createUpdateStrategy } from './create-strategy'
import { DisabledUpdateStrategy } from './disabled-strategy'
import type { UpdateStrategy } from './strategy'

/**
 * Pick the strategy for the resolved policy (LS5-R1-02).
 *
 * The ordering matters: the policy is consulted *before* createUpdateStrategy()
 * runs, because that factory constructs an ElectronUpdaterStrategy which binds
 * electron-updater's `autoUpdater` at construction time. Deciding afterwards
 * would already have created the network client we are trying not to create.
 */
export const selectUpdateStrategy = (
  policy: UpdatePolicy,
  currentVersion: string
): UpdateStrategy =>
  policy.enabled ? createUpdateStrategy() : new DisabledUpdateStrategy(currentVersion, policy.reason)

// Registers the renderer-callable update commands. Returns the strategy so the scheduler can drive it.
//
// Callers may inject a strategy (tests, and any future explicitly-configured
// path). When they do not, the policy decides — and with no Lumen-owned feed
// configured the result is DisabledUpdateStrategy, which holds no network
// client at all.
export const registerUpdateIpcHandlers = (strategy?: UpdateStrategy): UpdateStrategy => {
  const resolved =
    strategy ??
    selectUpdateStrategy(
      resolveUpdatePolicy(process.env),
      app?.getVersion?.() ?? '0.0.0'
    )

  ipcMain.handle('update:get-app-info', (): AppInfo => ({
    name: APP.name,
    version: resolved.getStatus().current,
    copyright: APP.copyright
  }))
  ipcMain.handle('update:get-status', (): UpdateStatus => resolved.getStatus())
  ipcMain.handle('update:check', (): Promise<UpdateStatus> => resolved.check())
  ipcMain.handle('update:download', (): Promise<UpdateStatus> => resolved.download())
  ipcMain.handle('update:cancel', (): Promise<UpdateStatus> => resolved.cancel())
  ipcMain.handle('update:apply', (): Promise<UpdateStatus> => resolved.apply())
  return resolved
}
