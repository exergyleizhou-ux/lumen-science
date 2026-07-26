/**
 * LUMEN STUB: Reviewer IPC — execution authority REMOVED.
 *
 * Original: Open Science v0.7.1, Apache-2.0, commit d8f11e34
 *   Full reviewer orchestration with runReview, fix loops, logging.
 *
 * Lumen Science Desktop: registerReviewerIpcHandlers is a no-op.
 * Reviewer execution is owned by Rust Lumen SessionActor.
 * route: x.ai/science/review_submit → SessionActor → EvidenceGraph
 */

const REVIEWER_IPC = {
  RUN: 'reviewer:run',
  GET_FOR_SESSION: 'reviewer:get-for-session',
  ABORT_FIX_LOOP: 'reviewer:abort-fix-loop',
}

export const registerReviewerIpcHandlers = () => {
  console.warn('[lumen-stub] registerReviewerIpcHandlers — EXECUTION AUTHORITY REMOVED.')
}
export const createDefaultReviewRepository = () => ({
  save: () => null,
  get: () => null,
  listForSession: () => [],
})

// Re-export types
export type { REVIEWER_IPC }
