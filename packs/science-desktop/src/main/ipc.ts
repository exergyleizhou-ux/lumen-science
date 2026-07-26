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

import { app, ipcMain, Notification } from 'electron'
import type { AppIconPreview, AppIconVariant } from '../shared/settings'
import { BackendShutdownCoordinator } from './lifecycle-shutdown'
import { createLogger } from './logger'
import {
  installIpcGuard,
  safeHandle,
  getLumenBinaryHash,
  acpCall,
  listScienceTools
} from './lumen-acp-bridge'
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
import {
  createAcpMembershipAsserter,
  listArtifactsViaAcp,
} from './files/acp-membership'
import { createAcpAuthoritativeMembershipAsserter } from './files/hybrid-membership'
import { getDefaultLocalProjectCatalog } from './files/local-project-catalog'
import { resolveStorageRoot } from './storage-root'
import { runtimeRoot } from './notebook/runtime-paths'
import { join } from 'node:path'

type IpcRegistrationOptions = {
  mainEntryPath: string
  headless?: boolean
  // AppIconVariant, not string: settings/ipc.ts validates with isAppIconVariant before emitting,
  // and app-icon.ts's setVariant only accepts the union. `string` here was wider than either end of
  // the wire, so index.ts's callback argument could not be forwarded to the controller.
  onAppIconVariantChanged?: (variant: AppIconVariant) => void
  listAppIconPreviews?: () => AppIconPreview[]
}

/**
 * Registers every Lumen Desktop IPC surface.
 *
 * Science operations go through registerScienceIpcHandlers:
 *   acp:* / app:get-lumen-hash / files:preview-by-artifact /
 *   files:bind-session / files:unbind-session
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
  // ACP-wired store + hybrid membership (ACP then local UI catalog) + seed.
  const acpTool = async (tool: string, args: Record<string, unknown>) =>
    acpCall(tool, args)
  const wiredStore = new AcpPreviewStore(acpTool)
  const catalogPath = join(app.getPath('userData'), 'lumen-ui-projects.json')
  const projectCatalog = getDefaultLocalProjectCatalog(catalogPath)

  registerScienceIpcHandlers(ipcMain, {
    safeHandle,
    getLumenBinaryHash,
    // The real transport: an ACP extension-method call over stdio. No HTTP
    // shape, no fake Request, no loopback port.
    callScienceTool: acpCall,
    listScienceTools,
    previewStore: wiredStore,
    // ACP is the sole authority. The local catalog is a display cache and can
    // no longer grant membership — see files/hybrid-membership.ts.
    assertMembership: createAcpAuthoritativeMembershipAsserter({
      acp: createAcpMembershipAsserter(acpTool),
    }),
    listArtifacts: ({ projectId, runId }) =>
      listArtifactsViaAcp(acpTool, { projectId, runId }),
    projectCatalog,
    defaultOwnerId: process.env.LUMEN_DESKTOP_OWNER_ID || 'local-user',
    // LS5-K4: where this installation's environments live. Resolved here
    // because it needs Electron's app paths; the adapter itself stays
    // Electron-free so the authority scripts can execute the shipped module.
    runtimeRoot: runtimeRoot(resolveStorageRoot()),
  })

  // ── Backend handles with shutdown contracts ─────────────────
  //
  // Both stubs must also answer storage/detect-active.ts, which asks each backend which of its
  // sessions is mid-flight so the migration and close/quit flows can warn before interrupting work.
  // This process cannot answer that from its own state — agent prompts and notebook kernels both
  // run inside the Rust SessionActor — so it answers with the truth it has: none that IT is
  // running. (Empty is also what the old code effectively meant, except it omitted the methods
  // entirely, so detectActiveSessions threw a TypeError the first time a close/quit asked.)
  // Surfacing the Rust process's real in-flight set is a separate task; it needs an ACP query.
  type ActiveSessionSource = { projectName: string; sessionId: string }

  const runtime = {
    connectedAgents: [] as readonly never[],
    sessions: [] as readonly never[],
    on: () => {},
    off: () => {},
    destroy: () => { log.info('lumen runtime destroy (no-op)') },
    shutdownForQuit: async () => ({ reaped: true }),
    shutdownForUpdateGate: async () => ({ reaped: true }),
    getActivePromptSessions: (): ActiveSessionSource[] => [],
  }

  const notebook = {
    shutdownAll: async (): Promise<{ reaped: boolean }> => {
      log.info('notebook shutdown (no-op — kernels managed by Rust Lumen)')
      return { reaped: true }
    },
    getActiveNotebookSessions: (): ActiveSessionSource[] => [],
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
    runtime,
    notebook,
    shutdownCoordinator,
    taskNotifications,
    settingsService,
    previewStore: wiredStore,
  }
}
