// S0-B: pure resolution of the enabled/disabled skill sets for runtime
// provisioning. Fail-closed by construction:
//   - a skill the catalog does not know (unknown) is never enabled;
//   - a revoked skill is never enabled;
//   - a skill without an actor-approved active revision is never enabled
//     (the ActiveRevision mechanism lands with X-M1; until then the set is
//     empty and the rule is a no-op, keeping today's shipped behavior for
//     normal enabled skills);
//   - task-level forced activation can NOT resurrect any of the above, nor a
//     skill the user disabled. This module deliberately takes no
//     `forcedSkillIds` input at all: there is no code path in which a forced
//     id could flip a skill into the enabled set.
// The one positive case: a known, non-revoked, active-revision skill that is
// not in the disabled set resolves enabled.
export type SkillResolutionInput = {
  /** ids the user disabled in settings. */
  disabledSkillIds: readonly string[]
  /** ids the skill catalog actually knows. Anything else is unknown. */
  catalogIds: ReadonlySet<string>
  /** Reserved for X-M1: ids whose revision is revoked. Defaults to empty. */
  revokedIds?: ReadonlySet<string>
  /** Reserved for X-M1: ids lacking an actor-approved ActiveRevision. Defaults to empty. */
  idsWithoutActiveRevision?: ReadonlySet<string>
}

export type SkillResolution = {
  enabledIds: string[]
  disabledIds: string[]
}

export function resolveEnabledSkillIds(input: SkillResolutionInput): SkillResolution {
  const { disabledSkillIds, catalogIds } = input
  const revokedIds = input.revokedIds ?? new Set<string>()
  const withoutActiveRevision = input.idsWithoutActiveRevision ?? new Set<string>()

  const disabled = new Set(disabledSkillIds)
  const enabled = new Set<string>()

  for (const id of catalogIds) {
    if (disabled.has(id)) {
      continue
    }
    if (revokedIds.has(id) || withoutActiveRevision.has(id)) {
      continue
    }
    // Unknown ids are not in catalogIds at all, so they never reach here.
    enabled.add(id)
  }

  const sorted = (set: ReadonlySet<string>): string[] => [...set].sort()
  return { enabledIds: sorted(enabled), disabledIds: sorted(disabled) }
}
