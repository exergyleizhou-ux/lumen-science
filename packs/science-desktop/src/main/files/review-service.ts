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
 * The judgment itself happens here, and it is exactly one check: every cited
 * artifact resolves in the content-addressed store and its actual hash equals
 * the expected hash. Any miss or mismatch fails closed BEFORE anything is
 * recorded, so the verdict that reaches the engine is 'pass' by construction —
 * not because reviews cannot fail, but because a failed validation refuses to
 * produce a record at all. What is recorded is an attestation: "these exact
 * bytes were reviewed", bound to the project by the actor.
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
  /** Mandatory for hash-mismatch validation at submit — fail-closed when absent */
  previewStore: PreviewFileStore
  storeRoot?: string
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
        const record = (await opts.acpCall('review_record', {
          storeRoot: opts.storeRoot ?? 'science-store',
          projectId: trusted.projectId,
          reviewerId: trusted.ownerId,
          // Earned above, not asserted: a hash miss or mismatch has already
          // returned before this line.
          verdict: 'pass',
          ...(req.claimId ? { claimId: req.claimId } : {}),
        })) as { verdict?: string; notes?: string[] }

        // The projection is built from facts this process verified plus the
        // engine's own record — not parsed back out of a foreign response
        // shape. `resolved` carries the actual hashes read from the store.
        const verdict = normalizeReviewResult(
          {
            report: {
              outcome: record?.verdict ?? 'pass',
              summary: (record?.notes ?? []).join(' ') || 'Artifact hashes verified against the content-addressed store.',
              artifacts: resolved.map((a) => ({
                artifact_id: a.artifactId,
                passed: true,
                reason: 'hash verified',
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
