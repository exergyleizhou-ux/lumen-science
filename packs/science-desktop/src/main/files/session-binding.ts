/**
 * OSF-2 session binding — trusted identity + artifact index seed.
 *
 * Identity is never taken from the renderer alone. A MembershipAsserter
 * (ACP project membership or fixture) must accept the claim first.
 * After bind, optional artifact_list results seed the preview store so
 * files:preview-by-artifact can resolve artifact_id without path open.
 *
 * ZIP/.skill quarantine requires sender-scoped identity (`senderId`).
 * Legacy consumers still use process-global context (residual P0).
 */

import {
  setTrustedPreviewContext,
  clearTrustedPreviewContext,
  clearTrustedPreviewContextForSender,
  clearAllTrustedPreviewContexts,
  attachTrustedIdentitySenderCleanup,
  beginTrustedPreviewContextBinding,
  commitTrustedPreviewContextForSender,
  type TrustedPreviewContext,
  type TrustedIdentitySender,
} from './session-identity'
import type { PreviewFileRecord } from './preview-resolver'

export type MembershipClaim = {
  ownerId: string
  projectId: string
}

/**
 * Why a membership claim failed.
 *
 * This distinction is load-bearing, not documentation. The previous two-state
 * result made "the authority said no" and "we could not reach the authority"
 * literally indistinguishable, so `createHybridMembershipAsserter` fell through
 * to the local catalog in both cases — while its comment claimed it did not.
 * A comment cannot make a distinction the type cannot represent.
 *
 *  `denied`      the authority answered, and the answer was no. FINAL: no
 *                other source may grant what the authority refused.
 *  `unavailable` the authority could not be reached, returned an unusable
 *                response, or does not implement the check. NOT a denial, and
 *                also not permission — it means we do not know, and not
 *                knowing must fail closed on any execution path.
 *  `no-record`   no authority was consulted and no local record exists.
 */
export type MembershipFailure = 'denied' | 'unavailable' | 'no-record'

export type MembershipResult =
  | { ok: true; ownerId: string; projectId: string }
  | { ok: false; failure: MembershipFailure; reason: string }

export type MembershipAsserter = (claim: MembershipClaim) => Promise<MembershipResult>

export type ArtifactListItem = {
  artifact_id?: string
  artifactId?: string
  path?: string
  storage_path?: string
  sha256?: string
  digest?: string
  project_id?: string
  projectId?: string
  run_id?: string
  runId?: string
  owner_id?: string
  ownerId?: string
}

export type SeedableStore = {
  put(artifactId: string, record: PreviewFileRecord): void
}

export type BindSessionOptions = {
  assertMembership: MembershipAsserter
  /**
   * Electron webContents.id of the invoking renderer. When set, identity is
   * stored only for that sender (ZIP quarantine authority path). When omitted,
   * falls back to process-global context for legacy consumers / scripts.
   */
  senderId?: number
  /** Optional WebContents-like handle used to clear identity on teardown. */
  sender?: TrustedIdentitySender
}

export type BindSessionResult =
  | { ok: true; ownerId: string; projectId: string }
  | { ok: false; reason: string }

/**
 * Assert membership then set main-process trusted preview context.
 *
 * Failed rebind for a senderId ALWAYS revokes that sender's prior binding so a
 * denied membership cannot leave a stale capability in place.
 */
export async function bindTrustedSession(
  claim: MembershipClaim,
  opts: BindSessionOptions,
): Promise<BindSessionResult> {
  const senderEpoch =
    opts.senderId === undefined
      ? undefined
      : beginTrustedPreviewContextBinding(opts.senderId)
  if (opts.sender) {
    attachTrustedIdentitySenderCleanup(opts.sender)
  }
  if (!claim.ownerId || !claim.projectId) {
    return { ok: false, reason: 'ownerId and projectId are required' }
  }
  const result = await opts.assertMembership(claim)
  if (!result.ok) {
    if (opts.senderId === undefined) {
      clearTrustedPreviewContext()
    }
    return { ok: false, reason: result.reason || 'membership denied' }
  }
  const trusted = {
    ownerId: result.ownerId,
    projectId: result.projectId,
  }
  if (opts.senderId !== undefined) {
    if (
      senderEpoch === undefined ||
      !commitTrustedPreviewContextForSender(opts.senderId, senderEpoch, trusted)
    ) {
      return {
        ok: false,
        reason: 'membership result was superseded by navigation, unbind, restart, or a newer bind',
      }
    }
  } else {
    // Legacy process-global path (residual P0 for non-ZIP consumers).
    setTrustedPreviewContext(trusted)
  }
  return { ok: true, ownerId: result.ownerId, projectId: result.projectId }
}

/** Clear one sender's trusted binding (remove-current-project / unbind). */
export function unbindTrustedSession(senderId?: number): void {
  if (senderId !== undefined) {
    clearTrustedPreviewContextForSender(senderId)
    return
  }
  clearTrustedPreviewContext()
}

/** Engine stop/restart: invalidate every sender and the legacy global bag. */
export function clearAllTrustedSessions(): void {
  clearAllTrustedPreviewContexts()
}

/**
 * Seed preview store from artifact_list-shaped items under trusted ownership.
 * Skips malformed rows. Ownership for isolation always comes from `ownership`
 * (trusted session), not from untrusted list fields alone — list project_id
 * is checked when present to avoid cross-project pollution.
 */
export function seedPreviewStoreFromList(
  store: SeedableStore,
  items: ArtifactListItem[],
  ownership: TrustedPreviewContext,
): number {
  let n = 0
  for (const item of items) {
    const artifactId = String(item.artifact_id ?? item.artifactId ?? '')
    const path = String(item.path ?? item.storage_path ?? '')
    const sha256 = String(item.sha256 ?? item.digest ?? '')
    if (!artifactId || !path || !sha256) continue

    const itemProject = item.project_id ?? item.projectId
    if (itemProject && itemProject !== ownership.projectId) continue

    const itemOwner = item.owner_id ?? item.ownerId
    if (itemOwner && itemOwner !== ownership.ownerId) continue
    const runId = String(item.run_id ?? item.runId ?? '')
    if (!runId) continue

    store.put(artifactId, {
      path,
      sha256,
      ownerId: ownership.ownerId,
      projectId: ownership.projectId,
      runId,
    })
    n++
  }
  return n
}
