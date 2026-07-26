/**
 * OSF-4 Reviewer plan — pure module (no Electron, no fix-loop spawn).
 *
 * Builds a review request that can only be fulfilled by Lumen ACP
 * start_review / review_status. TypeScript orchestrator stays stubbed.
 */

import { createHash, randomUUID } from 'node:crypto'
import type { AccessResult } from '../lumen-authority-policy'
import type { TrustedPreviewContext } from './session-identity'

export type VerdictOutcome = 'pass' | 'warn' | 'fail' | 'needs_revision' | 'inconclusive'

export type ReviewEvidence = {
  artifactId: string
  expectedSha256: string
  mimeType?: string
  label?: string
  /** Actual hash fetched from durable store at submit time */
  actualSha256?: string
}

export type ReviewRequest = {
  artifacts: ReviewEvidence[]
  rubricVersion?: string
  projectId?: string
  runId?: string
  /** Claim this review supports/contradicts, when the reviewer names one. */
  claimId?: string
}

export type ReviewPlan = {
  planId: string
  reviewId: string
  artifactCount: number
  artifactIds: string[]
  hashes: string[]
  rubricVersion: string
  tool: 'start_review'
  authority: 'SessionActor/EvidenceGraph'
  requiresTrustedSession: true
  createdAt: number
  evidenceFingerprint: string
}

export type ReviewVerdictProjection = {
  reviewId: string
  planId: string
  outcome: VerdictOutcome
  summary: string
  evidenceReferences: string[]
  findings: ReviewFinding[]
  supportCount: number
  contradictCount: number
  stale: boolean
  reviewedAt: number
  reviewerIdentity?: string
  /** Artifact IDs and hashes from the plan for dossier export */
  artifactIds: string[]
  artifactHashes: string[]
  planRef: string
  verdictRef: string
}

export type ReviewFinding = {
  artifactId: string
  passed: boolean
  reason: string
  expectedSha256: string
  actualSha256?: string
}

const DEFAULT_RUBRIC = 'lumen-v1.0'

export function hashEvidenceFingerprint(artifacts: ReviewEvidence[]): string {
  const payload = artifacts
    .map((a) => `${a.artifactId}:${a.expectedSha256}`)
    .sort()
    .join('|')
  return createHash('sha256').update(payload, 'utf8').digest('hex')
}

export function planReview(req: ReviewRequest): ReviewPlan | { ok: false; reason: string } {
  if (!req.artifacts || req.artifacts.length === 0) {
    return { ok: false, reason: 'at least one artifact is required' }
  }
  for (const a of req.artifacts) {
    if (!a.artifactId || !a.expectedSha256) {
      return { ok: false, reason: 'artifact_id and expected_sha256 are required for each artifact' }
    }
    if (a.expectedSha256.length < 16) {
      return { ok: false, reason: 'expected_sha256 too short' }
    }
  }
  return {
    planId: randomUUID(),
    reviewId: randomUUID(),
    artifactCount: req.artifacts.length,
    artifactIds: req.artifacts.map((a) => a.artifactId),
    hashes: req.artifacts.map((a) => a.expectedSha256),
    rubricVersion: req.rubricVersion || DEFAULT_RUBRIC,
    tool: 'start_review',
    authority: 'SessionActor/EvidenceGraph',
    requiresTrustedSession: true,
    createdAt: Date.now(),
    evidenceFingerprint: hashEvidenceFingerprint(req.artifacts),
  }
}

export function assertReviewAccess(
  plan: ReviewPlan,
  trusted: TrustedPreviewContext | null,
): AccessResult {
  if (plan.artifactCount === 0) {
    return { ok: false, reason: 'empty evidence set' }
  }
  if (!trusted?.ownerId || !trusted?.projectId) {
    return {
      ok: false,
      reason: 'no trusted session — open a project before submitting review',
    }
  }
  if (plan.authority !== 'SessionActor/EvidenceGraph') {
    return { ok: false, reason: 'invalid authority claim' }
  }
  return { ok: true }
}

/**
 * Check if a prior verdict is stale relative to current evidence.
 * A verdict is stale when: (a) findings count differs from evidence count,
 * (b) any evidence sha256 has changed, or (c) any finding's expected hash
 * no longer matches the current fingerprint.
 */
export function isVerdictStale(
  verdict: ReviewVerdictProjection,
  currentEvidence: ReviewEvidence[],
): { stale: boolean; mismatches: string[] } {
  const mismatches: string[] = []

  if (verdict.findings.length !== currentEvidence.length) {
    mismatches.push(
      `finding count changed: verdict=${verdict.findings.length} current=${currentEvidence.length}`,
    )
  }

  const evidenceMap = new Map(currentEvidence.map((e) => [e.artifactId, e]))
  const findingMap = new Map(verdict.findings.map((f) => [f.artifactId, f]))

  for (const [id, ev] of evidenceMap) {
    const f = findingMap.get(id)
    if (!f) {
      mismatches.push(`artifact ${id} not in prior verdict`)
      continue
    }
    if (f.expectedSha256 !== ev.expectedSha256) {
      mismatches.push(`hash changed for ${id}: verdict=${f.expectedSha256} current=${ev.expectedSha256}`)
    }
  }

  // New artifacts not in prior verdict
  for (const id of evidenceMap.keys()) {
    if (!findingMap.has(id)) {
      mismatches.push(`new artifact ${id} not in prior verdict`)
    }
  }

  return { stale: mismatches.length > 0, mismatches }
}

