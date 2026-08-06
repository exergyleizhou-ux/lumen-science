import { describe, expect, it } from 'vitest'

import { resolveEnabledSkillIds } from './skill-resolution'

const CATALOG = new Set(['alpha', 'beta', 'gamma', 'delta'])

describe('resolveEnabledSkillIds (S0-B fail-closed forced-skill resolution)', () => {
  it('resolves every known, non-disabled skill as enabled when nothing is forced or disabled', () => {
    const result = resolveEnabledSkillIds({ disabledSkillIds: [], catalogIds: CATALOG })
    expect(result.enabledIds).toEqual(['alpha', 'beta', 'delta', 'gamma'])
    expect(result.disabledIds).toEqual([])
  })

  it('keeps user-disabled skills disabled even when a task names them (forced cannot resurrect)', () => {
    const result = resolveEnabledSkillIds({
      disabledSkillIds: ['beta', 'delta'],
      catalogIds: CATALOG,
    })
    expect(result.enabledIds).toEqual(['alpha', 'gamma'])
    expect(result.disabledIds).toEqual(['beta', 'delta'])
  })

  it('never enables unknown ids (fail-closed) regardless of any other input', () => {
    const result = resolveEnabledSkillIds({
      disabledSkillIds: [],
      catalogIds: CATALOG,
      // "mystery" is not in CATALOG: even though it is not disabled and not
      // revoked, it must stay out of the enabled set.
      idsWithoutActiveRevision: new Set(['mystery']),
    })
    expect(result.enabledIds).not.toContain('mystery')
    expect(result.enabledIds).toEqual(['alpha', 'beta', 'delta', 'gamma'])
  })

  it('keeps revoked skills disabled even when they are not in the disabled set', () => {
    const result = resolveEnabledSkillIds({
      disabledSkillIds: ['beta'],
      catalogIds: CATALOG,
      revokedIds: new Set(['gamma']),
    })
    expect(result.enabledIds).toEqual(['alpha', 'delta'])
    expect(result.disabledIds).toEqual(['beta'])
  })

  it('keeps skills without an actor-approved active revision disabled (X-M1 reservation)', () => {
    const result = resolveEnabledSkillIds({
      disabledSkillIds: [],
      catalogIds: CATALOG,
      idsWithoutActiveRevision: new Set(['alpha', 'beta']),
    })
    expect(result.enabledIds).toEqual(['delta', 'gamma'])
    expect(result.disabledIds).toEqual([])
  })

  it('revoked and no-active-revision take precedence even when the id is forced-equivalent input', () => {
    // The resolver takes no forced input; this case proves the reservation
    // sets alone are sufficient to exclude the id.
    const result = resolveEnabledSkillIds({
      disabledSkillIds: [],
      catalogIds: CATALOG,
      revokedIds: new Set(['alpha']),
      idsWithoutActiveRevision: new Set(['beta']),
    })
    expect(result.enabledIds).toEqual(['delta', 'gamma'])
  })

  it('normal enabled + active-revision skills still resolve (positive case)', () => {
    const result = resolveEnabledSkillIds({
      disabledSkillIds: ['delta'],
      catalogIds: CATALOG,
      idsWithoutActiveRevision: new Set(['gamma']),
    })
    // alpha and beta are known, not disabled, not revoked, with active revision.
    expect(result.enabledIds).toEqual(['alpha', 'beta'])
  })
})
