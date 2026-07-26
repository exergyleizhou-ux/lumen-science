/**
 * OSF-4 Reviewer product service.
 *
 * Plan / validate locally; submit via ACP start_review with artifact hashes;
 * hash-mismatch fail-closed at submission time. Dossier export includes
 * artifact IDs, hashes, and plan/verdict refs.
 */

import {
  planReview,
  assertReviewAccess,
  normalizeReviewResult,
  buildReviewAcpPayload,
  validateArtifactHashes,
  isVerdictStale,
  type ReviewRequest,
  type ReviewPlan,
  type ReviewVerdictProjection,
  type ReviewEvidence,
} from './review-plan'
import { getTrustedPreviewContext } from './session-identity'
import type { PreviewFileStore } from './preview-resolver'

export type AcpReviewCall = (
  toolName: string,
  args: Record<string, unknown>,
) => Promise<unknown>

export type ReviewService = {
  plan: (req: ReviewRequest) => ReturnType<typeof planReview>
  submit: (req: ReviewRequest) => Promise<unknown>
  history: () => ReviewVerdictProjection[]
  latest: () => ReviewVerdictProjection | null
  exportDossier: () => DossierExportProjection | { ok: false; reason: string }
  /** Check if latest verdict has gone stale against fresh evidence */
  checkStale: (evidence: ReviewEvidence[]) => { stale: boolean; mismatches: string[] } | { ok: false; reason: string }
  clear: () => void
}

export type DossierExportProjection = {
  projectId: string
  planRefs: string[]
  verdictRefs: string[]
  artifacts: { artifactId: string; sha256: string }[]
  verdicts: { outcome: string; evidenceReferences: string[] }[]
  generatedAt: number
  authority: 'projection-only'
}

export function createReviewService(opts: {
  acpCall?: AcpReviewCall
  /** Store for hash-mismatch validation at submit */
  previewStore?: PreviewFileStore
}): ReviewService {
  const history: ReviewVerdictProjection[] = []

  return {
    plan(req) {
      return planReview(req)
    },

    async submit(req) {
      const trusted = getTrustedPreviewContext()
      if (!trusted) {
        return { ok: false, reason: 'no trusted session — open a project before submitting review' }
      }

      const planned = planReview(req)
      if ('ok' in planned && planned.ok === false) {
        return planned
      }
      const plan = planned as ReviewPlan

      const access = assertReviewAccess(plan, trusted)
      if (!access.ok) {
        return { ok: false, reason: access.reason, plan }
      }

      // Hash-mismatch fail-closed: verify artifacts against store before submission
      if (opts.previewStore) {
        const resolved: ReviewEvidence[] = []
        for (const art of req.artifacts) {
          const record = await opts.previewStore.resolveById(art.artifactId)
          if (!record) {
            return {
              ok: false,
              reason: `artifact ${art.artifactId} not found in store — fail-closed`,
              plan,
            }
          }
          resolved.push({
            ...art,
            actualSha256: record.sha256,
          })
        }
        const hashCheck = validateArtifactHashes(resolved)
        if (!hashCheck.ok) {
          return {
            ok: false,
            reason: `hash mismatch: ${hashCheck.mismatches.map((m) => `${m.artifactId} expected=${m.expected} actual=${m.actual}`).join('; ')}`,
            plan,
            hashMismatches: hashCheck.mismatches,
          }
        }
      }

      // Check staleness against prior verdict
      const prev = history[history.length - 1]
      if (prev) {
        const { stale, mismatches } = isVerdictStale(prev, req.artifacts)
        if (stale) {
          prev.stale = true
        }
      }

      if (!opts.acpCall) {
        return {
          ok: false,
          reason: 'no ACP caller — cannot submit review without Lumen bridge',
          plan,
        }
      }

      try {
        const acpPayload = buildReviewAcpPayload(plan, req.artifacts, trusted, req.runId)
        const raw = await opts.acpCall('start_review', acpPayload as unknown as Record<string, unknown>)
        const verdict = normalizeReviewResult(raw, plan)
        history.push(verdict)
        return {
          ok: true,
          plan,
          verdict,
          authority: 'SessionActor/EvidenceGraph',
        }
      } catch (e: unknown) {
        return {
          ok: false,
          reason: (e as Error).message || String(e),
          plan,
        }
      }
    },

    history() {
      return [...history]
    },

    latest() {
      return history.length > 0 ? history[history.length - 1] : null
    },

    exportDossier() {
      const trusted = getTrustedPreviewContext()
      if (!trusted) {
        return { ok: false, reason: 'no trusted session for dossier export' }
      }
      const planRefs: string[] = []
      const verdictRefs: string[] = []
      const allArtifacts = new Map<string, string>()
      const allVerdicts: { outcome: string; evidenceReferences: string[] }[] = []

      for (const v of history) {
        planRefs.push(v.planRef)
        verdictRefs.push(v.verdictRef)
        allVerdicts.push({
          outcome: v.outcome,
          evidenceReferences: v.evidenceReferences,
        })
        for (let i = 0; i < v.artifactIds.length; i++) {
          allArtifacts.set(v.artifactIds[i], v.artifactHashes[i] ?? '')
        }
      }

      return {
        projectId: trusted.projectId,
        planRefs,
        verdictRefs,
        artifacts: [...allArtifacts.entries()].map(([artifactId, sha256]) => ({
          artifactId,
          sha256,
        })),
        verdicts: allVerdicts,
        generatedAt: Date.now(),
        authority: 'projection-only',
      }
    },

    checkStale(evidence) {
      const latest = history[history.length - 1]
      if (!latest) {
        return { ok: false, reason: 'no verdicts in history' }
      }
      return isVerdictStale(latest, evidence)
    },

    clear() {
      history.length = 0
    },
  }
}
