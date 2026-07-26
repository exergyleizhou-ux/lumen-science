/**
 * Membership assertion + artifact list via ACP loopback.
 *
 * Fail-closed when Lumen binary is unavailable or tools reject the claim.
 */

import type { MembershipAsserter, ArtifactListItem } from './session-binding'

export type AcpToolCall = (
  toolName: string,
  args: Record<string, unknown>,
) => Promise<unknown>

/**
 * Assert project membership through ACP.
 * Tool contract (best-effort): project_assert_membership { owner_id, project_id }
 * → { ok: true, owner_id, project_id } | { ok: false, reason }
 *
 * Fallback when tool missing: reject (no silent self-attestation).
 */
export function createAcpMembershipAsserter(call: AcpToolCall): MembershipAsserter {
  return async (claim) => {
    try {
      const raw = await call('project_assert_membership', {
        owner_id: claim.ownerId,
        project_id: claim.projectId,
      })
      const body = unwrap(raw)
      if (!body) {
        return { ok: false, reason: 'empty membership response' }
      }
      if (body.ok === false || body.ok === 'false') {
        return {
          ok: false,
          reason: String(body.reason ?? body.error ?? 'membership denied'),
        }
      }
      // Accept explicit ok:true or presence of matching owner/project fields
      const ownerId = String(body.owner_id ?? body.ownerId ?? claim.ownerId)
      const projectId = String(body.project_id ?? body.projectId ?? claim.projectId)
      if (body.ok === true || body.ok === 'true' || body.member === true) {
        if (ownerId !== claim.ownerId || projectId !== claim.projectId) {
          return {
            ok: false,
            reason: 'membership response identity mismatch',
          }
        }
        return { ok: true, ownerId, projectId }
      }
      return { ok: false, reason: 'membership not confirmed by ACP' }
    } catch (e: unknown) {
      return {
        ok: false,
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
