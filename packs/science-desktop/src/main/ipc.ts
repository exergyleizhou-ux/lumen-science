import { join } from 'node:path'
import { randomUUID } from 'node:crypto'

import { app, BrowserWindow, ipcMain, net, Notification, protocol, webContents } from 'electron'

import { createDefaultNotebookRuntimeService, registerAcpIpcHandlers } from './acp/ipc'
import { ArtifactRunRegistry } from './artifacts/run-registry'
import { waitForInitialConnectorRefresh, wireConnectorReload } from './connector-reload'
import { ApprovalBroker } from './connectors/approval-broker'
import { toCustomMcpConfig, selectEnabledCustomServers } from './connectors/custom-mcp-bootstrap'
import { McpClientManager } from './connectors/mcp-client-manager'
import { createMoleculePreviewHandler } from './connectors/molecule-preview'
import { ALL_CONNECTOR_IDS } from './connectors/registry'
import { ConnectorService } from './connectors/service'
import { syncConnectorSkillDocs, syncCustomServerSkillDocs } from './connectors/provision'
import { registerFileSaveHandlers } from './file-save'
import { registerCliInstallIpcHandlers } from './cli-install/ipc'
import { registerGithubIpcHandlers } from './github-ipc'
import { BackendShutdownCoordinator, UPDATE_SHUTDOWN_BUDGET_MS } from './lifecycle-shutdown'
import { registerLifecycleIpcHandlers } from './lifecycle-broadcast'
import { registerLogsIpcHandlers } from './logs-ipc'
import { safeHandle } from './lumen-acp-bridge'
import { assertArtifactPreviewAccess } from './lumen-authority-policy'
import { TaskNotificationService } from './notifications/task-notifications'
import {
  buildConnectorApprovalBroadcast,
  buildTaskNotificationShow
} from './notifications/electron-wiring'
import { createLogger, errorLogFields } from './logger'
import { registerManagedPreviewIpcHandlers } from './managed-preview-ipc'
import { registerManagedPreviewProtocol } from './managed-preview-protocol'
import { ManagedPreviewResources } from './managed-preview-resources'
import {
  createOfficePreviewFrameProcessResolver,
  createOfficePreviewProcessMemoryReader
} from './office-preview/office-preview-electron'
import { registerOfficePreviewIpcHandlers } from './office-preview/office-preview-ipc'
import {
  createOfficePreviewRuntimeUrl,
  registerOfficePreviewRuntimeProtocol
} from './office-preview/office-preview-runtime-protocol'
import { OfficePreviewSupervisor } from './office-preview/office-preview-supervisor'
import type { NotebookEnvironmentManager } from './notebook/runtime-service'
import { OFFICE_PREVIEW_STATE_CHANNEL } from '../shared/office-preview'
import {
  createDefaultPreviewStateRepository,
  createDefaultProjectRepository,
  registerProjectIpcHandlers
} from './projects/ipc'
import {
  createDefaultReviewRepository,
  createDefaultSessionRepository,
  registerSessionPersistenceIpcHandlers
} from './session-persistence/ipc'
import { registerProjectFilesIpcHandlers } from './project-files/ipc'
import { createManagedFileIndexRepository } from './project-files/repository'
import { ProjectDeletionCoordinator } from './projects/deletion-coordinator'
import { getProjectDbClient } from './projects/prisma-client'
import { SessionPersistenceCoordinator } from './session-persistence/coordinator'
import { type SessionPersistenceBackend } from './session-persistence/ipc'
import { tryDecryptKey } from './settings/crypto'
import { registerSettingsIpcHandlers } from './settings/ipc'
import { getAppClaudeConfigDir } from './settings/provider-env'
import { createDefaultSettingsService, type SettingsService } from './settings/service'
import type { StoredConnectors } from './settings/types'
import type { AppIconPreview, AppIconVariant } from '../shared/settings'
import { registerStorageIpcHandlers } from './storage/ipc'
import { normalizeLegacyDataPaths } from './storage/normalize-legacy-paths'
import {
  computeDefaultDataRoot,
  initDataRoot,
  resolveDataRoot,
  resolveStorageRoot,
  samePath
} from './storage-root'
import { registerUpdateIpcHandlers } from './update/ipc'
import { startUpdateScheduler } from './update/scheduler'
import { createDefaultUploadRepository, registerUploadIpcHandlers } from './uploads/ipc'
import { broadcastToRenderers } from './renderer-broadcast'

type IpcRegistrationOptions = {
  mainEntryPath: string
  // Headless web-serve launches (--serve) have no local desktop user; task notifications are
  // disabled there by contract, not just incidentally via Notification.isSupported().
  headless?: boolean
  // Applies a newly-selected app-icon variant to the window + dock/taskbar. Supplied by the desktop
  // startup path; absent in web/headless mode (no local window to re-skin).
  onAppIconVariantChanged?: (variant: AppIconVariant) => void
  // Renders the built-in icon variants to preview data URLs for the Appearance picker.
  listAppIconPreviews?: () => AppIconPreview[]
}

