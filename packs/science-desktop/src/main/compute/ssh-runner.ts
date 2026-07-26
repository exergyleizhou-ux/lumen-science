/**
 * LUMEN STUB: SSH runner — execution authority REMOVED.
 *
 * Original: Open Science v0.7.1, Apache-2.0, commit d8f11e34
 *   Full SSH execution capability.
 *
 * Lumen Science Desktop: this is a NO-OP stub.
 * SSH execution is owned by Rust Lumen SessionActor.
 * Route via: x.ai/science/compute_plan → Rust SSH ToolAdapter
 */
import { EventEmitter } from 'events'

export class SystemSshRunner extends EventEmitter {
  constructor() {
    super()
    console.warn('[lumen-stub] SystemSshRunner instantiated — EXECUTION AUTHORITY REMOVED. Route via Lumen bridge.')
  }
  connect(_host: string, _opts: Record<string, unknown>) {
    return Promise.reject(new Error('SSH execution stubbed — use Rust Lumen'))
  }
  run(_cmd: string) {
    return Promise.reject(new Error('SSH exec stubbed — use Rust Lumen'))
  }
  disconnect() { /* no-op */ }
}
