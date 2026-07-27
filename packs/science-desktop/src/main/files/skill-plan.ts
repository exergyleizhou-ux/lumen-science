/**
 * OSF-5 Skills admission — pure module.
 *
 * Import → quarantine → DS-43 field check → human admit/reject.
 * Never bulk auto-approve. Never grant independent execution authority.
 */

import { createHash } from 'node:crypto'
import type { AccessResult } from '../lumen-authority-policy'
import type { TrustedPreviewContext } from './session-identity'

export type SkillDisposition = 'pending' | 'approved' | 'rejected' | 'quarantined'

export type SkillImportRequest = {
  skillId: string
  displayName?: string
  sourcePath?: string
  content: string
  fileLicense?: string
  sourceRepository?: string
  exactCommit?: string
}

export type SkillRecord = {
  skillId: string
  displayName: string
  contentHash: string
  fileLicense: string
  sourceRepository: string
  exactCommit: string
  sourcePath: string
  disposition: SkillDisposition
  quarantinedAt: number
  admittedAt?: number
  rejectionReason?: string
  /** DS-43 field completeness */
  ds43: {
    hasSourceRepo: boolean
    hasExactCommit: boolean
    hasSourcePath: boolean
    hasSha256: boolean
    hasLicense: boolean
    promptInjectionPass: boolean
    runtimePermissionsReviewed: boolean
    complete: boolean
  }
}

export type SkillAdmitRequest = {
  skillId: string
  reviewer: string
  promptInjectionPass: boolean
  runtimePermissionsReviewed: boolean
  /** Must be true to admit — no silent defaults */
  explicitApprove: boolean
}

const DENIED_LICENSES = new Set(['gpl', 'gpl-2.0', 'gpl-3.0', 'agpl', 'agpl-3.0', 'unknown', ''])

export function hashSkillContent(content: string): string {
  return createHash('sha256').update(content, 'utf8').digest('hex')
}

export function planSkillImport(
  req: SkillImportRequest,
): SkillRecord | { ok: false; reason: string } {
  if (!req.skillId || !req.skillId.trim()) {
    return { ok: false, reason: 'skill_id is required' }
  }
  if (!req.content || !req.content.trim()) {
    return { ok: false, reason: 'skill content is required' }
  }
  if (req.content.length > 2_000_000) {
    return { ok: false, reason: 'skill content exceeds 2MB cap' }
  }
  // Detect obvious shell bridges
  if (/\b(os\.system|subprocess\.|child_process|execSync|rm\s+-rf)\b/i.test(req.content)) {
    return { ok: false, reason: 'skill content contains denied execution patterns' }
  }

  const license = (req.fileLicense || 'unknown').toLowerCase().trim()
  if (DENIED_LICENSES.has(license) || license.includes('gpl')) {
    return { ok: false, reason: `license denied: ${license}` }
  }

  const contentHash = hashSkillContent(req.content)
  const sourceRepo = req.sourceRepository || ''
  const exactCommit = req.exactCommit || ''
  const sourcePath = req.sourcePath || ''

  const ds43 = {
    hasSourceRepo: Boolean(sourceRepo),
    hasExactCommit: Boolean(exactCommit),
    hasSourcePath: Boolean(sourcePath),
    hasSha256: contentHash.length === 64,
    hasLicense: Boolean(req.fileLicense) && !DENIED_LICENSES.has(license),
    promptInjectionPass: false, // requires human review
    runtimePermissionsReviewed: false,
    complete: false,
  }
  ds43.complete = false // never complete on import

  return {
    skillId: req.skillId.trim(),
    displayName: req.displayName || req.skillId,
    contentHash,
    fileLicense: req.fileLicense || 'unknown',
    sourceRepository: sourceRepo,
    exactCommit,
    sourcePath,
    disposition: 'quarantined',
    quarantinedAt: Date.now(),
    ds43,
  }
}

/**
 * Admit only with explicitApprove + DS-43 fields + injection/runtime review.
 * Bulk auto-approve is impossible: each call is one skill.
 */
export function planSkillAdmit(
  record: SkillRecord,
  req: SkillAdmitRequest,
): AccessResult & { record?: SkillRecord } {
  if (record.skillId !== req.skillId) {
    return { ok: false, reason: 'skill_id mismatch' }
  }
  if (record.disposition === 'approved') {
    return { ok: false, reason: 'already approved' }
  }
  if (record.disposition === 'rejected') {
    return { ok: false, reason: 'skill was rejected; re-import required' }
  }
  if (!req.explicitApprove) {
    return { ok: false, reason: 'explicitApprove must be true — no auto-approve' }
  }
  if (!req.reviewer || !req.reviewer.trim()) {
    return { ok: false, reason: 'reviewer identity required' }
  }
  if (!req.promptInjectionPass) {
    return { ok: false, reason: 'prompt_injection_audit must pass' }
  }
  if (!req.runtimePermissionsReviewed) {
    return { ok: false, reason: 'runtime_permissions must be reviewed' }
  }
  // DS-43 required fields
  if (!record.ds43.hasSourceRepo || !record.ds43.hasExactCommit || !record.ds43.hasSourcePath) {
    return {
      ok: false,
      reason: 'DS-43 incomplete: source_repository, exact_commit, source_path required',
    }
  }
  if (!record.ds43.hasLicense || !record.ds43.hasSha256) {
    return { ok: false, reason: 'DS-43 incomplete: license and sha256 required' }
  }

  const admitted: SkillRecord = {
    ...record,
    disposition: 'approved',
    admittedAt: Date.now(),
    ds43: {
      ...record.ds43,
      promptInjectionPass: true,
      runtimePermissionsReviewed: true,
      complete: true,
    },
  }
  return { ok: true, record: admitted }
}

export function assertSkillSession(
  trusted: TrustedPreviewContext | null,
): AccessResult {
  if (!trusted?.ownerId || !trusted?.projectId) {
    return {
      ok: false,
      reason: 'no trusted session — open a project before skill import/admit',
    }
  }
  return { ok: true }
}

/** Hard reject bulk auto-approve of multiple skill IDs */
export function rejectBulkAutoApprove(skillIds: string[]): AccessResult {
  if (skillIds.length > 1) {
    return {
      ok: false,
      reason: `bulk auto-approve of ${skillIds.length} skills is denied — admit one at a time`,
    }
  }
  return { ok: true }
}
