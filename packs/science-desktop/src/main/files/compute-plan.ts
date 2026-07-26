/**
 * OSF-6 Remote Compute plan — pure module (no SSH/SCP spawn).
 *
 * LocalProcess fixture → SSH fixture → authorized SSH (live) progression.
 * Generic remote shell is always denied. Live schedule requires explicit
 * operator authorization flag (still only plans ACP; does not spawn SSH).
 */

import { createHash, randomUUID } from 'node:crypto'
import type { AccessResult } from '../lumen-authority-policy'
import type { TrustedPreviewContext } from './session-identity'

export type ComputeTargetKind = 'local_process' | 'ssh_fixture' | 'ssh_authorized' | 'slurm_fixture'

export type ComputePlanRequest = {
  hostname: string
  targetKind?: ComputeTargetKind
  command?: string
  nodes?: number
  cpus?: number
  memoryGb?: number
  gpuCount?: number
  walltimeSecs?: number
  /** Live schedule intent — still does not execute without ACP + auth */
  requestLive?: boolean
  /** Operator authorization token present (opaque; presence only) */
  operatorAuthorized?: boolean
}

export type PlannedJob = {
  command: string
  nodes: number
  cpus: number
  memoryGb: number
  gpuCount: number
  walltimeSecs: number
}

export type ComputePlan = {
  planId: string
  planHash: string
  clusterId: string
  hostname: string
  scheduler: string
  targetKind: ComputeTargetKind
  jobs: PlannedJob[]
  /** Always false for generic shell; true only for fixture paths or authorized live after gates */
  canSchedule: boolean
  dryRun: true
  authority: 'SessionActor/ToolAdapter'
  tool: 'compute_plan'
  notes: string[]
  createdAt: number
  /** Denied if generic shell or unauthorized live */
  denied?: boolean
  denyReason?: string
}

const GENERIC_SHELL_RE =
  /\b(bash|sh|zsh|powershell|cmd\.exe)\b.*(-c|\/c)\b|;\s*(rm|curl|wget)\b/i

export function hashComputePlanPayload(parts: string[]): string {
  return createHash('sha256').update(parts.join('|'), 'utf8').digest('hex')
}

export function planRemoteCompute(
  req: ComputePlanRequest,
): ComputePlan | { ok: false; reason: string } {
  const hostname = (req.hostname || '').trim()
  if (!hostname) {
    return { ok: false, reason: 'hostname is required' }
  }
  // Block wildcard / path-like host abuse
  if (hostname.includes('/') || hostname.includes(' ') || hostname === '*') {
    return { ok: false, reason: 'invalid hostname' }
  }

  const targetKind: ComputeTargetKind = req.targetKind || 'ssh_fixture'
  const command =
    (req.command || 'lumen-science pipeline offline ...').trim() ||
    'lumen-science pipeline offline ...'

  if (GENERIC_SHELL_RE.test(command) || command === 'shell' || command === '/bin/sh') {
    return {
      ok: false,
      reason: 'generic remote shell is denied — use admitted compute plan commands only',
    }
  }

  const nodes = clamp(req.nodes ?? 1, 1, 64)
  const cpus = clamp(req.cpus ?? 4, 1, 256)
  const memoryGb = clamp(req.memoryGb ?? 8, 1, 1024)
  const gpuCount = clamp(req.gpuCount ?? 0, 0, 16)
  const walltimeSecs = clamp(req.walltimeSecs ?? 7200, 60, 86400 * 7)

  const jobs: PlannedJob[] = [
    {
      command,
      nodes,
      cpus,
      memoryGb,
      gpuCount,
      walltimeSecs,
    },
  ]

  const planId = randomUUID()
  const planHash = hashComputePlanPayload([
    planId,
    hostname,
    targetKind,
    command,
    String(nodes),
    String(cpus),
    String(memoryGb),
    String(gpuCount),
    String(walltimeSecs),
  ])

  const notes: string[] = [
    'Dry-run plan only — no live HPC credentials used in desktop.',
    'Require RemoteCompute feature gate + operator authorization for live scheduling.',
    `targetKind=${targetKind}`,
  ]

  let canSchedule = false
  let denied = false
  let denyReason: string | undefined

  if (targetKind === 'local_process' || targetKind === 'ssh_fixture' || targetKind === 'slurm_fixture') {
    // Fixture paths: plan is valid, cannot live schedule
    canSchedule = false
    notes.push('Fixture path: can_schedule=false until authorized live path.')
  } else if (targetKind === 'ssh_authorized') {
    if (!req.operatorAuthorized) {
      denied = true
      denyReason = 'ssh_authorized requires operatorAuthorized=true'
      canSchedule = false
    } else if (req.requestLive) {
      // Still dry-run at desktop: live only via ACP after plan hash bound permission
      canSchedule = false
      notes.push(
        'Authorized live intent recorded — execute only via SessionActor ToolAdapter with plan hash permission.',
      )
    } else {
      canSchedule = false
      notes.push('Authorized host cataloged; requestLive not set — plan only.')
    }
  }

  const scheduler =
    targetKind === 'slurm_fixture' || targetKind === 'ssh_authorized' ? 'slurm' : 'local'

  if (denied) {
    return {
      ok: false,
      reason: denyReason || 'compute plan denied',
    }
  }

  return {
    planId,
    planHash,
    clusterId: targetKind === 'local_process' ? 'local' : 'sim-cluster',
    hostname,
    scheduler,
    targetKind,
    jobs,
    canSchedule,
    dryRun: true,
    authority: 'SessionActor/ToolAdapter',
    tool: 'compute_plan',
    notes,
    createdAt: Date.now(),
  }
}

export function assertComputePlanAccess(
  plan: ComputePlan,
  trusted: TrustedPreviewContext | null,
): AccessResult {
  if (!trusted?.ownerId || !trusted?.projectId) {
    return {
      ok: false,
      reason: 'no trusted session — open a project before compute plan',
    }
  }
  if (plan.authority !== 'SessionActor/ToolAdapter') {
    return { ok: false, reason: 'invalid authority claim' }
  }
  if (!plan.dryRun) {
    return { ok: false, reason: 'desktop compute plans must be dry-run' }
  }
  if (plan.canSchedule) {
    // Desktop must never claim canSchedule true without live path — fail closed
    return { ok: false, reason: 'desktop cannot set canSchedule=true' }
  }
  return { ok: true }
}

/**
 * Live execute intent: always rejected at desktop layer (no SSH spawn).
 * Caller must use ACP after plan hash admission.
 */
export function rejectDesktopLiveExecute(): AccessResult {
  return {
    ok: false,
    reason:
      'desktop live SSH/SCP/Slurm execution is denied — use ACP compute_plan + SessionActor ToolAdapter',
  }
}

function clamp(n: number, min: number, max: number): number {
  if (!Number.isFinite(n)) return min
  return Math.min(max, Math.max(min, Math.floor(n)))
}
