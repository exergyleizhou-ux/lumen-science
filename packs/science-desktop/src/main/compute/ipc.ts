/**
 * LUMEN STUB: Compute IPC handlers — execution authority REMOVED.
 *
 * Original: Open Science v0.7.1, Apache-2.0, commit d8f11e34
 *   Full compute job submission, polling, harvesting via SSH.
 *
 * Lumen Science Desktop: stubbed. Returns compatible shapes for
 * ipc.ts destructuring so startup doesn't crash, but all execution
 * methods are no-ops. SSH/Slurm is owned by Rust Lumen.
 */

import { EventEmitter } from 'events'

// ── Compatibility types ──────────────────────────────────────────

export type JobSummary = {
  id: string; host: string; status: string
  jobName?: string; createdAt: string; updatedAt: string
}

// Stub service that satisfies the destructured shape ipc.ts expects
class StubComputeService {
  listJobs() { return [] }
  submit() { return Promise.reject(new Error('Compute submit stubbed — use Rust Lumen')) }
  cancel() { return Promise.resolve() }
  notifyJobCompleted() { /* no-op */ }
}

class StubJobRepository extends EventEmitter {
  get() { return null }
  save() { return null }
  list() { return [] }
}

class StubHostRepository extends EventEmitter {
  get() { return null }
  list() { return [] }
}

export const COMPUTE_JOBS_LIST_CHANNEL = 'compute:jobs:list'
export const COMPUTE_JOB_UPDATED_CHANNEL = 'compute:job-updated'

export const registerComputeIpcHandlers = (
  _a?: unknown, _b?: unknown, _c?: unknown
) => {
  console.warn('[lumen-stub] registerComputeIpcHandlers — EXECUTION AUTHORITY REMOVED.')
  return {
    computeService: new StubComputeService(),
    jobRepository: new StubJobRepository(),
    hostRepository: new StubHostRepository(),
    enabledComputeHostsRegistry: { list: () => [], add: () => {}, remove: () => {} },
  }
}

export const broadcastJobUpdated = () => {}
export const createJobUpdatedBroadcaster = () => ({ broadcast: broadcastJobUpdated })
