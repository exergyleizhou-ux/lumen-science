/**
 * Turns the result of opening a project into something worth reading.
 *
 * Opening used to emit its internals directly into a full-width monospace bar.
 * Two separate problems were hidden in that presentation:
 *
 *   1. "seeded 0 artifacts" reads as a failure. For a project created seconds
 *      ago it is simply the truth — there is nothing to seed yet.
 *
 *   2. Errors named source files. That is a fact about our repository, not about
 *      the user's project.
 *
 * The fix is not to hide it — a swallowed absence is how a product ends up
 * claiming to have seeded evidence it never had. Instead the plain outcome
 * leads, and the technical reason stays available underneath, verbatim.
 */

export type OpenOutcome = {
  /** One plain sentence about what happened. */
  headline: string
  /**
   * The engine's own words, when there were any. Kept whole and never
   * summarised; the UI puts it behind a disclosure rather than dropping it.
   */
  detail?: string
  /** Whether the outcome is normal rather than a seed failure. */
  expected: boolean
}

export function describeOpen(res: { seeded?: number; seedError?: string }): OpenOutcome {
  const seeded = res.seeded ?? 0

  if (!res.seedError) {
    return {
      headline:
        seeded > 0
          ? `Opened — ${seeded} artifact${seeded === 1 ? '' : 's'} ready to preview.`
          : // Not a failure. A new project has nothing in it yet, and saying so
            // plainly beats reporting a zero as though something went missing.
            'Opened. No artifacts yet — results appear here as the project produces them.',
      expected: true,
    }
  }

  // artifact_list is part of the Rust engine now. Any seed error is unexpected
  // and worth surfacing; an old/new binary mismatch is not a normal absence.
  return {
    headline: 'Opened, but artifacts could not be loaded.',
    detail: res.seedError,
    expected: false,
  }
}