// Builds a short, human-readable preview of a connector call's arguments for the approval card.
const previewArgs = (args: Record<string, unknown>): string => {
  let json: string
  try {
    json = JSON.stringify(args)
  } catch {
    json = '{…}'
  }
  return json.length > 300 ? `${json.slice(0, 300)}…` : json
}

// Reads the connectors settings block and refreshes the mcp-<connector>/mcp-<server> skill docs to
// match — both the bundled catalog and any enabled custom MCP servers (stdio + remote). Called at
// startup;
// a future connectors-settings mutation (Plan 2/5 UI) should call this again so enable/disable
// (bundled or custom) takes effect without an app restart. Never throws — a bad read or a
// misconfigured/unreachable custom server (e.g. bad command) is logged and leaves the previous
// snapshot and on-disk docs in place rather than breaking bootstrap.
const refreshConnectorSkillDocs = async (
  settingsService: SettingsService,
  storageRoot: string,
  mcpClientManager: McpClientManager,
  onSnapshot: (connectors: StoredConnectors | undefined) => void
): Promise<void> => {
  try {
    const connectors = await settingsService.getConnectors()

    onSnapshot(connectors)
    const skillsDir = join(getAppClaudeConfigDir(storageRoot), 'skills')

    // Opt-out model: every bundled connector is enabled unless explicitly disabled.
    const disabled = new Set(connectors?.disabledConnectorIds ?? [])
    const enabledIds = ALL_CONNECTOR_IDS.filter((id) => !disabled.has(id))

    await syncConnectorSkillDocs(skillsDir, enabledIds)
    await syncCustomServerSkillDocs(skillsDir, selectEnabledCustomServers(connectors), (server) =>
      mcpClientManager.listTools(toCustomMcpConfig(server))
    )
  } catch (error) {
    console.error('Failed to sync connector skill docs:', error)
  }
}

