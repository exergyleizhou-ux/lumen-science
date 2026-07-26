/**
 * LUMEN STUB: Job poller — execution authority REMOVED.
 *
 * Original: Open Science v0.7.1, Apache-2.0, commit d8f11e34
 * Lumen: job polling is owned by Rust Lumen SessionActor.
 */
import { EventEmitter } from 'events'

export class JobPoller extends EventEmitter {
  constructor(_opts: Record<string, unknown>) {
    super()
    console.warn('[lumen-stub] JobPoller instantiated — EXECUTION AUTHORITY REMOVED.')
  }
  start() { /* no-op */ }
  stop() { /* no-op */ }
}
