import { describe, expect, it } from 'vitest'
import {
  filterEcosystemSkills,
  parseEcosystemSkillInventory,
  type EcosystemSkillCandidate,
} from './ecosystem-skills'

const candidate: EcosystemSkillCandidate = {
  skillId: 'ecosystem/scp/clinical_trial_search',
  displayName: 'clinical_trial_search',
  description: 'Find registered clinical trials for a disease.',
  discipline: 'Clinical Research',
  sourceRepository: 'https://github.com/InternScience/scp.git',
  exactCommit: 'a'.repeat(40),
  sourceSha256: 'b'.repeat(64),
  candidateLumenRoutes: ['candidate-connector:clinicaltrials-gov'],
  requiredUpstreamToolCount: 1,
  disposition: 'quarantined',
}

function response(overrides: Record<string, unknown> = {}): unknown {
  return {
    ok: true,
    ecosystem: {
      candidates: [candidate],
      summary: { total: 1, approved: 0, quarantined: 1 },
      authority: 'catalog-only; Rust SessionActor required',
    },
    ...overrides,
  }
}

describe('ecosystem skill catalog', () => {
  it('accepts a zero-approved SessionActor-only catalog', () => {
    const parsed = parseEcosystemSkillInventory(response())
    expect(parsed.ok).toBe(true)
    if (parsed.ok) {
      expect(parsed.inventory.total).toBe(1)
      expect(parsed.inventory.candidates[0]).toEqual(candidate)
    }
  })

  it('rejects a catalog that claims an approved candidate', () => {
    const parsed = parseEcosystemSkillInventory({
      ok: true,
      ecosystem: {
        candidates: [{ ...candidate, disposition: 'approved' }],
        summary: { total: 1, approved: 1, quarantined: 0 },
        authority: 'catalog-only; Rust SessionActor required',
      },
    })
    expect(parsed).toEqual({
      ok: false,
      reason: 'Ecosystem catalog authority or count check failed.',
    })
  })

  it('rejects duplicate ids and malformed provenance hashes', () => {
    const duplicate = { ...candidate, sourceSha256: 'not-a-sha' }
    const parsed = parseEcosystemSkillInventory({
      ok: true,
      ecosystem: {
        candidates: [candidate, duplicate],
        summary: { total: 2, approved: 0, quarantined: 2 },
        authority: 'catalog-only; Rust SessionActor required',
      },
    })
    expect(parsed.ok).toBe(false)
  })

  it('searches name, discipline, description, and candidate Lumen routes', () => {
    expect(filterEcosystemSkills([candidate], 'clinical trial')).toEqual([candidate])
    expect(filterEcosystemSkills([candidate], 'clinicaltrials-gov')).toEqual([candidate])
    expect(filterEcosystemSkills([candidate], 'genomics')).toEqual([])
  })
})
