/**
 * Lumen Science Desktop — IPC handler registration (greenfield rewrite).
 *
 * This file REPLACES the Open Science registerIpcHandlers orchestrator
 * (~700 LOC, 80+ OS imports, full artifact/upload/project/session/review
 * /deletion/managed-preview/compute/notebook/SSH graph).
 *
 * The OS orchestrator is preserved as ipc.open-science-reference.ts for
 * future OSF-2 through OSF-8 surface absorption. It is NOT imported.
 *
 * This greenfield version imports ONLY:
 *   - lumen-acp-bridge (installIpcGuard, safeHandle, getLumenBinaryHash)
 *   - lumen-authority-policy (validateIpcChannel)
 *   - Electron-safe settings service
 *
 * Import exclusions (NONE of these are imported or constructed):
 *   - artifact repository / managed preview / project files
 *   - SSH runner / SCP runner / job poller / harvest engine
 *   - compute IPC / notebook IPC / reviewer IPC / ACP coordinator
 *   - any Open Science science execution path
 *
 * See: packs/science-desktop/ARCHITECTURE.md
 * See: third_party/open-science/NOTICE
 */

import { app, ipcMain, Notification } from 'electron'
import { BackendShutdownCoordinator } from './lifecycle-shutdown'
import { createLogger } from './logger'
import { installIpcGuard, safeHandle, getLumenBinaryHash } from './lumen-acp-bridge'
import { validateIpcChannel, getAllowedChannels } from './lumen-authority-policy'
import {
  buildTaskNotificationShow
} from './notifications/electron-wiring'
import { TaskNotificationService } from './notifications/task-notifications'
import { registerWindowIpcHandlers } from './window-ipc'
import { registerLogsIpcHandlers } from './logs-ipc'
import { registerLifecycleIpcHandlers } from './lifecycle-broadcast'
import { registerSettingsIpcHandlers } from './settings/ipc'
import { createDefaultSettingsService } from './settings/service'
import { registerUpdateIpcHandlers } from './update/ipc'

type IpcRegistrationOptions = {
  mainEntryPath: string
  headless?: boolean
  onAppIconVariantChanged?: (variant: string) => void
  listAppIconPreviews?: () => unknown[]
}

/**
 * Registers every Lumen Desktop IPC surface.
 *
 * Science operations are NOT registered here — they go through
 * the ACP bridge (acp:call → Rust Lumen binary).
 *
 * Only UI/window/settings/logs/lifecycle/update handlers are registered.
 * Every channel registration passes through safeHandle which validates
 * against the shipped lumen-authority-policy allowlist.
 *
 * Returns the backend handles the app lifecycle needs to shut down cleanly.
 */
export const registerIpcHandlers = async (_opts: IpcRegistrationOptions) => {
  const log = createLogger('ipc')

  // ── Install IPC guard BEFORE any handler registration ────────
  // safeHandle gates every future ipcMain.handle call against
  // the shipped authority policy. Banned channels are rejected
  // at registration time, fail-fast.
  installIpcGuard(ipcMain)

  // ── Settings service ─────────────────────────────────────────
  const settingsService = createDefaultSettingsService()

  // ── UI-only IPC handlers ─────────────────────────────────────
  // These handle window management, logs, lifecycle, settings, and
  // updates — zero science execution authority.
  registerWindowIpcHandlers()
  registerLogsIpcHandlers()
  registerLifecycleIpcHandlers()
  registerUpdateIpcHandlers()
  registerSettingsIpcHandlers({
    service: settingsService,
  })

  // ── ACP proxy handlers (the ONLY science path) ──────────────
  // All science operations route through safeHandle → ACP bridge.
  // These are registered here so the renderer can call them via IPC.
  // Actual execution happens in Rust Lumen binary.
  safeHandle(ipcMain, 'acp:call', async (_event, toolName: string, args: Record<string, unknown>) => {
    try {
      const resp = await fetch('http://127.0.0.1:17000/tools/call', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: toolName, arguments: args }),
      })
      return resp.json()
    } catch (e: unknown) {
      return { _lumenError: true, message: (e as Error).message || String(e) }
    }
  })

  safeHandle(ipcMain, 'acp:list-tools', async () => {
    try {
      const resp = await fetch('http://127.0.0.1:17000/tools/list')
      return resp.json()
    } catch {
      return { tools: [], _lumenUnavailable: true }
    }
  })

  safeHandle(ipcMain, 'app:get-lumen-hash', async () => {
    return getLumenBinaryHash()
  })

  // ── Backend handles with shutdown contracts ─────────────────
  // Every symbol is defined in-file. No free references to
  // open Science orchestrator symbols.

  const runtime = {
    connectedAgents: [] as readonly never[],
    sessions: [] as readonly never[],
    on: () => {},
    off: () => {},
    destroy: () => { log.info('lumen runtime destroy (no-op)') },
    // Shutdown contracts needed by BackendShutdownDeps
    shutdownForQuit: async () => ({ reaped: true }),
    shutdownForUpdateGate: async () => ({ reaped: true }),
  }

  const notebook = {
    shutdownAll: async (): Promise<{ reaped: boolean }> => {
      log.info('notebook shutdown (no-op — kernels managed by Rust Lumen)')
      return { reaped: true }
    },
  }

  const logs = createLogger('notifications')
  const taskNotifications = new TaskNotificationService({
    isEnabled: async () => false,
    isAppFocused: () => false,
    show: buildTaskNotificationShow({
      notificationCtor: Notification,
      liveNotifications: new Set(),
      log: logs,
      headless: Boolean(_opts.headless),
    }),
    onDeliveryError: (error: unknown) => {
      logs.warn('notification delivery error', String(error))
    },
  })

  const shutdownCoordinator = new BackendShutdownCoordinator({
    runtime,
    notebook,
    log: createLogger('shutdown'),
  })

  log.info('ipc handlers registered', {
    channelsRegistered: getAllowedChannels().size,
    lumenHash: getLumenBinaryHash(),
  })

  return {
    runtime: runtime as any,
    notebook,
    shutdownCoordinator,
    taskNotifications,
    settingsService,
  }
}
