/**
 * LUMEN STUB: SCP runner — execution authority REMOVED.
 *
 * Original: Open Science v0.7.1, Apache-2.0, commit d8f11e34
 * Lumen: SCP is owned by Rust Lumen SessionActor.
 * Route via: x.ai/science/compute_plan → Rust SCP ToolAdapter
 */
export class SystemScpRunner {
  constructor() {
    console.warn('[lumen-stub] SystemScpRunner instantiated — EXECUTION AUTHORITY REMOVED.')
  }
  copy(_from: string, _to: string, _opts?: Record<string, unknown>) {
    return Promise.reject(new Error('SCP stubbed — use Rust Lumen'))
  }
}
