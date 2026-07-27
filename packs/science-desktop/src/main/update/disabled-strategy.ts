import type { UpdateStatus } from '../../shared/update'
import type { UpdateStrategy } from './strategy'

/**
 * The strategy used when update policy is off (LS5-R1-02).
 *
 * The point is what this class does *not* have: no `autoUpdater`, no `fetch`,
 * no manifest URL, no filesystem staging. Selecting it is what guarantees the
 * app opens no update socket — not a flag checked inside a strategy that has
 * already constructed a network client.
 *
 * Every command reports `disabled` with the policy's reason, so the UI can
 * explain the state instead of showing a check that silently does nothing.
 */
export class DisabledUpdateStrategy implements UpdateStrategy {
  private readonly status: UpdateStatus

  constructor(currentVersion: string, reason: string) {
    this.status = { state: 'disabled', current: currentVersion, error: reason }
  }

  getStatus(): UpdateStatus {
    return this.status
  }

  // All four are the same deliberate no-op. They resolve rather than reject:
  // a disabled updater is a supported configuration, not a failure.
  async check(): Promise<UpdateStatus> {
    return this.status
  }

  async download(): Promise<UpdateStatus> {
    return this.status
  }

  async cancel(): Promise<UpdateStatus> {
    return this.status
  }

  async apply(): Promise<UpdateStatus> {
    return this.status
  }
}
