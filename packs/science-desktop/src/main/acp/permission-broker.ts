/**
 * LUMEN STUB: Permission broker — execution authority REMOVED.
 *
 * Original: Open Science v0.7.1, Apache-2.0, commit d8f11e34
 *   Full permission store with auto-approve, Full Access, deny semantics.
 *
 * Lumen Science Desktop: stub that exports the same class names for
 * module-graph compatibility (runtime-coordinator.ts imports
 * ConversationPermissionGrantStore), but all permission decisions
 * route through Rust Lumen SessionActor.
 *
 * See: packs/science-desktop/ARCHITECTURE.md
 */

const LOG_ONCE = new Set<string>()

function warnOnce(msg: string) {
  if (!LOG_ONCE.has(msg)) {
    LOG_ONCE.add(msg)
    console.warn(`[lumen-stub] ${msg}`)
  }
}

// ── Compatibility stubs ──────────────────────────────────────────

export class ConversationPermissionGrantStore {
  list(_sessionId: string): string[] {
    return []
  }

  has(_sessionId: string, _categoryKey: string): boolean {
    return false
  }

  remember(_sessionId: string, _categoryKey: string): void {
    // no-op; authority is Rust Lumen
  }

  snapshot(): Record<string, unknown[]> {
    return {}
  }
}

export class AcpPermissionBroker {
  constructor(_opts?: Record<string, unknown>) {
    warnOnce('AcpPermissionBroker constructed — AUTHORITY REMOVED. Use Rust Lumen.')
  }

  request() {
    return Promise.resolve({
      outcome: 'cancelled' as const,
      reason: 'Permission stubbed — Rust Lumen owns permission decisions',
    })
  }
}
