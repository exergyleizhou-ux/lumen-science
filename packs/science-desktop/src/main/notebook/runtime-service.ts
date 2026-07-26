/**
 * LUMEN STUB: Notebook runtime service — execution authority REMOVED.
 *
 * Original: Open Science v0.7.1, Apache-2.0, commit d8f11e34
 *   Manages notebook kernel lifecycle and session binding.
 *
 * Lumen Science Desktop: this is a NO-OP stub.
 * Notebook runtime is owned by Rust Lumen KernelAdapter.
 * This stub exists for IPC handler registration compatibility only.
 *
 * Use: the UI surface (history, IPYNB export) is preserved in the renderer.
 * Actual kernel execution routes via Rust Lumen.
 */
import { EventEmitter } from 'events'

export interface NotebookEnvironmentManager {
  listEnvironments(): Promise<string[]>
  currentEnvironment(): string | null
  setEnvironment(_env: string): Promise<void>
}

export function createDefaultNotebookRuntimeService() {
  console.warn('[lumen-stub] createDefaultNotebookRuntimeService — EXECUTION AUTHORITY REMOVED.')
  return {
    execute: (_code: string) =>
      Promise.reject(new Error('Notebook execution stubbed — use Rust KernelAdapter')),
    interrupt: () => {},
    shutdown: () => {},
    get history() { return [] },
    on: (_ev: string, _cb: (...args: unknown[]) => void) => {},
    off: (_ev: string, _cb: (...args: unknown[]) => void) => {},
  }
}
