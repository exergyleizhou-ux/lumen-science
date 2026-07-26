/**
 * OSF-5 Skills product service.
 *
 * Reads Lumen registry (approved/pending inventory) + local quarantine.
 * Import never approves. Admit is single-skill, DS-43 gated.
 */

import fs from 'node:fs'
import path from 'node:path'
import {
  planSkillImport,
  planSkillAdmit,
  assertSkillSession,
  rejectBulkAutoApprove,
  type SkillImportRequest,
  type SkillAdmitRequest,
  type SkillRecord,
} from './skill-plan'
import { getTrustedPreviewContext } from './session-identity'

export type LumenRegistrySkill = {
  skill_id: string
  display_name?: string
  final_disposition?: string
  file_license?: string
  source_sha256?: string
}

export type SkillService = {
  listInventory: () => {
    approved: string[]
    pending: string[]
    quarantined: SkillRecord[]
    summary: { approved: number; pending: number; quarantined: number; total: number }
  }
  import: (req: SkillImportRequest) => unknown
  admit: (req: SkillAdmitRequest) => unknown
  reject: (skillId: string, reason: string) => unknown
  /** Explicitly reject bulk approve attempts */
  bulkAdmit: (skillIds: string[]) => unknown
  quarantineList: () => SkillRecord[]
}

export function createSkillService(opts: {
  /** Path to packs/science/skills/registry.json */
  registryPath: string
}): SkillService {
  const quarantine = new Map<string, SkillRecord>()

  function loadRegistry(): {
    approved: string[]
    pending: string[]
    total: number
  } {
    try {
      const raw = JSON.parse(fs.readFileSync(opts.registryPath, 'utf-8')) as {
        skills?: LumenRegistrySkill[]
        summary?: { approved?: number; pending?: number; total?: number }
      }
      const skills = raw.skills ?? []
      const approved = skills
        .filter((s) => s.final_disposition === 'approved')
        .map((s) => s.skill_id)
      // pending includes pending-* and missing disposition (not approved/rejected)
      const pending = skills
        .filter((s) => {
          const d = s.final_disposition || 'pending'
          return d !== 'approved' && d !== 'rejected'
        })
        .map((s) => s.skill_id)
      return {
        approved,
        pending,
        total: raw.summary?.total ?? skills.length,
      }
    } catch {
      return { approved: [], pending: [], total: 0 }
    }
  }

  return {
    listInventory() {
      const reg = loadRegistry()
      const quarantined = [...quarantine.values()]
      return {
        approved: reg.approved,
        pending: reg.pending,
        quarantined,
        summary: {
          approved: reg.approved.length,
          pending: reg.pending.length,
          quarantined: quarantined.length,
          total: reg.total + quarantined.length,
        },
      }
    },

    import(req) {
      const session = assertSkillSession(getTrustedPreviewContext())
      if (!session.ok) return { ok: false, reason: session.reason }

      const planned = planSkillImport(req)
      if ('ok' in planned && planned.ok === false) return planned

      const record = planned as SkillRecord
      // Never auto-approve on import
      record.disposition = 'quarantined'
      quarantine.set(record.skillId, record)
      return {
        ok: true,
        record,
        disposition: 'quarantined',
        note: 'imported to quarantine — admit requires DS-43 + explicitApprove',
      }
    },

    admit(req) {
      const session = assertSkillSession(getTrustedPreviewContext())
      if (!session.ok) return { ok: false, reason: session.reason }

      const record = quarantine.get(req.skillId)
      if (!record) {
        return {
          ok: false,
          reason: `skill ${req.skillId} not in quarantine — import first`,
        }
      }

      const result = planSkillAdmit(record, req)
      if (!result.ok || !result.record) {
        return { ok: false, reason: result.reason }
      }
      quarantine.set(req.skillId, result.record)
      return {
        ok: true,
        record: result.record,
        disposition: 'approved',
        note: 'admitted after DS-43 — does not mutate packs/science/skills/registry.json in-process (ledger update is release process)',
      }
    },

    reject(skillId, reason) {
      const session = assertSkillSession(getTrustedPreviewContext())
      if (!session.ok) return { ok: false, reason: session.reason }

      const record = quarantine.get(skillId)
      if (!record) {
        return { ok: false, reason: `skill ${skillId} not in quarantine` }
      }
      const rejected: SkillRecord = {
        ...record,
        disposition: 'rejected',
        rejectionReason: reason || 'rejected',
      }
      quarantine.set(skillId, rejected)
      return { ok: true, record: rejected }
    },

    bulkAdmit(skillIds) {
      const blocked = rejectBulkAutoApprove(skillIds)
      if (!blocked.ok) return blocked
      if (skillIds.length === 0) {
        return { ok: false, reason: 'empty skill id list' }
      }
      // Single id still goes through admit with explicit flags required by caller
      return {
        ok: false,
        reason: 'use skills:admit with explicitApprove for a single skill_id',
      }
    },

    quarantineList() {
      return [...quarantine.values()]
    },
  }
}

export function defaultRegistryPath(repoRoot?: string): string {
  const root = repoRoot || path.resolve(process.cwd(), '../..')
  return path.join(root, 'packs/science/skills/registry.json')
}
