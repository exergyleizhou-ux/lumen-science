export type EcosystemSkillCandidate = {
  skillId: string
  displayName: string
  description: string
  discipline: string
  sourceRepository: string
  exactCommit: string
  sourceSha256: string
  candidateLumenRoutes: string[]
  requiredUpstreamToolCount: number
  disposition: 'quarantined'
}

export type EcosystemSkillInventory = {
  candidates: EcosystemSkillCandidate[]
  total: number
  quarantined: number
  approved: 0
  authority: 'catalog-only; Rust SessionActor required'
}

export type EcosystemInventoryParse =
  | { ok: true; inventory: EcosystemSkillInventory }
  | { ok: false; reason: string }

/**
 * Treat the main-process response as hostile input.
 *
 * The desktop catalog is discovery metadata, not an execution registry. The
 * renderer refuses the whole catalog if any item claims approval or if the
 * SessionActor-only authority marker is missing.
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
  if (
    ecosystem?.authority !== 'catalog-only; Rust SessionActor required' ||
    !Array.isArray(rawCandidates) ||
    ecosystem.summary?.approved !== 0 ||
    ecosystem.summary.total !== rawCandidates.length ||
    ecosystem.summary.quarantined !== rawCandidates.length
  ) {
    return { ok: false, reason: 'Ecosystem catalog authority or count check failed.' }
  }

  const ids = new Set<string>()
  const candidates: EcosystemSkillCandidate[] = []
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
      item.disposition !== 'quarantined'
    ) {
      return {
        ok: false,
        reason: `Unsafe or malformed ecosystem candidate: ${
          typeof item.skillId === 'string' ? item.skillId : '<missing>'
        }.`,
      }
    }
    ids.add(item.skillId)
    candidates.push(item as EcosystemSkillCandidate)
  }

  return {
    ok: true,
    inventory: {
      candidates,
      total: candidates.length,
      approved: 0,
      quarantined: candidates.length,
      authority: 'catalog-only; Rust SessionActor required',
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
      ...candidate.candidateLumenRoutes,
    ]
      .join(' ')
      .toLocaleLowerCase()
    return terms.every((term) => searchable.includes(term))
  })
}
