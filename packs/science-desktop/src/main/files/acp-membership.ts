/**
 * Membership assertion + artifact list via ACP loopback.
 *
 * Fail-closed when Lumen binary is unavailable or tools reject the claim.
 */

import type { MembershipAsserter, ArtifactListItem } from './session-binding'

/**
 * Where project state lives, relative to the engine's session workspace.
 *
 * A name rather than a path: the engine resolves it with
 * `canonical_dir_within`, so the desktop cannot aim this at somewhere else on
 * disk even if this constant were wrong.
 */
export const SCIENCE_STORE_DIR = 'science-store'

export type AcpToolCall = (
  toolName: string,
  args: Record<string, unknown>,
) => Promise<unknown>

/**
 * Assert project membership through ACP.
 * Tool contract: project_assert_membership { projectId, ownerId, storeRoot }
 * `sessionId` is filled in by the session manager, which knows it.
 * → { ok: true, owner_id, project_id } | { ok: false, reason }
 *
 * Every failure is classified as `denied` or `unavailable`, because callers
 * must treat them differently: a denial is the authority's answer and is final,
 * while unavailability means we do not know. Collapsing both into a bare
 * `ok: false` is what let the hybrid asserter grant what ACP had refused.
 *
 * A malformed or unrecognised response is `unavailable`, not `denied` — we
 * cannot claim the authority denied something when we could not read its
 * answer. Neither outcome grants anything.
 */
export function createAcpMembershipAsserter(call: AcpToolCall): MembershipAsserter {
  return async (claim) => {
    try {
      const raw = await call('project_assert_membership', {
        // camelCase, and exactly these fields: the Rust param structs are
        // `deny_unknown_fields`, so a stray or snake_cased key is a hard
        // rejection rather than a silently ignored argument.
        projectId: claim.projectId,
        ownerId: claim.ownerId,
        // Resolved inside the session workspace by the engine. The desktop
        // names the subdirectory; it does not get to point at an arbitrary
        // path, and `canonical_dir_within` enforces that on the other side.
        storeRoot: SCIENCE_STORE_DIR,
      })
      const body = unwrap(raw)
      if (!body) {
        // Reached the authority but could not read its answer.
        return { ok: false, failure: 'unavailable', reason: 'empty membership response' }
      }
      if (body.ok === false || body.ok === 'false') {
        // The authority answered no. This is the only true denial.
        return {
          ok: false,
          failure: 'denied',
          reason: String(body.reason ?? body.error ?? 'membership denied'),
        }
      }
      // Accept explicit ok:true or presence of matching owner/project fields
      const ownerId = String(body.owner_id ?? body.ownerId ?? claim.ownerId)
      const projectId = String(body.project_id ?? body.projectId ?? claim.projectId)
      if (body.ok === true || body.ok === 'true' || body.member === true) {
        if (ownerId !== claim.ownerId || projectId !== claim.projectId) {
          // The authority granted a DIFFERENT owner/project than was claimed.
          // Treated as a denial of the claim actually made.
          return {
            ok: false,
            failure: 'denied',
            reason: 'membership response identity mismatch',
          }
        }
        return { ok: true, ownerId, projectId }
      }
      // Response parsed but affirmed nothing: not a denial, just no grant.
      return { ok: false, failure: 'unavailable', reason: 'membership not confirmed by ACP' }
    } catch (e: unknown) {
      // Transport error, missing tool, timeout, crashed child. We did not
      // reach a decision — emphatically not a denial, and not permission.
      return {
        ok: false,
        failure: 'unavailable',
        reason: `membership ACP error: ${(e as Error).message || String(e)}`,
      }
    }
  }
}

export async function listArtifactsViaAcp(
  call: AcpToolCall,
  args: { projectId: string; runId: string },
): Promise<ArtifactListItem[]> {
  const raw = await call('artifact_list', {
    project_id: args.projectId,
    run_id: args.runId,
  })
  return normalizeArtifactList(raw)
}

function unwrap(raw: unknown): Record<string, unknown> | null {
  if (!raw || typeof raw !== 'object') return null
  const r = raw as Record<string, unknown>
  // MCP TextResult sometimes nests JSON in content[0].text
  if (Array.isArray(r.content) && r.content[0] && typeof r.content[0] === 'object') {
    const c0 = r.content[0] as Record<string, unknown>
    if (typeof c0.text === 'string') {
      try {
        const parsed = JSON.parse(c0.text)
        if (parsed && typeof parsed === 'object') return parsed as Record<string, unknown>
      } catch {
        /* fall through */
      }
    }
  }
  if (r.result && typeof r.result === 'object') return r.result as Record<string, unknown>
  return r
}

export function normalizeArtifactList(raw: unknown): ArtifactListItem[] {
  if (!raw) return []
  if (Array.isArray(raw)) return raw as ArtifactListItem[]
  const body = unwrap(raw)
  if (!body) return []
  if (Array.isArray(body)) return body as ArtifactListItem[]
  if (Array.isArray(body.artifacts)) return body.artifacts as ArtifactListItem[]
  if (Array.isArray(body.items)) return body.items as ArtifactListItem[]
  // Single meta object
  if (body.artifact_id || body.artifactId) return [body as ArtifactListItem]
  return []
}
