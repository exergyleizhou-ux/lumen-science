/**
 * Hybrid membership: ACP first, then local UI catalog.
 *
 * Local catalog membership is only for projects created via files:create-ui-project
 * in this desktop — not arbitrary self-attestation of foreign project IDs.
 */

import type { MembershipAsserter } from './session-binding'
import type { LocalProjectCatalog } from './local-project-catalog'

export function createHybridMembershipAsserter(opts: {
  acp?: MembershipAsserter
  catalog: LocalProjectCatalog
}): MembershipAsserter {
  return async (claim) => {
    if (opts.acp) {
      const acpResult = await opts.acp(claim)
      if (acpResult.ok) return acpResult
      // If ACP explicitly denies (vs tool missing), still allow local catalog
      // only when catalog owns the project — catalog is UI scope, not ACP override
      // for foreign projects.
    }
    if (opts.catalog.hasMembership(claim.ownerId, claim.projectId)) {
      return {
        ok: true,
        ownerId: claim.ownerId,
        projectId: claim.projectId,
      }
    }
    return {
      ok: false,
      reason: 'membership denied (ACP + local catalog)',
    }
  }
}
