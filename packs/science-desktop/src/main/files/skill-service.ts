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

export type EcosystemSkillCandidate = {
  skillId: string
  displayName: string
  description: string
  discipline: string
  sourceKind: 'skill-document' | 'tool-descriptor'
  sourceRepository: string
  exactCommit: string
  sourceSha256: string
  candidateLumenRoutes: string[]
  requiredUpstreamToolCount: number
  parameterCount: number
  riskFlags: string[]
  admissionTrack: string
  disposition: 'quarantined'
}

export type SkillService = {
  listInventory: () => {
    ok: boolean
    reason?: string
    approved: string[]
    pending: string[]
    quarantined: SkillRecord[]
    ecosystem: {
      candidates: EcosystemSkillCandidate[]
      summary: { total: number; approved: 0; quarantined: number }
      authority: 'catalog-only; Rust SessionActor required'
      unavailable?: string
    }
    summary: {
      approved: number
      pending: number
      quarantined: number
      ecosystemQuarantined: number
      total: number
    }
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
  /** Optional path to the read-only, zero-approved ecosystem candidate catalog. */
  ecosystemCatalogPath?: string
  /** Optional complete set of read-only, zero-approved ecosystem catalogs. */
  ecosystemCatalogPaths?: string[]
}): SkillService {
  const quarantine = new Map<string, SkillRecord>()

  function loadRegistry(): {
    approved: string[]
    pending: string[]
    total: number
    /** Set when the registry could not be read. Absent means it was read. */
    unavailable?: string
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
    } catch (e: unknown) {
      // NOT a silent empty. "No skills are registered" and "the registry could
      // not be read" render identically as an empty list, and the second is a
      // broken installation that the user would spend an afternoon on. The
      // reason travels with the answer.
      return {
        approved: [],
        pending: [],
        total: 0,
        unavailable: `skill registry unreadable at ${opts.registryPath}: ${(e as Error).message}`,
      }
    }
  }

  function loadOneEcosystemCatalog(catalogPath: string): {
    candidates: EcosystemSkillCandidate[]
    unavailable?: string
  } {
    try {
      const raw = JSON.parse(fs.readFileSync(catalogPath, 'utf-8')) as {
        schema_version?: number
        source?: {
          id?: string
          repository?: string
          exact_commit?: string
        }
        authority?: {
          runtime_authority?: string
          source_runtime_authority?: string
          catalog_is_executable?: boolean
          direct_scp_hub_calls_admitted?: boolean
          direct_upstream_calls_admitted?: boolean
          bulk_auto_approval?: boolean
        }
        summary?: { total?: number; approved?: number; quarantined?: number }
        skills?: Array<{
          skill_id?: string
          display_name?: string
          description?: string
          discipline?: string
          source_repository?: string
          exact_commit?: string
          source_sha256?: string
          source_kind?: string
          candidate_lumen_routes?: string[]
          required_upstream_tools?: unknown[]
          parameter_contract?: { required?: unknown[]; optional?: unknown[] }
          risk_flags?: unknown[]
          admission_track?: string
          prompt_injection_audit?: { status?: string }
          runtime_permissions?: {
            session_actor_required?: boolean
            may_call_lumen_tools_only?: boolean
            controlled_tools?: unknown[]
            independent_execution_authority?: boolean
            network?: string
            shell?: string
            filesystem?: string
            device?: string
          }
          final_disposition?: string
        }>
      }
      const authority = raw.authority
      const skills = raw.skills ?? []
      const sourceId = raw.source?.id
      const sourceProfile =
        sourceId === 'internscience-scp-skills'
          ? {
              repository: 'https://github.com/InternScience/scp.git',
              sourceKind: 'skill-document' as const,
              network: 'denied-until-per-skill-admission',
            }
          : sourceId === 'snap-stanford-biomni'
            ? {
                repository: 'https://github.com/snap-stanford/Biomni.git',
                sourceKind: 'tool-descriptor' as const,
                network: 'denied-until-per-tool-admission',
              }
            : undefined
      const directCallsDenied =
        (sourceId === 'internscience-scp-skills' &&
          authority?.direct_scp_hub_calls_admitted === false) ||
        (sourceId === 'snap-stanford-biomni' &&
          authority?.direct_upstream_calls_admitted === false)
      if (
        raw.schema_version !== 1 ||
        !sourceProfile ||
        raw.source?.repository !== sourceProfile.repository ||
        !/^[0-9a-f]{40}$/.test(raw.source?.exact_commit ?? '') ||
        authority?.runtime_authority !== 'Rust SessionActor' ||
        authority.source_runtime_authority !== 'none' ||
        authority.catalog_is_executable !== false ||
        !directCallsDenied ||
        authority.bulk_auto_approval !== false ||
        raw.summary?.approved !== 0 ||
        raw.summary?.total !== skills.length ||
        raw.summary?.quarantined !== skills.length
      ) {
        throw new Error('ecosystem catalog authority or summary invariant failed')
      }

      const ids = new Set<string>()
      const candidates = skills.map((skill): EcosystemSkillCandidate => {
        const permissions = skill.runtime_permissions
        if (
          !skill.skill_id ||
          ids.has(skill.skill_id) ||
          !skill.display_name ||
          !skill.description ||
          skill.source_repository !== sourceProfile.repository ||
          skill.exact_commit !== raw.source?.exact_commit ||
          !/^[0-9a-f]{64}$/.test(skill.source_sha256 ?? '') ||
          (sourceProfile.sourceKind === 'tool-descriptor'
            ? skill.source_kind !== 'tool-descriptor' ||
              !Array.isArray(skill.parameter_contract?.required) ||
              !Array.isArray(skill.parameter_contract.optional) ||
              !Array.isArray(skill.risk_flags) ||
              !skill.risk_flags.every((flag) => typeof flag === 'string') ||
              !skill.admission_track
            : skill.source_kind !== undefined) ||
          skill.final_disposition !== 'quarantined' ||
          skill.prompt_injection_audit?.status !== 'pending' ||
          permissions?.session_actor_required !== true ||
          permissions.may_call_lumen_tools_only !== true ||
          permissions.independent_execution_authority !== false ||
          permissions.network !== sourceProfile.network ||
          permissions.shell !== 'denied' ||
          permissions.filesystem !== 'denied' ||
          (sourceProfile.sourceKind === 'tool-descriptor' &&
            permissions.device !== 'denied') ||
          !Array.isArray(permissions.controlled_tools) ||
          permissions.controlled_tools.length !== 0
        ) {
          throw new Error(`unsafe or malformed ecosystem skill: ${skill.skill_id ?? '<missing>'}`)
        }
        ids.add(skill.skill_id)
        return {
          skillId: skill.skill_id,
          displayName: skill.display_name,
          description: skill.description,
          discipline: skill.discipline || 'unclassified',
          sourceKind: sourceProfile.sourceKind,
          sourceRepository: skill.source_repository,
          exactCommit: skill.exact_commit!,
          sourceSha256: skill.source_sha256!,
          candidateLumenRoutes: Array.isArray(skill.candidate_lumen_routes)
            ? skill.candidate_lumen_routes.filter(
                (route): route is string => typeof route === 'string',
              )
            : [],
          requiredUpstreamToolCount: Array.isArray(skill.required_upstream_tools)
            ? skill.required_upstream_tools.length
            : 0,
          parameterCount:
            (Array.isArray(skill.parameter_contract?.required)
              ? skill.parameter_contract.required.length
              : 0) +
            (Array.isArray(skill.parameter_contract?.optional)
              ? skill.parameter_contract.optional.length
              : 0),
          riskFlags: Array.isArray(skill.risk_flags)
            ? skill.risk_flags.filter(
                (flag): flag is string => typeof flag === 'string',
              )
            : [],
          admissionTrack: skill.admission_track || 'per-skill-review',
          disposition: 'quarantined',
        }
      })
      return { candidates }
    } catch (e: unknown) {
      return {
        candidates: [],
        unavailable: `ecosystem skill catalog unreadable at ${catalogPath}: ${(e as Error).message}`,
      }
    }
  }

  function loadEcosystemCatalog(): {
    candidates: EcosystemSkillCandidate[]
    unavailable?: string
  } {
    const paths = [
      ...(opts.ecosystemCatalogPaths ?? []),
      ...(opts.ecosystemCatalogPath ? [opts.ecosystemCatalogPath] : []),
    ].filter((value, index, values) => values.indexOf(value) === index)
    if (paths.length === 0) return { candidates: [] }

    const loaded = paths.map(loadOneEcosystemCatalog)
    const errors = loaded
      .map((catalog) => catalog.unavailable)
      .filter((reason): reason is string => Boolean(reason))
    if (errors.length > 0) {
      return { candidates: [], unavailable: errors.join('; ') }
    }

    const candidates = loaded.flatMap((catalog) => catalog.candidates)
    const ids = new Set<string>()
    for (const candidate of candidates) {
      if (ids.has(candidate.skillId)) {
        return {
          candidates: [],
          unavailable: `ecosystem catalogs repeat candidate id: ${candidate.skillId}`,
        }
      }
      ids.add(candidate.skillId)
    }
    return { candidates }
  }

  return {
    listInventory() {
      const reg = loadRegistry()
      const ecosystem = loadEcosystemCatalog()
      const quarantined = [...quarantine.values()]
      const reasons = [reg.unavailable, ecosystem.unavailable].filter((reason): reason is string =>
        Boolean(reason),
      )
      return {
        ok: reasons.length === 0,
        ...(reasons.length > 0 ? { reason: reasons.join('; ') } : {}),
        approved: reg.approved,
        pending: reg.pending,
        quarantined,
        ecosystem: {
          candidates: ecosystem.candidates,
          summary: {
            total: ecosystem.candidates.length,
            approved: 0,
            quarantined: ecosystem.candidates.length,
          },
          authority: 'catalog-only; Rust SessionActor required',
          ...(ecosystem.unavailable ? { unavailable: ecosystem.unavailable } : {}),
        },
        summary: {
          approved: reg.approved.length,
          pending: reg.pending.length,
          quarantined: quarantined.length,
          ecosystemQuarantined: ecosystem.candidates.length,
          total: reg.total + quarantined.length + ecosystem.candidates.length,
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
