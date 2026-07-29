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
  sourceSha256: string
  candidateLumenRoutes: string[]
  requiredUpstreamToolCount: number
  parameterCount: number
  riskFlags: string[]
  admissionTrack: string
  /** Catalog default is quarantined; exactly one Biomni tool may be admitted-executable. */
  disposition: 'quarantined' | 'admitted-executable'
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

export type EcosystemSkillInventory = {
  candidates: EcosystemSkillCandidate[]
  total: number
  quarantined: number
  approved: number
  admittedExecutable: number
  stillQuarantined: number
  authority: string
}

export type EcosystemInventoryParse =
  | { ok: true; inventory: EcosystemSkillInventory }
  | { ok: false; reason: string }

/** Sole admitted Biomni executable skill id (1 of 224). */
export const ADMITTED_BIOMNI_UNIPROT_ID = 'ecosystem/biomni/query_uniprot'

/**
 * Treat the main-process response as hostile input.
 *
 * The desktop catalog is discovery metadata plus a single admission overlay.
 * At most one candidate may be `admitted-executable` / `canRunViaLumen`, and it
 * must be the Biomni UniProt capability with fixed Lumen mapping metadata.
 */
export function parseEcosystemSkillInventory(value: unknown): EcosystemInventoryParse {
  if (!value || typeof value !== 'object') {
    return { ok: false, reason: 'Skill inventory returned no object.' }
  }
  const response = value as {
    ok?: boolean
    reason?: unknown
    ecosystem?: {
      candidates?: unknown
      summary?: { total?: unknown; approved?: unknown; quarantined?: unknown }
      authority?: unknown
      honesty?: {
        biomniCatalogTotal?: unknown
        admittedExecutable?: unknown
        stillQuarantined?: unknown
      }
    }
  }
  if (response.ok !== true) {
    return {
      ok: false,
      reason:
        typeof response.reason === 'string'
          ? response.reason
          : 'Skill inventory is unavailable.',
    }
  }

  const ecosystem = response.ecosystem
  const rawCandidates = ecosystem?.candidates
  const authority = String(ecosystem?.authority ?? '')
  if (
    !authority.includes('SessionActor') ||
    !Array.isArray(rawCandidates) ||
    typeof ecosystem?.summary?.total !== 'number' ||
    ecosystem.summary.total !== rawCandidates.length ||
    typeof ecosystem.summary.approved !== 'number' ||
    typeof ecosystem.summary.quarantined !== 'number' ||
    ecosystem.summary.approved + ecosystem.summary.quarantined !== rawCandidates.length
  ) {
    return { ok: false, reason: 'Ecosystem catalog authority or count check failed.' }
  }

  const ids = new Set<string>()
  const candidates: EcosystemSkillCandidate[] = []
  let admittedCount = 0
  for (const raw of rawCandidates) {
    if (!raw || typeof raw !== 'object') {
      return { ok: false, reason: 'Ecosystem catalog contains a malformed candidate.' }
    }
    const item = raw as Record<string, unknown>
    if (
      typeof item.skillId !== 'string' ||
      ids.has(item.skillId) ||
      typeof item.displayName !== 'string' ||
      typeof item.description !== 'string' ||
      typeof item.discipline !== 'string' ||
      ![
        'skill-document',
        'tool-descriptor',
        'data-resource',
        'software-resource',
        'protocol-reference',
        'knowledge-document',
      ].includes(String(item.sourceKind)) ||
      typeof item.sourceRepository !== 'string' ||
      typeof item.exactCommit !== 'string' ||
      !/^[0-9a-f]{40}$/.test(item.exactCommit) ||
      typeof item.sourceSha256 !== 'string' ||
      !/^[0-9a-f]{64}$/.test(item.sourceSha256) ||
      !Array.isArray(item.candidateLumenRoutes) ||
      !item.candidateLumenRoutes.every((route) => typeof route === 'string') ||
      typeof item.requiredUpstreamToolCount !== 'number' ||
      !Number.isInteger(item.requiredUpstreamToolCount) ||
      item.requiredUpstreamToolCount < 0 ||
      typeof item.parameterCount !== 'number' ||
      !Number.isInteger(item.parameterCount) ||
      item.parameterCount < 0 ||
      !Array.isArray(item.riskFlags) ||
      !item.riskFlags.every((flag) => typeof flag === 'string') ||
      typeof item.admissionTrack !== 'string' ||
      (item.disposition !== 'quarantined' && item.disposition !== 'admitted-executable')
    ) {
      return {
        ok: false,
        reason: `Unsafe or malformed ecosystem candidate: ${
          typeof item.skillId === 'string' ? item.skillId : '<missing>'
        }.`,
      }
    }

    const canRun = item.canRunViaLumen === true
    if (item.disposition === 'admitted-executable' || canRun) {
      admittedCount += 1
      if (item.skillId !== ADMITTED_BIOMNI_UNIPROT_ID) {
        return {
          ok: false,
          reason: `Only ${ADMITTED_BIOMNI_UNIPROT_ID} may be admitted-executable.`,
        }
      }
      if (!canRun || item.disposition !== 'admitted-executable') {
        return {
          ok: false,
          reason: 'Admitted capability must set canRunViaLumen and admitted-executable.',
        }
      }
      const runVia = item.runVia as Record<string, unknown> | undefined
      if (
        !runVia ||
        runVia.source !== 'Biomni' ||
        runVia.executor !== 'Rust Lumen SessionActor' ||
        runVia.dataSource !== 'UniProt' ||
        runVia.lumenMethod !== 'x.ai/science/capability_run' ||
        runVia.connectorId !== 'uniprot' ||
        runVia.mode !== 'fixture/offline'
      ) {
        return {
          ok: false,
          reason: 'Admitted UniProt capability missing fixed Lumen runVia metadata.',
        }
      }
    } else if (canRun) {
      return { ok: false, reason: 'Quarantined candidates must not set canRunViaLumen.' }
    }

    ids.add(item.skillId)
    candidates.push({
      skillId: item.skillId,
      displayName: item.displayName as string,
      description: item.description as string,
      discipline: item.discipline as string,
      sourceKind: item.sourceKind as EcosystemSkillCandidate['sourceKind'],
      sourceRepository: item.sourceRepository as string,
      exactCommit: item.exactCommit as string,
      sourceSha256: item.sourceSha256 as string,
      candidateLumenRoutes: item.candidateLumenRoutes as string[],
      requiredUpstreamToolCount: item.requiredUpstreamToolCount as number,
      parameterCount: item.parameterCount as number,
      riskFlags: item.riskFlags as string[],
      admissionTrack: item.admissionTrack as string,
      disposition: item.disposition as EcosystemSkillCandidate['disposition'],
      canRunViaLumen: canRun,
      ...(canRun && item.runVia
        ? { runVia: item.runVia as EcosystemSkillCandidate['runVia'] }
        : {}),
    })
  }

  if (admittedCount > 1) {
    return { ok: false, reason: 'At most one ecosystem capability may be admitted-executable.' }
  }
  if (ecosystem.summary.approved !== admittedCount) {
    return { ok: false, reason: 'Ecosystem approved count does not match admitted candidates.' }
  }
  if (ecosystem.summary.quarantined !== candidates.length - admittedCount) {
    return { ok: false, reason: 'Ecosystem quarantined count does not match candidates.' }
  }

  // Honesty block is optional but if present must match Biomni 224/1/223.
  const honesty = ecosystem.honesty
  if (honesty) {
    if (
      honesty.biomniCatalogTotal !== 224 ||
      honesty.admittedExecutable !== 1 ||
      honesty.stillQuarantined !== 223
    ) {
      return {
        ok: false,
        reason: 'Ecosystem honesty block must report Biomni 224 total / 1 admitted / 223 quarantined.',
      }
    }
  }

  return {
    ok: true,
    inventory: {
      candidates,
      total: candidates.length,
      approved: admittedCount,
      quarantined: candidates.length - admittedCount,
      admittedExecutable: admittedCount,
      stillQuarantined: candidates.length - admittedCount,
      authority,
    },
  }
}

export function filterEcosystemSkills(
  candidates: readonly EcosystemSkillCandidate[],
  query: string,
): EcosystemSkillCandidate[] {
  const terms = query
    .trim()
    .toLocaleLowerCase()
    .split(/\s+/)
    .filter(Boolean)
  if (terms.length === 0) return [...candidates]

  return candidates.filter((candidate) => {
    const searchable = [
      candidate.displayName,
      candidate.description,
      candidate.discipline,
      candidate.skillId,
      candidate.sourceKind,
      candidate.admissionTrack,
      candidate.canRunViaLumen ? 'run via lumen admitted' : 'quarantined',
      ...candidate.riskFlags,
      ...candidate.candidateLumenRoutes,
    ]
      .join(' ')
      .toLocaleLowerCase()
    return terms.every((term) => searchable.includes(term))
  })
}
