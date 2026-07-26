/**
 * Turns an engine error into something a person can act on, without hiding it.
 *
 * The desk surfaced raw internals: file paths, method names, and a paragraph of
 * implementation history in a red bar across the top. That is honest and
 * unusable — a reader cannot tell whether they did something wrong, whether the
 * app is broken, or whether to wait.
 *
 * The fix is NOT to shorten it. Truncating an engine error is how a product
 * ends up saying "something went wrong" while the cause sits in a log nobody
 * reads. Instead: lead with a plain sentence about what happened and what to
 * do, and keep the original text underneath, verbatim.
 *
 * Headlines are matched on stable protocol vocabulary — "failing closed",
 * "denied", "no such method", "ECONNREFUSED" — not on prose that a later commit
 * will reword.
 */

export type DescribedError = {
  /** One sentence: what happened, and what the reader can do. */
  headline: string
  /** The original message, unmodified. Always kept. */
  detail: string
  /**
   * Whether this is a refusal by design rather than a fault.
   *
   * Worth distinguishing: "the engine is not running" is a thing the user can
   * fix, while "this operation is not implemented yet" is not, and showing both
   * in the same alarmed red teaches people to ignore the colour.
   */
  expected: boolean
}

const RULES: { match: RegExp; headline: string; expected: boolean }[] = [
  {
    // The registry refused a name that exists in neither engine.
    match: /no such method in either engine|rejected by registry/i,
    headline: 'This action is not available yet — the engine has no method for it.',
    expected: true,
  },
  {
    // Reached nothing. Distinct from a denial: nobody decided anything.
    match: /ECONNREFUSED|no permission UI|engine (is )?unavailable|transport|not wired/i,
    headline: 'The Lumen engine is not running, so nothing could be verified.',
    expected: true,
  },
  {
    // The authority answered, and the answer was no.
    match: /\bdenied\b/i,
    headline: 'The engine refused this operation.',
    expected: true,
  },
  {
    // We could not obtain a decision, so the desk refused rather than guess.
    match: /failing closed|undetermined/i,
    headline: 'Could not confirm you have access, so nothing was opened.',
    expected: true,
  },
  {
    match: /timed? ?out/i,
    headline: 'The engine did not answer in time.',
    expected: true,
  },
]

export function describeError(raw: string): DescribedError {
  const detail = raw.trim()
  for (const rule of RULES) {
    if (rule.match.test(detail)) {
      return { headline: rule.headline, detail, expected: rule.expected }
    }
  }
  // Unrecognised. Say so rather than inventing a reassuring headline: an
  // unexpected failure is exactly the one worth looking at.
  return { headline: 'Something failed that this screen does not recognise.', detail, expected: false }
}
