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
import type { TrustedPreviewContext } from './session-identity'

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
  sourceKind:
    | 'skill-document'
    | 'tool-descriptor'
    | 'data-resource'
    | 'software-resource'
    | 'protocol-reference'
    | 'knowledge-document'
  sourceRepository: string
  exactCommit: string
  sourcePath: string
  sourceSha256: string
  fileLicense: string
  candidateLumenRoutes: string[]
  requiredUpstreamToolCount: number
  parameterCount: number
  riskFlags: string[]
  admissionTrack: string
  disposition: 'quarantined' | 'admitted-executable'
  /**
   * Product may show "Run via Lumen" only when true.
   * Today: only ecosystem/biomni/query_uniprot after admission overlay.
   */
  canRunViaLumen: boolean
  runVia?: {
    source: 'Biomni'
    executor: 'Rust Lumen SessionActor'
    dataSource: 'UniProt'
    lumenMethod: 'x.ai/science/capability_run'
    connectorId: 'uniprot'
    mode: 'fixture/offline'
  }
}

/** Sole admitted Biomni executable capability (1 of 224). */
export const ADMITTED_BIOMNI_CAPABILITY_ID = 'ecosystem/biomni/query_uniprot'

export type SkillService = {
  listInventory: () => {
    ok: boolean
    reason?: string
    approved: string[]
    pending: string[]
    quarantined: SkillRecord[]
    ecosystem: {
      candidates: EcosystemSkillCandidate[]
      summary: { total: number; approved: number; quarantined: number }
      authority: string
      honesty?: {
        biomniCatalogTotal: number
        admittedExecutable: number
        stillQuarantined: number
        claimForbidden: string
      }
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
  import: (req: SkillImportRequest, trusted: TrustedPreviewContext | null) => unknown
  admit: (req: SkillAdmitRequest, trusted: TrustedPreviewContext | null) => unknown
  reject: (
    skillId: string,
    reason: string,
    trusted: TrustedPreviewContext | null,
  ) => unknown
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
  /**
   * Machine-backed admission dossier. Optional at the API boundary so older
   * callers fail closed; without a valid dossier no ecosystem candidate is
   * executable.
   */
  admissionPath?: string
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
          catalog_kind?: string
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
          source_path?: string
          file_license?: string
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
              sourceKinds: ['skill-document'] as const,
              network: 'denied-until-per-skill-admission',
              extendedMetadata: false,
            }
          : sourceId === 'snap-stanford-biomni' &&
              raw.source?.catalog_kind === 'tool-descriptors'
            ? {
                repository: 'https://github.com/snap-stanford/Biomni.git',
                sourceKinds: ['tool-descriptor'] as const,
                network: 'denied-until-per-tool-admission',
                extendedMetadata: true,
              }
            : sourceId === 'snap-stanford-biomni' &&
                raw.source?.catalog_kind === 'resource-inventory'
              ? {
                  repository: 'https://github.com/snap-stanford/Biomni.git',
                  sourceKinds: [
                    'data-resource',
                    'software-resource',
                    'protocol-reference',
                    'knowledge-document',
                  ] as const,
                  network: 'denied-until-per-resource-admission',
                  extendedMetadata: true,
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
        const sourceKind = skill.source_kind ?? 'skill-document'
        if (
          !skill.skill_id ||
          ids.has(skill.skill_id) ||
          !skill.display_name ||
          !skill.description ||
          skill.source_repository !== sourceProfile.repository ||
          skill.exact_commit !== raw.source?.exact_commit ||
          !/^[0-9a-f]{64}$/.test(skill.source_sha256 ?? '') ||
          !(sourceProfile.sourceKinds as readonly string[]).includes(sourceKind) ||
          (sourceProfile.extendedMetadata
            ? !skill.source_kind ||
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
          (sourceProfile.extendedMetadata &&
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
          sourceKind: sourceKind as EcosystemSkillCandidate['sourceKind'],
          sourceRepository: skill.source_repository,
          exactCommit: skill.exact_commit!,
          sourcePath: skill.source_path ?? '',
          sourceSha256: skill.source_sha256!,
          fileLicense: skill.file_license ?? '',
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
          // Admission is applied only after every catalog has validated and the
          // machine-readable dossier has matched the source row exactly.
          disposition: 'quarantined',
          canRunViaLumen: false,
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
    const admission = loadAdmission(candidates)
    if (!admission.ok) {
      return { candidates, unavailable: admission.reason }
    }
    const admitted = candidates.find(
      (candidate) => candidate.skillId === admission.capabilityId,
    )
    if (!admitted) {
      return {
        candidates,
        unavailable: `ecosystem admission candidate missing after validation: ${admission.capabilityId}`,
      }
    }
    admitted.disposition = 'admitted-executable'
    admitted.canRunViaLumen = true
    admitted.runVia = {
      source: 'Biomni',
      executor: 'Rust Lumen SessionActor',
      dataSource: 'UniProt',
      lumenMethod: 'x.ai/science/capability_run',
      connectorId: 'uniprot',
      mode: 'fixture/offline',
    }
    return { candidates }
  }

  function loadAdmission(
    candidates: EcosystemSkillCandidate[],
  ): { ok: true; capabilityId: string } | { ok: false; reason: string } {
    if (!opts.admissionPath) {
      return {
        ok: false,
        reason: 'ecosystem admission unavailable: no admissionPath configured',
      }
    }
    try {
      const raw = JSON.parse(fs.readFileSync(opts.admissionPath, 'utf-8')) as {
        schema_version?: number
        biomni_catalog?: {
          path?: string
          total?: number
          admitted_executable?: number
          still_quarantined?: number
        }
        capability?: {
          id?: string
          display_name?: string
          source?: {
            repository?: string
            exact_commit?: string
            source_path?: string
            source_sha256?: string
            license?: string
            reuse_mode?: string
          }
          mapping?: {
            lumen_method?: string
            connector_id?: string
            prompt_maps_to?: string
            controlled_tools?: unknown[]
          }
          status?: string
        }
      }
      const biomni = candidates.filter(
        (candidate) =>
          candidate.sourceRepository === 'https://github.com/snap-stanford/Biomni.git' &&
          candidate.sourceKind === 'tool-descriptor',
      )
      const capability = raw.capability
      const source = capability?.source
      const mapping = capability?.mapping
      const candidate = candidates.find(
        (item) => item.skillId === capability?.id,
      )
      const controlledTools = mapping?.controlled_tools
      if (
        raw.schema_version !== 1 ||
        raw.biomni_catalog?.path !==
          'packs/science/skills/ecosystem/biomni-tool-catalog.json' ||
        raw.biomni_catalog.total !== biomni.length ||
        raw.biomni_catalog.total !== 224 ||
        raw.biomni_catalog.admitted_executable !== 1 ||
        raw.biomni_catalog.still_quarantined !== biomni.length - 1 ||
        capability?.id !== ADMITTED_BIOMNI_CAPABILITY_ID ||
        capability.display_name !== candidate?.displayName ||
        source?.repository !== candidate?.sourceRepository ||
        source?.exact_commit !== candidate?.exactCommit ||
        source?.source_path !== candidate?.sourcePath ||
        source?.source_sha256 !== candidate?.sourceSha256 ||
        source?.license !== candidate?.fileLicense ||
        source?.reuse_mode !== 'adapted-capability-mapping' ||
        mapping?.lumen_method !== 'x.ai/science/connector_fetch' ||
        mapping?.connector_id !== 'uniprot' ||
        mapping?.prompt_maps_to !== 'query' ||
        !Array.isArray(controlledTools) ||
        controlledTools.length !== 1 ||
        controlledTools[0] !== 'x.ai/science/connector_fetch' ||
        capability.status !== 'admitted-executable'
      ) {
        throw new Error(
          'Biomni admission does not exactly match catalog source, mapping, and counts',
        )
      }
      return { ok: true, capabilityId: capability.id }
    } catch (e: unknown) {
      return {
        ok: false,
        reason: `ecosystem admission unreadable or mismatched at ${opts.admissionPath}: ${(e as Error).message}`,
      }
    }
  }

  // Read catalog + admission once. A later pathname replacement cannot change
  // which capability this service instance exposes.
  let cachedEcosystem:
    | { candidates: EcosystemSkillCandidate[]; unavailable?: string }
    | undefined
  function cachedEcosystemCatalog(): {
    candidates: EcosystemSkillCandidate[]
    unavailable?: string
  } {
    cachedEcosystem ??= loadEcosystemCatalog()
    return cachedEcosystem
  }

  return {
    listInventory() {
      const reg = loadRegistry()
      const ecosystem = cachedEcosystemCatalog()
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
            // Exactly one Biomni tool is admitted-executable (query_uniprot).
            approved: ecosystem.candidates.filter((c) => c.canRunViaLumen).length,
            quarantined: ecosystem.candidates.filter((c) => !c.canRunViaLumen).length,
          },
          authority:
            'catalog + admission overlay; only admitted capabilities run via SessionActor',
          honesty: {
            biomniCatalogTotal: ecosystem.candidates.filter(
              (candidate) =>
                candidate.sourceRepository ===
                  'https://github.com/snap-stanford/Biomni.git' &&
                candidate.sourceKind === 'tool-descriptor',
            ).length,
            admittedExecutable: ecosystem.candidates.filter(
              (candidate) =>
                candidate.sourceRepository ===
                  'https://github.com/snap-stanford/Biomni.git' &&
                candidate.sourceKind === 'tool-descriptor' &&
                candidate.canRunViaLumen,
            ).length,
            stillQuarantined: ecosystem.candidates.filter(
              (candidate) =>
                candidate.sourceRepository ===
                  'https://github.com/snap-stanford/Biomni.git' &&
                candidate.sourceKind === 'tool-descriptor' &&
                !candidate.canRunViaLumen,
            ).length,
            claimForbidden: 'Biomni is not fully integrated',
          },
          ...(ecosystem.unavailable ? { unavailable: ecosystem.unavailable } : {}),
        },
        summary: {
          approved: reg.approved.length,
          pending: reg.pending.length,
          quarantined: quarantined.length,
          ecosystemQuarantined: ecosystem.candidates.filter(
            (candidate) => !candidate.canRunViaLumen,
          ).length,
          total: reg.total + quarantined.length + ecosystem.candidates.length,
        },
      }
    },

    import(req, trusted) {
      const session = assertSkillSession(trusted)
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

    admit(req, trusted) {
      const session = assertSkillSession(trusted)
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

    reject(skillId, reason, trusted) {
      const session = assertSkillSession(trusted)
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
