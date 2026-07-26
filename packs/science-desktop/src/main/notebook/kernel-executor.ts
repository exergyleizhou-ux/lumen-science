/**
 * LUMEN STUB: Kernel executor — execution authority REMOVED.
 *
 * Original: Open Science v0.7.1, Apache-2.0, commit d8f11e34
 *   Full Python/R kernel execution via TypeScript.
 *
 * Lumen Science Desktop: this is a NO-OP stub.
 * Kernel execution is owned by Rust Lumen KernelAdapter.
 * Route via: x.ai/science/notebook_execute → SessionActor → KernelAdapter
 */

import { EventEmitter } from 'events'

export class KernelExecutor extends EventEmitter {
  constructor(_opts?: Record<string, unknown>) {
    super()
    console.warn('[lumen-stub] KernelExecutor instantiated — EXECUTION AUTHORITY REMOVED. Route via Lumen bridge.')
  }
  execute(_code: string) {
    return Promise.reject(new Error('Kernel execution stubbed — use Rust KernelAdapter'))
  }
  interrupt() { /* no-op stub */ }
  shutdown() { /* no-op stub */ }
  get state() { return 'stopped' as const }
}

export type KernelState = 'running' | 'stopped' | 'error'
export type NotebookOutput = { stdout: string; stderr: string; ok: boolean }
