/**
 * OSF-4 Reviewer product service.
 *
 * Plan / validate locally; record via the engine's `review_record`;
 * hash-mismatch fail-closed at submission time. Dossier export includes
 * artifact IDs, hashes, and plan/verdict refs.
 *
 * ## Why review_record, and what the verdict means
 *
 * Submission used to call `start_review` — a Go MCP tool the method registry
 * rejects, so no submission had ever reached an engine. The Rust engine's real
 * method is `review_record`: it does not judge, it RECORDS a verdict under
 * SessionActor authority.
 *
 * Desktop validation is only an early UX failure. The judgment and rationale
 * are explicit user input; the authoritative commit happens in Rust after
 * permission. SessionActor reopens the cited succeeded run and ProjectStore
 * hashes every registered artifact again before committing the review,
 * evidence manifest, and provenance.
 */

import {
  planReview,
  assertReviewAccess,
  normalizeReviewResult,
  validateArtifactHashes,
  isVerdictStale,
  type ReviewRequest,
  type ReviewPlan,
  type ReviewVerdictProjection,
  type ReviewEvidence,
} from './review-plan'
import type { TrustedPreviewContext } from './session-identity'
import type { PreviewFileStore } from './preview-resolver'

export type AcpReviewCall = (
  toolName: string,
  args: Record<string, unknown>,
) => Promise<unknown>

export type ReviewService = {
  plan: (req: ReviewRequest) => ReturnType<typeof planReview>
  submit: (
    req: ReviewRequest,
    trusted: TrustedPreviewContext | null,
  ) => Promise<unknown>
  history: () => ReviewVerdictProjection[]
  latest: () => ReviewVerdictProjection | null
  exportDossier: (
    trusted: TrustedPreviewContext | null,
  ) => DossierExportProjection | { ok: false; reason: string }
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
  /** Mandatory for hash-mismatch validation at submit — fail-closed when absent */
  previewStore: PreviewFileStore
  storeRoot?: string
}): ReviewService {
  const history: ReviewVerdictProjection[] = []

  return {
    plan(req) {
      return planReview(req)
    },

    async submit(req, trusted) {
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

      // Hash-mismatch fail-closed: verify artifacts against store
      // — store is mandatory; if unavailable, reject the submission.
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
        if (
          record.ownerId !== trusted.ownerId ||
          record.projectId !== trusted.projectId ||
          record.runId !== req.runId
        ) {
          return {
            ok: false,
            reason: `artifact ${art.artifactId} is not bound to the trusted owner/project/source run`,
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

      // Check staleness against prior verdict
      const prev = history[history.length - 1]
      if (prev) {
        // Only the staleness verdict is consumed here; the per-artifact `mismatches` detail is
        // reported by the hash-mismatch branch above, which has the authoritative list.
        const { stale } = isVerdictStale(prev, req.artifacts)
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
        const response = (await opts.acpCall('review_record', {
          storeRoot: opts.storeRoot ?? 'science-store',
          projectId: trusted.projectId,
          ownerId: trusted.ownerId,
          reviewerId: trusted.ownerId,
          verdict: req.verdict,
          summary: req.summary,
          runId: req.runId,
          artifactSha256s: resolved.map((artifact) => artifact.actualSha256 as string),
          operationId: plan.reviewId,
          ...(req.claimId ? { claimId: req.claimId } : {}),
        })) as Record<string, unknown>
        if (
          response?.runtimeAuthority !== 'SessionActor-gated ACP adapter' ||
          response?.kind !== 'review_record' ||
          !response?.result ||
          typeof response.result !== 'object'
        ) {
          throw new Error('review_record returned no actor-owned durable mutation result')
        }
        const record = response.result as Record<string, unknown>
        const recordArtifacts = Array.isArray(record.artifacts)
          ? (record.artifacts as Record<string, unknown>[])
          : []
        const returnedHashes = recordArtifacts.map((artifact) => String(artifact.sha256 ?? '')).sort()
        const expectedHashes = resolved.map((artifact) => String(artifact.actualSha256 ?? '')).sort()
        if (
          response.operationId !== plan.reviewId ||
          response.projectId !== trusted.projectId ||
          record.review_id !== plan.reviewId ||
          record.operation_id !== plan.reviewId ||
          record.project_id !== trusted.projectId ||
          record.owner_id !== trusted.ownerId ||
          record.reviewer_id !== trusted.ownerId ||
          record.source_run_id !== req.runId ||
          typeof record.authority_run_id !== 'string' ||
          !/^[A-Za-z0-9_-]{1,128}$/.test(record.authority_run_id) ||
          typeof record.evidence_fingerprint !== 'string' ||
          !/^[a-f0-9]{64}$/.test(record.evidence_fingerprint) ||
          record.verdict !== req.verdict ||
          record.summary !== req.summary ||
          recordArtifacts.some((artifact) => artifact.source_run_id !== req.runId) ||
          returnedHashes.length !== expectedHashes.length ||
          returnedHashes.some((hash, index) => hash !== expectedHashes[index])
        ) {
          throw new Error('review_record durable result does not match the trusted evidence request')
        }

        // Projection only after the engine confirms the durable record.
        const verdict = normalizeReviewResult(
          {
            reviewer_id: record.reviewer_id,
            report: {
              outcome: record.verdict,
              summary: String(record.summary),
              artifacts: resolved.map((a) => ({
                artifact_id: a.artifactId,
                passed: null,
                reason: 'integrity verified; no per-artifact scientific judgment recorded',
                expected_sha256: a.expectedSha256,
                actual_sha256: a.actualSha256,
              })),
            },
          },
          plan,
        )
        history.push(verdict)
        return {
          ok: true,
          plan,
          verdict,
          record,
          authority: 'SessionActor/ReviewLedger',
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

    exportDossier(trusted) {
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
