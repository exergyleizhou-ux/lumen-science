import { describe, expect, it } from 'vitest'
import {
  ADMITTED_BIOMNI_UNIPROT_ID,
  filterEcosystemSkills,
  parseEcosystemSkillInventory,
  type EcosystemSkillCandidate,
} from './ecosystem-skills'

const candidate: EcosystemSkillCandidate = {
  skillId: 'ecosystem/scp/clinical_trial_search',
  displayName: 'clinical_trial_search',
  description: 'Find registered clinical trials for a disease.',
  discipline: 'Clinical Research',
  sourceKind: 'skill-document',
  sourceRepository: 'https://github.com/InternScience/scp.git',
  exactCommit: 'a'.repeat(40),
  sourceSha256: 'b'.repeat(64),
  candidateLumenRoutes: ['candidate-connector:clinicaltrials-gov'],
  requiredUpstreamToolCount: 1,
  parameterCount: 0,
  riskFlags: ['network-or-download'],
  admissionTrack: 'new-lumen-connector',
  disposition: 'quarantined',
  canRunViaLumen: false,
}

const admittedUniprot: EcosystemSkillCandidate = {
  skillId: ADMITTED_BIOMNI_UNIPROT_ID,
  displayName: 'query_uniprot',
  description: 'Query UniProt via Lumen connector_fetch.',
  discipline: 'Database',
  sourceKind: 'tool-descriptor',
  sourceRepository: 'https://github.com/snap-stanford/Biomni.git',
  exactCommit: '400c1f366b96a35ca253e13c9b06c5076af41d65',
  sourceSha256: '875473dc5473cf4f7615c2b4fd886f543ca8a295f7c58eca00fdceb22d2883b6',
  candidateLumenRoutes: ['x.ai/science/connector_fetch:uniprot'],
  requiredUpstreamToolCount: 0,
  parameterCount: 2,
  riskFlags: ['network-or-download'],
  admissionTrack: 'map-to-existing-lumen-connector',
  disposition: 'admitted-executable',
  canRunViaLumen: true,
  runVia: {
    source: 'Biomni',
    executor: 'Rust Lumen SessionActor',
    dataSource: 'UniProt',
    lumenMethod: 'x.ai/science/capability_run',
    connectorId: 'uniprot',
    mode: 'fixture/offline',
  },
}

function response(overrides: Record<string, unknown> = {}): unknown {
  return {
    ok: true,
    ecosystem: {
      candidates: [candidate],
      summary: { total: 1, approved: 0, quarantined: 1 },
      authority: 'catalog + admission overlay; only admitted capabilities run via SessionActor',
      honesty: {
        biomniCatalogTotal: 224,
        admittedExecutable: 1,
        stillQuarantined: 223,
        claimForbidden: 'Biomni is not fully integrated',
      },
    },
    ...overrides,
  }
}

describe('ecosystem skill catalog', () => {
  it('accepts a SessionActor catalog with zero executable candidates', () => {
    const parsed = parseEcosystemSkillInventory(response())
    expect(parsed.ok).toBe(true)
    if (parsed.ok) {
      expect(parsed.inventory.total).toBe(1)
      expect(parsed.inventory.admittedExecutable).toBe(0)
      expect(parsed.inventory.candidates[0].canRunViaLumen).toBe(false)
    }
  })

  it('accepts exactly one admitted Biomni UniProt capability with fixed runVia', () => {
    const parsed = parseEcosystemSkillInventory({
      ok: true,
      ecosystem: {
        candidates: [candidate, admittedUniprot],
        summary: { total: 2, approved: 1, quarantined: 1 },
        authority: 'catalog + admission overlay; only admitted capabilities run via SessionActor',
        honesty: {
          biomniCatalogTotal: 224,
          admittedExecutable: 1,
          stillQuarantined: 223,
        },
      },
    })
    expect(parsed.ok).toBe(true)
    if (parsed.ok) {
      expect(parsed.inventory.admittedExecutable).toBe(1)
      expect(parsed.inventory.stillQuarantined).toBe(1)
      const run = parsed.inventory.candidates.find((c) => c.canRunViaLumen)
      expect(run?.skillId).toBe(ADMITTED_BIOMNI_UNIPROT_ID)
      expect(run?.runVia?.connectorId).toBe('uniprot')
    }
  })

  it('rejects a catalog that claims arbitrary approved disposition', () => {
    const parsed = parseEcosystemSkillInventory({
      ok: true,
      ecosystem: {
        candidates: [{ ...candidate, disposition: 'approved' }],
        summary: { total: 1, approved: 1, quarantined: 0 },
        authority: 'catalog + admission overlay; only admitted capabilities run via SessionActor',
      },
    })
    expect(parsed.ok).toBe(false)
  })

  it('rejects admitting a non-UniProt Biomni tool', () => {
    const parsed = parseEcosystemSkillInventory({
      ok: true,
      ecosystem: {
        candidates: [
          {
            ...admittedUniprot,
            skillId: 'ecosystem/biomni/analyze_enzyme_kinetics_assay',
          },
        ],
        summary: { total: 1, approved: 1, quarantined: 0 },
        authority: 'catalog + admission overlay; only admitted capabilities run via SessionActor',
      },
    })
    expect(parsed.ok).toBe(false)
  })

  it('rejects wrong connectorId on admitted capability', () => {
    const parsed = parseEcosystemSkillInventory({
      ok: true,
      ecosystem: {
        candidates: [
          {
            ...admittedUniprot,
            runVia: { ...admittedUniprot.runVia!, connectorId: 'pubmed' as 'uniprot' },
          },
        ],
        summary: { total: 1, approved: 1, quarantined: 0 },
        authority: 'catalog + admission overlay; only admitted capabilities run via SessionActor',
      },
    })
    expect(parsed.ok).toBe(false)
  })

  it('rejects duplicate ids and malformed provenance hashes', () => {
    const duplicate = { ...candidate, sourceSha256: 'not-a-sha' }
    const parsed = parseEcosystemSkillInventory({
      ok: true,
      ecosystem: {
        candidates: [candidate, duplicate],
        summary: { total: 2, approved: 0, quarantined: 2 },
        authority: 'catalog + admission overlay; only admitted capabilities run via SessionActor',
      },
    })
    expect(parsed.ok).toBe(false)
  })

  it('searches name, discipline, risk, admission track, and candidate routes', () => {
    expect(filterEcosystemSkills([candidate], 'clinical trial')).toEqual([candidate])
    expect(filterEcosystemSkills([candidate], 'clinicaltrials-gov')).toEqual([candidate])
    expect(filterEcosystemSkills([candidate], 'network-or-download')).toEqual([candidate])
    expect(filterEcosystemSkills([candidate], 'new-lumen-connector')).toEqual([candidate])
    expect(filterEcosystemSkills([candidate], 'genomics')).toEqual([])
    expect(filterEcosystemSkills([admittedUniprot], 'run via lumen')).toEqual([admittedUniprot])
  })
})
