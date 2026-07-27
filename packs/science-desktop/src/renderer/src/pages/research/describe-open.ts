/**
 * Turns the result of opening a project into something worth reading.
 *
 * Opening emitted its internals directly into a full-width monospace bar, so
 * the first thing a user saw on entering a project was:
 *
 *   Open: seeded 0 artifacts (seed: science method 'artifact_list' rejected by
 *   registry: Go MCP tool, not a Rust ACP extension method. The Rust engine
 *   dispatches only x.ai/science/* (extensions/science.rs); this call site
 *   needs the Go MCP client, not this bridge.)
 *
 * Three separate problems in one line:
 *
 *   1. "seeded 0 artifacts" reads as a failure. For a project created seconds
 *      ago it is simply the truth — there is nothing to seed yet.
 *
 *   2. The seed error is PERMANENT in this build. `artifact_list` is a Go MCP
 *      tool and this bridge speaks Rust ACP; no user action changes that, and
 *      it will appear on every open forever. A message that always fires is one
 *      people learn to ignore, which is how the banner that matters gets missed.
 *
 *   3. It names a source file. That is a fact about our repository, not about
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
  /**
   * Whether `detail` describes something structurally absent from this build
   * rather than something that went wrong.
   *
   * Worth distinguishing: a capability this build does not have is not an
   * incident, and showing both in the same alarmed styling teaches people that
   * the styling means nothing.
   */
  expected: boolean
}

/** A seed failure this build can never avoid, so it is stated as an absence. */
const STRUCTURAL = /rejected by registry|not a Rust ACP extension method|needs the Go MCP client/i

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

  if (STRUCTURAL.test(res.seedError)) {
    return {
      headline: 'Opened. Artifact previews are not available in this build.',
      detail: res.seedError,
      expected: true,
    }
  }

  // An unrecognised seed failure. Say that it is unexpected rather than
  // folding it in with the absences above — this is the one worth looking at.
  return {
    headline: 'Opened, but artifacts could not be loaded.',
    detail: res.seedError,
    expected: false,
  }
}