/**
 * Build the ACP start_review payload: project, run, and every artifact hash.
 */
export function buildReviewAcpPayload(
  plan: ReviewPlan,
  artifacts: ReviewEvidence[],
  trusted: TrustedPreviewContext,
  runId?: string,
): { project_id: string; run_id: string; plan_id: string; artifacts: { artifact_id: string; expected_sha256: string }[] } {
  return {
    project_id: trusted.projectId,
    run_id: runId || 'default',
    plan_id: plan.planId,
    artifacts: artifacts.map((a) => ({
      artifact_id: a.artifactId,
      expected_sha256: a.expectedSha256,
    })),
  }
}

/**
 * Validate store-supplied actual hashes against expected hashes.
 * Fail-closed: any mismatch rejects the whole submission.
 */
export function validateArtifactHashes(
  artifacts: ReviewEvidence[],
): { ok: boolean; mismatches: { artifactId: string; expected: string; actual: string }[] } {
  const mismatches = artifacts
    .filter((a) => a.actualSha256 && a.actualSha256 !== a.expectedSha256)
    .map((a) => ({
      artifactId: a.artifactId,
      expected: a.expectedSha256,
      actual: a.actualSha256 || '',
    }))
  return { ok: mismatches.length === 0, mismatches }
}

export function normalizeReviewResult(
  raw: unknown,
  plan: ReviewPlan,
): ReviewVerdictProjection {
  const now = Date.now()
  const defaultVerdict: ReviewVerdictProjection = {
    reviewId: plan.reviewId,
    planId: plan.planId,
    outcome: 'inconclusive',
    summary: 'Review completed but raw output was unparseable',
    evidenceReferences: [],
    findings: plan.artifactIds.map((id, i) => ({
      artifactId: id,
      passed: false,
      reason: 'raw result unparseable',
      expectedSha256: plan.hashes[i] ?? '',
    })),
    supportCount: 0,
    contradictCount: 0,
    stale: false,
    reviewedAt: now,
    artifactIds: plan.artifactIds,
    artifactHashes: plan.hashes,
    planRef: plan.planId,
    verdictRef: plan.reviewId,
  }

  if (!raw || typeof raw !== 'object') return defaultVerdict

  const r = raw as Record<string, unknown>
  const body = (r.meta as Record<string, unknown>) ?? r
  const report = (body.report as Record<string, unknown>) ?? body

  const outcome = normOutcome(String(report.outcome ?? report.pass ?? ''))
  const findings: ReviewFinding[] = []
  const artifacts = Array.isArray(report.artifacts)
    ? (report.artifacts as Record<string, unknown>[])
    : Array.isArray(report.findings)
      ? (report.findings as Record<string, unknown>[])
      : []

  let supportCount = 0
  let contradictCount = 0

  for (const fa of artifacts) {
    const passed = Boolean(fa.passed ?? fa.ok ?? (outcome === 'pass'))
    if (passed) supportCount++
    else contradictCount++
    findings.push({
      artifactId: String(fa.artifact_id ?? fa.artifactId ?? ''),
      passed,
      reason: String(fa.reason ?? fa.detail ?? ''),
      expectedSha256: String(fa.expected_sha256 ?? fa.sha256 ?? ''),
      actualSha256: String(fa.actual_sha256 ?? fa.actual ?? ''),
    })
  }

  // Determine stale by comparing plan evidence fingerprint against findings
  const currentEvidence = plan.artifactIds.map((id, i) => ({
    artifactId: id,
    expectedSha256: plan.hashes[i] ?? '',
  }))
  const { stale } = isVerdictStale(
    {
      ...defaultVerdict,
      findings,
    },
    currentEvidence,
  )

  return {
    reviewId: plan.reviewId,
    planId: plan.planId,
    outcome,
    summary: String(report.summary ?? report.message ?? ''),
    evidenceReferences: artifacts.map((fa) => String(fa.artifact_id ?? fa.artifactId ?? '')),
    findings,
    supportCount,
    contradictCount,
    stale,
    reviewedAt: now,
    reviewerIdentity: String(body.reviewer_id ?? body.reviewer ?? ''),
    artifactIds: plan.artifactIds,
    artifactHashes: plan.hashes,
    planRef: plan.planId,
    verdictRef: plan.reviewId,
  }
}

function normOutcome(outcome: string): VerdictOutcome {
  const lower = outcome.toLowerCase()
  if (lower === 'pass' || lower === 'supported') return 'pass'
  if (lower === 'warn' || lower === 'warning') return 'warn'
  if (lower === 'needs_revision' || lower === 'needs revision') return 'needs_revision'
  if (lower === 'fail' || lower === 'contradicted') return 'fail'
  if (lower === 'inconclusive' || lower === 'inconclusive') return 'inconclusive'
  return 'inconclusive'
}
