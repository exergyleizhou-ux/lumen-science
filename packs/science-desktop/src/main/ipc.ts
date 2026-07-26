/**
 * Lumen Science Desktop — IPC handler registration (greenfield rewrite).
 *
 * This file REPLACES the Open Science registerIpcHandlers orchestrator
 * (~700 LOC, 80+ OS imports, full artifact/upload/project/session/review
 * /deletion/managed-preview/compute/notebook/SSH graph).
 *
 * The OS orchestrator is preserved as ipc.open-science-reference.ts for
 * future OSF-3 through OSF-8 surface absorption. It is NOT imported.
 *
 * This greenfield version imports ONLY:
 *   - lumen-acp-bridge (installIpcGuard, safeHandle, getLumenBinaryHash)
 *   - lumen-authority-policy (validateIpcChannel)
 *   - files/science-ipc (ACP + OSF-2 preview product path)
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

import { ipcMain, Notification } from 'electron'
import { BackendShutdownCoordinator } from './lifecycle-shutdown'
import { createLogger } from './logger'
import { installIpcGuard, safeHandle, getLumenBinaryHash, acpCall } from './lumen-acp-bridge'
import { getAllowedChannels } from './lumen-authority-policy'
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
import { registerScienceIpcHandlers } from './files/science-ipc'
import { AcpPreviewStore } from './files/acp-preview-store'

type IpcRegistrationOptions = {
  mainEntryPath: string
  headless?: boolean
  onAppIconVariantChanged?: (variant: string) => void
  listAppIconPreviews?: () => unknown[]
}

/**
 * Registers every Lumen Desktop IPC surface.
 *
 * Science operations go through registerScienceIpcHandlers:
 *   acp:call / acp:list-tools / app:get-lumen-hash / files:preview-by-artifact
 *
 * Only UI/window/settings/logs/lifecycle/update handlers are registered
 * outside that path. Science channels always pass through safeHandle.
 *
 * Returns the backend handles the app lifecycle needs to shut down cleanly.
 */
export const registerIpcHandlers = async (_opts: IpcRegistrationOptions) => {
  const log = createLogger('ipc')

  // ── Install IPC guard BEFORE any handler registration ────────
  // Marks guard installed; does NOT register channels (avoids double-handle).
  installIpcGuard(ipcMain)

  // ── Settings service ─────────────────────────────────────────
  const settingsService = createDefaultSettingsService()

  // ── UI-only IPC handlers ─────────────────────────────────────
  registerWindowIpcHandlers()
  registerLogsIpcHandlers()
  registerLifecycleIpcHandlers()
  registerUpdateIpcHandlers()
  registerSettingsIpcHandlers({
    service: settingsService,
  })

  // ── Science + OSF-2 product path (single registration site) ──
  // ACP-wired store: optional artifact_resolve via Lumen; seed via put() after list.
  const wiredStore = new AcpPreviewStore(async (tool, args) => acpCall(tool, args))

  registerScienceIpcHandlers(ipcMain, {
    safeHandle,
    getLumenBinaryHash,
    previewStore: wiredStore,
  })

  // ── Backend handles with shutdown contracts ─────────────────
  const runtime = {
    connectedAgents: [] as readonly never[],
    sessions: [] as readonly never[],
    on: () => {},
    off: () => {},
    destroy: () => { log.info('lumen runtime destroy (no-op)') },
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
    osf2Preview: true,
  })

  return {
    runtime: runtime as any,
    notebook,
    shutdownCoordinator,
    taskNotifications,
    settingsService,
    previewStore: wiredStore,
  }
}
