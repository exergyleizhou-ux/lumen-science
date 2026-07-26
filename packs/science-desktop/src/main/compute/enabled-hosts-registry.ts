/**
 * LUMEN STUB: Enabled compute hosts registry — no-op stub.
 *
 * Original: Open Science v0.7.1, Apache-2.0, commit d8f11e34
 * Lumen: compute host registry is owned by Rust Lumen.
 */
export class EnabledComputeHostsRegistry {
  list() { return [] }
  add() {}
  remove() {}
}

export const enabledComputeHostsRegistry = new EnabledComputeHostsRegistry()

export function attachEnabledComputeHosts<T extends object>(
  _target: T,
  _registry?: EnabledComputeHostsRegistry
): T & { _computeHostsStubbed: true } {
  return { ..._target, _computeHostsStubbed: true } as T & { _computeHostsStubbed: true }
}
