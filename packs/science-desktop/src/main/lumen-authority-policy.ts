/**
 * Lumen Authority Policy — pure module (no Electron imports).
 *
 * Defines the IPC channel whitelist/banlist and artifact access rules
 * for Lumen Science Desktop. Importable from plain Node.js tests.
 *
 * These are the REAL production channel names from the shipping IPC files,
 * not fictional names. Banned channels correspond to Open Science execution
 * paths that must go through Rust Lumen SessionActor instead.
 *
 * Apache-2.0. Adapted from Open Science (d8f11e34).
 */

// ── Channel classification ───────────────────────────────────────

/**
 * Channels that Electron main MAY register handlers for.
 * These are UI-only operations that do not execute science.
 */
const ALLOWED_CHANNELS = new Set<string>([
  // Window & app lifecycle
  'window:minimize',
  'window:maximize',
  'window:close',
  'window:toggle-fullscreen',
  'app:quit',
  'app:get-version',
  'app:get-lumen-hash',

  // Permission prompts. The renderer RESPONDS to an ask the engine initiated;
  // it cannot originate one, and `permission:respond` carries a request id the
  // main process issued, so a reply to an unknown id is discarded rather than
  // resolving something else.
  'permission:respond',

  // Absent-capability status queries. The absorbed renderer calls these on
  // mount; each fronts a subsystem Lumen deliberately did not absorb, and each
  // answers with the truthful "nothing here" rather than a fabricated ready
  // state. Without them the invokes rejected and React never rendered — the
  // window opened blank. See main/absent-capability-ipc.ts.
  'notebook-env:status',
  'sessions:load-all',
  'storage:get-info',
  'notifications:take-pending-open-session',
  // Tray & notifications
  'tray:update',
  'notification:show',
  // Updater
  'updater:check',
  'updater:install',
  // Settings (persisted UI preferences only)
  'settings:get',
  'settings:set',
  // Uploaded bundles enter a main-owned ingress and are captured, approved
  // and persisted only by the Rust SessionActor quarantine route.
  'settings:import-skill-zip',
  'settings:import-skill-zip-batch',
  // Native dialogs
  'dialog:open-file',
  'dialog:save-file',
  // Clipboard (read-only, no science state)
  'clipboard:write',
  // Session layout save/restore (UI state, not science persistence)
  'session:restore-layout',
  'session:save-layout',
  // ACP proxy — routes ALL science operations to Rust Lumen
  'acp:call',
  'acp:list-tools',
  'acp:health',
  // OSF-2 Files/Preview — artifact_id only; isolation via trusted session + store
  'files:preview-by-artifact',
  // OSF-2 session bind — identity set only after membership assertion
  'files:bind-session',
  'files:unbind-session',
  // UI project catalog (not science authority; open binds via membership)
  'files:list-ui-projects',
  'files:create-ui-project',
  'files:open-ui-project',
  'files:delete-ui-project',
  // LS5-K28 Question refinement — actor-gated engine mutation, never local state
  'files:update-question',
  // LS5-K30 Export to a user-chosen path. Writes only where a human picked in
  // the OS save dialog — the renderer never names a destination.
  'files:save-export',
  // OSF-3 Notebook — plan/dry-run/export local; execute only via ACP
  'notebook:plan-cell',
  'notebook:dry-run-cell',
  'notebook:execute-cell',
  'notebook:history',
  'notebook:export-ipynb',
  // OSF-4 Reviewer — artifact-bound only; no path-based orchestration
  'review:plan',
  'review:submit',
  'review:history',
  'review:latest',
  'review:export-dossier',
  // OSF-5 Skills — catalog projections only. ZIP mutation enters through the
  // actor-gated settings import channels above; legacy local import/admit/reject
  // channels are deliberately absent.
  'skills:list',
  'skills:quarantine-list',
  'skills:bulk-admit',
  // OSF-6 Remote Compute — dry-run plan only; no desktop SSH/SCP
  'compute:plan',
  'compute:submit-plan',
  'compute:execute-live',
  'compute:history',
  // OSF-7 Connector catalog — read-only; no desktop fetch runtime
  'connectors:list',
  'connectors:fetch',
  // LS5-K4 Environment identity — discovery and identification return
  // observations (absolute path, sha256, exact version, os/arch, lock digest);
  // request-admission forwards to the SessionActor and returns ITS verdict.
  // None of the three can admit anything on the desktop's own authority.
  'environment:discover',
  'environment:identify',
  'environment:request-admission',
  // OSF-8 / office admission / release honesty
  'office:admission-list',
  'office:preview-open',
  'release:checklist-status',
])

