/**
 * STUB: Open Science permission broker — execution authority REMOVED.
 *
 * Original: Open Science v0.7.1, Apache-2.0, commit d8f11e34
 *   Full permission broker with auto-approve, Full Access, and deny semantics.
 *
 * Lumen Science Desktop: this file is a NO-OP stub.
 *   All permission decisions are made by Rust Lumen SessionActor.
 *   This stub exists only so React UI code that imports the broker compiles.
 *   At runtime, it always returns CANCELLED (pending Lumen bridge routing).
 *
 * NEVER re-enable "Full Access" or "auto-approve edits" semantics.
 * Hard-deny policy applies regardless of UI mode.
 *
 * See: packs/science-desktop/ARCHITECTURE.md
 * See: third_party/open-science/NOTICE
 */

import type {
  AcpPermissionGrant,
  AcpPermissionRequest,
  AcpPermissionResponse,
} from '../../shared/acp'

// ── Stub permission store (in-memory only, no persistence) ───────

class StubPermissionStore {
  request(_req: AcpPermissionRequest): Promise<AcpPermissionResponse> {
    console.warn(
      '[lumen-stub] Permission broker called — AUTHORITY REMOVED.\n' +
        'Routing through Rust Lumen SessionActor instead.'
    )
    return Promise.resolve({
      outcome: 'cancelled' as const,
      reason: 'Permission broker stubbed — authority belongs to Rust Lumen',
    })
  }
}

const stubStore = new StubPermissionStore()

export { stubStore as default }

// Re-export types so UI code compiles
export type { AcpPermissionGrant, AcpPermissionRequest, AcpPermissionResponse }
