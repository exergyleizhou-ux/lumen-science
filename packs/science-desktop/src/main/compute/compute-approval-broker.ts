/**
 * LUMEN STUB: Compute approval broker — execution authority REMOVED.
 *
 * Original: Open Science v0.7.1, Apache-2.0, commit d8f11e34
 * Lumen: compute approval is owned by Rust Lumen SessionActor.
 */
export class ComputeApprovalBroker {
  constructor(_opts?: Record<string, unknown>) {
    console.warn('[lumen-stub] ComputeApprovalBroker constructed — AUTHORITY REMOVED.')
  }
  request(_req: unknown) {
    return Promise.resolve({ approved: false, reason: 'Compute broker stubbed — use Rust Lumen' })
  }
}