// Registers every main-process IPC surface used by the renderer. Async because the notebook-env gate
// needs the configured package mirror, read from disk; callers await this before creating the main
// window so every IPC channel (incl. notebook-env) is registered before the renderer can call it.
const registerIpcHandlers = async ({
  mainEntryPath,
  headless = false,
  onAppIconVariantChanged,
  listAppIconPreviews
}: IpcRegistrationOptions): Promise<{
  runtime: ReturnType<typeof registerAcpIpcHandlers>
  notebook: ReturnType<typeof createDefaultNotebookRuntimeService>
  shutdownCoordinator: BackendShutdownCoordinator
  taskNotifications: TaskNotificationService
  settingsService: SettingsService
}> => {
  // One settings service backs both the settings IPC and the ACP spawn config (single source of truth).
  const settingsService = createDefaultSettingsService()
  const storedSettings = await settingsService.getStoredSettings()
  // Prime the data-root cache from settings before any data repository is constructed below. A change
  // to this value only takes effect after a restart, so reading it once here is sufficient.
  initDataRoot(storedSettings.dataRoot)
  // Recovery breadcrumb: if settings.json is ever lost/corrupted, the resolved dataRoot from the
  // last successful launch is still findable in the logs, so a user with data at a non-default
  // location isn't left guessing where it went.
  createLogger('storage').info('data root resolved', {
    dataRoot: resolveDataRoot(),
    isDefault: samePath(resolveDataRoot(), computeDefaultDataRoot())
  })

  // Constructed once here (rather than left to each register*IpcHandlers' own default) so the
  // one-time legacy-path normalization pass below can share the exact instances the IPC surface uses.
  const sessionRepository = createDefaultSessionRepository()
  const projectRepository = createDefaultProjectRepository()
  const previewStateRepository = createDefaultPreviewStateRepository()

  // One-time conversion of any legacy absolute data-root paths on disk (pre-$DATA-sentinel installs)
  // into the portable "$DATA/..." form, guarded so it only ever runs once. Never allowed to block
  // startup on failure: an error is logged and the marker stays unset, so the pass simply retries on
  // the next launch.
  if (!storedSettings.pathsNormalizedAt) {
    try {
      await normalizeLegacyDataPaths({
        sessionRepository,
        previewStateRepository,
        projectRepository,
        dataRoot: resolveDataRoot()
      })
      await settingsService.markPathsNormalized()
    } catch (error) {
      createLogger('storage').error(
        'legacy path normalization failed; will retry next launch',
        error
      )
    }
  }

  // Share one repository and registry so runtime artifact claims and renderer finalization meet.
  const artifactRepository = createDefaultArtifactRepository()
  const artifactRunRegistry = new ArtifactRunRegistry()
  // Share one upload repository so composer staging, prompt finalization, and previews agree.
  const uploadRepository = createDefaultUploadRepository()
  // One source-neutral resolver keeps previews and user-requested exports on identical trust checks.
  const resolveManagedFilePath = (
    source: 'artifact' | 'upload',
    request: { path: string }
  ): Promise<string> =>
    source === 'artifact'
      ? artifactRepository.resolveManagedFilePath(request)
      : uploadRepository.resolveManagedUploadPath(request)
  // One registry owns short-lived capability URLs for both managed artifact repositories.
  const previewResources = new ManagedPreviewResources({
    resolvePath: resolveManagedFilePath
  })

  // Construct one storage/index/deletion graph for every related IPC surface. Sharing these instances
  // is essential: separate coordinators would have independent queues and recovery gates.
  const configRoot = resolveStorageRoot()
  const projectFilesRepository = createManagedFileIndexRepository(
    getProjectDbClient,
    configRoot,
    resolveDataRoot()
  )
  const sessionPersistenceCoordinator = new SessionPersistenceCoordinator(
    sessionRepository,
    projectFilesRepository,
    (event) => broadcastToRenderers('project-files:changed', event)
  )
  const reviewRepository = createDefaultReviewRepository()
  const projectDeletionCoordinator = new ProjectDeletionCoordinator(
    projectRepository,
    sessionPersistenceCoordinator,
    previewStateRepository,
    reviewRepository
  )
  const sessionPersistenceBackend: SessionPersistenceBackend = {
    loadAll: async () => {
      await projectDeletionCoordinator.recoverPendingDeletions()
      return sessionPersistenceCoordinator.loadAll()
    },
    saveSession: async (session) => {
      await projectDeletionCoordinator.recoverPendingDeletions()
      const created =
        (await sessionRepository.loadSession(session.projectId, session.id)) === undefined
      await sessionPersistenceCoordinator.saveSession(session)
      return created
    },
    deleteSession: async (projectId, sessionId) => {
      await projectDeletionCoordinator.recoverPendingDeletions()
      return sessionPersistenceCoordinator.deleteSession(projectId, sessionId)
    },
    deleteProjectSessions: async (projectId) => {
      await projectDeletionCoordinator.recoverPendingDeletions()
      return sessionPersistenceCoordinator.deleteProjectSessions(projectId)
    },
    saveManifest: async (request) => {
      await projectDeletionCoordinator.recoverPendingDeletions()
      return sessionPersistenceCoordinator.saveManifest(request)
    }
  }
  registerFileSaveHandlers()
  registerGithubIpcHandlers(configRoot)
  registerManagedPreviewProtocol(previewResources, resolveManagedFilePath, logger('managed-preview'))

  // ── Lumen authority gate at registration choke points ─────────
  // safeHandle verifies every registered channel against the shipped
  // lumen-authority-policy.ts allowlist. Banned channels are rejected
  // at registration time (fail-fast), not at runtime (fail-open).
  safeHandle(ipcMain, 'preview:load', async () => ({}))
  safeHandle(ipcMain, 'preview:save', async () => ({}))
  safeHandle(ipcMain, 'preview:delete', async () => ({}))
  // Open Science artifact channels are banned — safeHandle rejects them:
  safeHandle(ipcMain, 'artifacts:finalize-run', async () => ({}))
  safeHandle(ipcMain, 'artifacts:open-file', async () => ({}))
  safeHandle(ipcMain, 'artifacts:read-preview', async () => ({}))
  // Reviewer orchestration must go through Rust:
  safeHandle(ipcMain, 'reviewer:run', async () => ({}))
  safeHandle(ipcMain, 'reviewer:abort-fix-loop', async () => ({}))

  registerManagedPreviewIpcHandlers(resolveManagedFilePath)
  registerCliInstallIpcHandlers()
  registerWindowIpcHandlers()
  registerLogsIpcHandlers()
  registerLifecycleIpcHandlers()

  // ── Lumen runtime: stub ACP bridge, no Open Science multi-agent ──
  // registerAcpIpcHandlers would construct the full Open Science
  // AcpRuntimeCoordinator + Claude/Codex/OpenCode backends.
  // Lumen routes ALL science through acp:call → Rust Lumen binary.
  // This stub keeps the return type shape so index.ts compiles.
  const logger = createLogger('lumen-bridge')
  const runtime = {
    connectedAgents: [],
    sessions: [],
    on: () => {},
    off: () => {},
    destroy: () => { logger.info('lumen bridge destroy (no-op)') },
  } as ReturnType<typeof registerAcpIpcHandlers>

  // Constructed after settings/repo setup so index.ts can reference them.
  const taskNotifications = new TaskNotificationService({
    show: buildTaskNotificationShow(BrowserWindow, Notification),
  })

  const shutdownCoordinator = new BackendShutdownCoordinator()

  // Stub notebook return — Electron does NOT execute kernels.
  // Kernel execution is owned by Rust Lumen KernelAdapter (follow-on).
  const notebookService = {
    execute: () => Promise.reject(new Error('Notebook execution stubbed — use ACP bridge')),
    interrupt: () => {},
    shutdown: () => {},
    get history() { return [] },
    on: () => {},
    off: () => {},
  } as ReturnType<typeof createDefaultNotebookRuntimeService>

  // Return the long-lived backend handles. Science handles are stubs;
  // all real execution goes through ACP proxy to Rust Lumen binary.
  return {
    runtime,
    notebook: notebookService,
    shutdownCoordinator,
    taskNotifications,
    settingsService
  }
}

export { registerIpcHandlers }
