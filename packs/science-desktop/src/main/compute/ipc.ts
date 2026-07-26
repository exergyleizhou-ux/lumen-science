/**
 * LUMEN STUB: Compute IPC handlers — execution authority REMOVED.
 *
 * Original: Open Science v0.7.1, Apache-2.0, commit d8f11e34
 *   Full compute job submission, polling, harvesting via SSH.
 *
 * Lumen Science Desktop: stubbed IPC handlers. SSH/Slurm execution
 * is owned by Rust Lumen. This module re-exports types to keep
 * the module graph intact but routes no compute operations.
 */

// ── Compatibility type stubs ─────────────────────────────────────

export type JobSummary = {
  id: string
  host: string
  status: string
  jobName?: string
  createdAt: string
  updatedAt: string
}

export type ComputeHandlers = Record<string, (...args: unknown[]) => unknown>

export const registerComputeIpcHandlers = (
  _artifactResolver?: unknown,
  _sshRunner?: unknown,
  _harvestEngine?: unknown
) => {
  console.warn('[lumen-stub] registerComputeIpcHandlers — EXECUTION AUTHORITY REMOVED. Route via Rust Lumen.')
  return {
    listJobs: () => [],
    submit: () => Promise.reject(new Error('Compute submit stubbed — use Rust Lumen')),
    cancel: () => Promise.resolve(),
    getResult: () => Promise.reject(new Error('Compute result stubbed')),
  }
}

export const broadcastJobUpdated = (_summary: JobSummary): void => {
  // no-op
}

export const createJobUpdatedBroadcaster = () => ({
  broadcast: broadcastJobUpdated,
})
