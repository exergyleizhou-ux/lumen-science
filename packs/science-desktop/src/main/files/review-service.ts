/**
 * OSF-4 Reviewer product service.
 *
 * Plan / validate locally; submit via ACP start_review; list/history
 * from in-memory projection. Never restores TypeScript orchestrator.
 */

import {
  planReview,
  assertReviewAccess,
  normalizeReviewResult,
  hashEvidenceFingerprint,
  type ReviewRequest,
  type ReviewPlan,
  type ReviewVerdictProjection,
} from './review-plan'
import { getTrustedPreviewContext } from './session-identity'

export type AcpReviewCall = (
  toolName: string,
  args: Record<string, unknown>,
) => Promise<unknown>

export type ReviewService = {
  plan: (req: ReviewRequest) => ReturnType<typeof planReview>
  submit: (req: ReviewRequest) => Promise<unknown>
  history: () => ReviewVerdictProjection[]
  latest: () => ReviewVerdictProjection | null
  /** Export a dossier manifest (artifactIds + plan/verdict refs) */
  exportDossier: () => {
    projectId: string
    verdicts: ReviewVerdictProjection[]
    planRefs: string[]
    generatedAt: number
  } | { ok: false; reason: string }
  clear: () => void
}

export function createReviewService(opts: {
  acpCall?: AcpReviewCall
}): ReviewService {
  const history: ReviewVerdictProjection[] = []

  return {
    plan(req) {
      return planReview(req)
    },

    async submit(req) {
      const planned = planReview(req)
      if ('ok' in planned && planned.ok === false) {
        return planned
      }
      const plan = planned as ReviewPlan
      const trusted = getTrustedPreviewContext()
      const access = assertReviewAccess(plan, trusted)
      if (!access.ok) {
        return { ok: false, reason: access.reason, plan }
      }
      if (!opts.acpCall) {
        return {
          ok: false,
          reason: 'no ACP caller — cannot submit review without Lumen bridge',
          plan,
        }
      }

      try {
        const raw = await opts.acpCall('start_review', {
          project_id: trusted!.projectId,
          run_id: req.runId || 'default',
          plan_id: plan.planId,
        })
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
      return {
        projectId: trusted.projectId,
        verdicts: [...history],
        planRefs: history.map((v) => v.planId),
        generatedAt: Date.now(),
      }
    },

    clear() {
      history.length = 0
    },
  }
}