/**
 * Channels that Electron main MUST NEVER register handlers for.
 * These are Open Science execution paths. All science operations
 * go through Rust Lumen SessionActor.
 *
 * Channel names are the EXACT strings from production Open Science IPC
 * files (artifacts/ipc.ts, projects/ipc.ts, reviewer/ipc.ts, etc.),
 * NOT fictional names.
 */
const BANNED_CHANNELS = new Set<string>([
  // ── Artifacts (Electron path-based write/read/verify authority)
  // Original: artifacts/ipc.ts
  'artifacts:finalize-run',
  'artifacts:open-file',
  'artifacts:read-preview',
  'artifacts:list-project-files',
  'artifacts:reconcile-pending',
  // ── Projects (Electron project CRUD authority)
  // Original: projects/ipc.ts
  'projects:create',
  'projects:delete',
  'projects:update',
  'projects:list',
  'projects:get',
  // ── Reviewer (Electron reviewer orchestration)
  // Original: reviewer/ipc.ts
  'reviewer:run',
  'reviewer:get-for-session',
  'reviewer:abort-fix-loop',
  // ── Compute (Electron SSH/SCP/job execution)
  // Original: compute/ipc.ts
  'compute:job-updated',
  // ── Notebook (Electron kernel execution)
  // Original: notebook/ipc.ts
  // ── Preview save/load (Electron path-based preview persistence)
  'preview:load',
  'preview:save',
  'preview:delete',
])

// ── Policy functions ─────────────────────────────────────────────

export function validateIpcChannel(channel: string): boolean {
  if (BANNED_CHANNELS.has(channel)) return false
  return ALLOWED_CHANNELS.has(channel)
}

export function getBannedChannels(): ReadonlySet<string> {
  return BANNED_CHANNELS
}

export function getAllowedChannels(): ReadonlySet<string> {
  return ALLOWED_CHANNELS
}

// ── Artifact preview access control ──────────────────────────────

export interface ArtifactPreviewRequest {
  artifactId: string
  ownerId: string
  projectId: string
  expectedSha256?: string
}

export interface ArtifactPreviewContext {
  ownerId: string
  projectId: string
  digest?: string
}

export interface AccessResult {
  ok: boolean
  reason?: string
}

/**
 * Fail-closed artifact preview access check.
 * Rejects: wrong owner, wrong project, hash mismatch, empty ids.
 * This is the enforcement point that prevents Electron from opening
 * arbitrary file paths for preview — it must go through Rust artifact_id.
 */
export function assertArtifactPreviewAccess(
  req: ArtifactPreviewRequest,
  ctx: ArtifactPreviewContext,
): AccessResult {
  // Reject empty identifiers (no anonymous artifact access)
  if (!req.artifactId || !req.ownerId || !req.projectId) {
    return { ok: false, reason: 'artifact_id, owner_id, and project_id are required' }
  }
  if (!ctx.ownerId || !ctx.projectId) {
    return { ok: false, reason: 'context owner_id and project_id are required' }
  }
  // Owner isolation
  if (req.ownerId !== ctx.ownerId) {
    return { ok: false, reason: `owner mismatch: request=${req.ownerId} context=${ctx.ownerId}` }
  }
  // Project isolation
  if (req.projectId !== ctx.projectId) {
    return { ok: false, reason: `project mismatch: request=${req.projectId} context=${ctx.projectId}` }
  }
  // Optional hash verification
  if (req.expectedSha256 && ctx.digest && req.expectedSha256 !== ctx.digest) {
    return { ok: false, reason: `sha256 mismatch: expected=${req.expectedSha256} actual=${ctx.digest}` }
  }
  return { ok: true }
}
