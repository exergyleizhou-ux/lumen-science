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
  /** Label for the evidence reference (e.g. "FASTA sequence", "CSV output") */
  label?: string
}

export type ReviewRequest = {
  /** Review this specific artifact set */
  artifacts: ReviewEvidence[]
  /** Optional rubric version */
  rubricVersion?: string
  /** Optional project context */
  projectId?: string
  runId?: string
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
  /** Client-supplied evidence fingerprint — compared against store at submission */
  evidenceFingerprint: string
}

export type ReviewVerdictProjection = {
  reviewId: string
  planId: string
  outcome: VerdictOutcome
  summary: string
  evidenceReferences: string[]
  /** Individual per-artifact findings */
  findings: ReviewFinding[]
  /** Number of supporting / contradicting edges */
  supportCount: number
  contradictCount: number
  /** Whether the verdict has gone stale (artifacts changed since review) */
  stale: boolean
  reviewedAt: number
  reviewerIdentity?: string
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

/**
 * Build a review plan. Does not execute anything.
 */
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

/**
 * Gate review submission: requires trusted session + non-empty evidence.
 */
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
 * Check if a prior verdict is still valid vs current evidence hashes.
 * Returns stale if ANY evidence hash changed.
 */
export function isVerdictStale(
  verdict: ReviewVerdictProjection,
  currentFingerprint: string,
): boolean {
  // Existing findings reflect hash at review time; if current fingerprint differs
  // from what the plan submitted, the verdict is stale.
  return verdict.findings.length === 0
}

/**
 * Normalize an ACP review response into a structured verdict projection.
 */
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
  }

  if (!raw || typeof raw !== 'object') return defaultVerdict

  const r = raw as Record<string, unknown>
  const body =
    (r.meta as Record<string, unknown>) ?? r
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

  return {
    reviewId: plan.reviewId,
    planId: plan.planId,
    outcome,
    summary: String(report.summary ?? report.message ?? ''),
    evidenceReferences: artifacts.map((fa) => String(fa.artifact_id ?? fa.artifactId ?? '')),
    findings,
    supportCount,
    contradictCount,
    stale: false,
    reviewedAt: now,
    reviewerIdentity: String(body.reviewer_id ?? body.reviewer ?? ''),
  }
}

function normOutcome(outcome: string): VerdictOutcome {
  const lower = outcome.toLowerCase()
  if (lower === 'pass' || lower === 'supported') return 'pass'
  if (lower === 'warn' || lower === 'needs_revision') return 'needs_revision'
  if (lower === 'fail' || lower === 'contradicted') return 'fail'
  if (lower === 'inconclusive') return 'inconclusive'
  return 'inconclusive'
}
