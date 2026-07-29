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

import { app, BrowserWindow, dialog, ipcMain, Notification } from 'electron'
import type { AppIconPreview, AppIconVariant } from '../shared/settings'
import { BackendShutdownCoordinator } from './lifecycle-shutdown'
import { createLogger } from './logger'
import {
  installIpcGuard,
  safeHandle,
  getLumenBinaryHash,
  acpCall,
  listScienceTools,
  setPermissionPrompt,
  denyPendingPermissions
} from './lumen-acp-bridge'
import { getAllowedChannels } from './lumen-authority-policy'
import { registerPermissionIpc } from './permission-ipc'
import { registerAbsentCapabilityIpc } from './absent-capability-ipc'
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
import { readFile, writeFile } from 'node:fs/promises'
import { basename, join } from 'node:path'

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

  // ── Absent capabilities ──────────────────────────────────────
  // The absorbed renderer queries four Open Science subsystems on mount. They
  // are intentionally absent here, and an unregistered channel rejects into an
  // unhandled page error that stops React rendering entirely. These answer
  // honestly that there is nothing, which lets the UI mount and tell the truth.
  registerAbsentCapabilityIpc(ipcMain, { safeHandle })

  // ── Permission prompts ───────────────────────────────────────
  // Until this was wired the engine's session/request_permission got -32601 and
  // every approval-requiring mutation failed. The prompt is installed rather
  // than defaulted: with none installed the broker DENIES, so an engine request
  // arriving before a window exists is refused instead of auto-approved.
  setPermissionPrompt(
    registerPermissionIpc(ipcMain, {
      safeHandle,
      getWindow: () => BrowserWindow.getAllWindows()[0] ?? null,
    }),
  )

  // ── Science + OSF-2 product path (single registration site) ──
  // ACP-wired store + ACP-authoritative membership + verified artifact seed.
  const acpTool = async (tool: string, args: Record<string, unknown>) =>
    acpCall(tool, args)
  const wiredStore = new AcpPreviewStore(acpTool)
  const catalogPath = join(app.getPath('userData'), 'lumen-ui-projects.json')
  const projectCatalog = getDefaultLocalProjectCatalog(catalogPath)
  const biomniUniprotFixturePath = app.isPackaged
    ? join(process.resourcesPath, 'science', 'fixtures', 'connector_uniprot_search.json')
    : join(
        __dirname,
        '../../../../agent/crates/codegen/xai-grok-science/fixtures/connector_uniprot_search.json',
      )
  const biomniUniprotFixtureBase64 = (
    await readFile(biomniUniprotFixturePath)
  ).toString('base64')

  /**
   * Write an export to a path the USER chooses.
   *
   * "Export .ipynb" and "Export dossier" produced a JSON dump in an output
   * pane and wrote nothing anywhere — a notebook you cannot get out of the app
   * is not an export, and the button said otherwise.
   *
   * The renderer supplies contents and a suggested filename; it never supplies
   * a destination. The path comes from the OS save dialog, so a compromised
   * renderer cannot pick where bytes land, and nothing is written unless a
   * human confirmed this exact file.
   */
  safeHandle(ipcMain, 'files:save-export', async (event, payload: unknown) => {
    const p = (payload ?? {}) as { suggestedName?: string; contents?: string }
    if (typeof p.contents !== 'string' || p.contents.length === 0) {
      return { ok: false, reason: 'there is nothing to export yet' }
    }
    // basename() so a suggested name can never carry a directory out of the
    // dialog's starting point.
    const suggested = basename(p.suggestedName || 'export.json')
    const sender = (event as { sender?: Electron.WebContents })?.sender
    const win = sender ? BrowserWindow.fromWebContents(sender) : null
    const result = win
      ? await dialog.showSaveDialog(win, { defaultPath: suggested })
      : await dialog.showSaveDialog({ defaultPath: suggested })
    if (result.canceled || !result.filePath) {
      // Not an error: choosing not to save is a normal outcome, and reporting
      // it as a failure trains people to ignore the message.
      return { ok: false, canceled: true }
    }
    try {
      await writeFile(result.filePath, p.contents, 'utf-8')
      return { ok: true, path: result.filePath, bytes: Buffer.byteLength(p.contents) }
    } catch (e: unknown) {
      return { ok: false, reason: (e as Error).message || String(e) }
    }
  })

  registerScienceIpcHandlers(ipcMain, {
    safeHandle,
    getLumenBinaryHash,
    // The real transport: an ACP extension-method call over stdio. No HTTP
    // shape, no fake Request, no loopback port.
    callScienceTool: acpCall,
    listScienceTools,
    previewStore: wiredStore,
    // The engine resolves relative store paths against its session cwd, which
    // the bridge pins to userData. Passing the same root here is what lets a
    // workflow report's relative artifact entries become absolute local paths.
    workspaceRoot: app.getPath('userData'),
    // Resolved from where the files ACTUALLY are, in both shapes this app
    // runs in. They were resolved relative to process.cwd(), which is the
    // repo in dev and something else entirely in an installed app — so these
    // two tabs worked only on a developer's machine. `extraResources` in
    // electron-builder.yml is what puts them under resourcesPath.
    skillsRegistryPath: app.isPackaged
      ? join(process.resourcesPath, 'science', 'skills-registry.json')
      : join(__dirname, '../../../../packs/science/skills/registry.json'),
    skillsEcosystemCatalogPaths: app.isPackaged
      ? [
          join(process.resourcesPath, 'science', 'ecosystem-skill-catalog.json'),
          join(process.resourcesPath, 'science', 'biomni-tool-catalog.json'),
          join(process.resourcesPath, 'science', 'biomni-resource-catalog.json'),
        ]
      : [
          join(__dirname, '../../../../packs/science/skills/ecosystem/scp-catalog.json'),
          join(
            __dirname,
            '../../../../packs/science/skills/ecosystem/biomni-tool-catalog.json',
          ),
          join(
            __dirname,
            '../../../../packs/science/skills/ecosystem/biomni-resource-catalog.json',
          ),
        ],
    biomniUniprotFixtureBase64,
    skillsAdmissionPath: app.isPackaged
      ? join(process.resourcesPath, 'science', 'admissions', 'biomni-query-uniprot.json')
      : join(
          __dirname,
          '../../../../docs/science/5.0/admissions/biomni-query-uniprot.admission.json',
        ),
    connectorLockPath: app.isPackaged
      ? join(process.resourcesPath, 'science', 'fusion-sources.lock.json')
      : join(__dirname, '../../../../docs/science/fusion-sources.lock.json'),
    // ACP is the sole authority. The local catalog is a display cache and can
    // no longer grant membership — see files/hybrid-membership.ts.
    assertMembership: createAcpAuthoritativeMembershipAsserter({
      acp: createAcpMembershipAsserter(acpTool),
    }),
    listArtifacts: ({ ownerId, projectId, runId }) =>
      listArtifactsViaAcp(acpTool, {
        ownerId,
        projectId,
        runId,
      }),
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
    // Deny anything still waiting on a human. The broker's timeout timers are
    // deliberately ref'd so an awaited ask always settles, which means quit has
    // to resolve them rather than leave the engine waiting on an answer that
    // can no longer come. Closing the app is not approval.
    shutdownForQuit: async () => {
      const denied = denyPendingPermissions('the application is closing')
      if (denied > 0) log.info(`denied ${denied} pending permission request(s) on quit`)
      return { reaped: true }
    },
    shutdownForUpdateGate: async () => {
      denyPendingPermissions('the application is updating')
      return { reaped: true }
    },
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
