/**
 * Membership assertion: the ACP authority decides.
 *
 * ## The defect this replaces
 *
 * The previous implementation called ACP, and if the result was not `ok`, fell
 * through to a local JSON catalog that the desktop itself writes. Its comment
 * said it distinguished "ACP explicitly denies" from "tool missing" — the code
 * made no such distinction, because `MembershipResult` could not express one.
 * Every non-grant took the same path.
 *
 * The consequences compounded:
 *   - an explicit ACP denial was overridden by a local file
 *   - a crashed or unreachable engine silently became "allowed"
 *   - the renderer supplies `ownerId` when creating a UI project, so it could
 *     mint a project and then assert membership in it
 *
 * Since the ACP bridge has never successfully connected, the fallback was not
 * a fallback: it was the only path anything took.
 *
 * ## The rule now
 *
 *   granted      → grant
 *   denied       → deny, final. Nothing may grant what the authority refused.
 *   unavailable  → deny. Not knowing is not permission.
 *
 * The local catalog can no longer authorize anything in the production path.
 * It remains a UI convenience — titles and recently-opened — which is all a
 * display cache should ever have been.
 *
 * ## Offline mode is now explicit
 *
 * The old factory took `acp?:` as optional, so omitting it silently produced a
 * catalog-only asserter that trusted local state. Production got its security
 * from remembering to pass a field. That is backwards: the insecure mode is now
 * a separately named function, so a caller cannot reach it by forgetting
 * something, and every call site declares which trust model it wants.
 */

import type { MembershipAsserter, MembershipResult } from './session-binding'
import type { LocalProjectCatalog } from './local-project-catalog'

/**
 * Production asserter. The ACP authority is the only thing that can grant.
 *
 * `acp` is required. There is no code path here that grants without it.
 */
export function createAcpAuthoritativeMembershipAsserter(opts: {
  acp: MembershipAsserter
}): MembershipAsserter {
  return async (claim): Promise<MembershipResult> => {
    const result = await opts.acp(claim)
    if (result.ok) return result

    // Both failures deny. They are reported distinctly because the UI should
    // say "you do not have access" for one and "the engine is unavailable" for
    // the other — a user who cannot tell those apart cannot fix either.
    return {
      ok: false,
      failure: result.failure,
      reason:
        result.failure === 'denied'
          ? `membership denied by ACP: ${result.reason}`
          : `membership undetermined, failing closed: ${result.reason}`
    }
  }
}

/**
 * Offline asserter that trusts the local catalog. NOT for production.
 *
 * Only for offline fixture paths that have no engine to ask and are not
 * protecting anything real (see `osf9-product-path.ts`). Named to be
 * conspicuous at the call site and in review, because reading local state as
 * authorization is precisely the defect that was removed above.
 *
 * A grant here proves the desktop recorded the project locally. It proves
 * nothing about whether the user may have it.
 */
export function createOfflineCatalogMembershipAsserter(opts: {
  catalog: LocalProjectCatalog
}): MembershipAsserter {
  return async (claim): Promise<MembershipResult> => {
    if (opts.catalog.hasMembership(claim.ownerId, claim.projectId)) {
      return { ok: true, ownerId: claim.ownerId, projectId: claim.projectId }
    }
    return {
      ok: false,
      failure: 'no-record',
      reason: 'offline catalog has no record of this project'
    }
  }
}
