#!/usr/bin/env npx tsx
/**
 * OSF-4 Reviewer + Dossier gold path tests.
 * Drives shipped modules: review-plan, review-service, dossier-service.
 * Run: npx tsx scripts/test-osf4-reviewer.mts
 */
import { strictEqual, ok } from 'node:assert/strict'
import fs from 'node:fs'
import {
  planReview,
  assertReviewAccess,
  normalizeReviewResult,
  hashEvidenceFingerprint,
  isVerdictStale,
  validateArtifactHashes,
  buildReviewAcpPayload,
} from '../src/main/files/review-plan.js'
import { createReviewService } from '../src/main/files/review-service.js'
import {
  runDossierGoldPath,
  type DossierFixture,
} from '../src/main/files/dossier-service.js'
import type { TrustedPreviewContext } from '../src/main/files/session-identity.js'
import {
  registerScienceIpcHandlers,
  type IpcMainLike,
  type SafeHandleFn,
} from '../src/main/files/science-ipc.js'
import { validateIpcChannel } from '../src/main/lumen-authority-policy.js'
import { LocalProjectCatalog } from '../src/main/files/local-project-catalog.js'
import { createOfflineCatalogMembershipAsserter } from '../src/main/files/hybrid-membership.js'
import { AcpPreviewStore } from '../src/main/files/acp-preview-store.js'
import { createNotebookService } from '../src/main/files/notebook-service.js'

const A_HASH = 'a'.repeat(64)
const B_HASH = 'b'.repeat(64)
const C_HASH = 'c'.repeat(64)
const SOURCE_RUN = 'review-source-run-1'
const REVIEW_SUMMARY = 'The cited artifact bytes satisfy the explicit fixture review rubric.'

let failures = 0
function test(name: string, fn: () => void | Promise<void>) {
  return Promise.resolve()
    .then(() => fn())
    .then(() => console.log(`OK  ${name}`))
    .catch((e: unknown) => {
      failures++
      console.log(`FAIL ${name}: ${(e as Error).message}`)
    })
}

const safeHandle: SafeHandleFn = (ipc, ch, h) => {
  if (!validateIpcChannel(ch)) throw new Error(`banned ${ch}`)
  ipc.handle(ch, h)
}

async function run() {
  // ── Pure plan ────────────────────────────────────────────────
  const empty = planReview({
    runId: SOURCE_RUN,
    verdict: 'pass',
    summary: REVIEW_SUMMARY,
    artifacts: [],
  })
  await test('plan rejects empty artifacts', () => {
    ok('ok' in empty && empty.ok === false)
  })

  const badHash = planReview({
    runId: SOURCE_RUN,
    verdict: 'pass',
    summary: REVIEW_SUMMARY,
    artifacts: [{ artifactId: 'a1', expectedSha256: 'short' }],
  })
  await test('plan rejects short sha256', () => {
    ok('ok' in badHash && badHash.ok === false)
  })

  const good = planReview({
    runId: SOURCE_RUN,
    verdict: 'pass',
    summary: REVIEW_SUMMARY,
    artifacts: [
      { artifactId: A_HASH, expectedSha256: A_HASH },
      { artifactId: B_HASH, expectedSha256: B_HASH },
    ],
  })
  await test('plan accepts valid evidence', () => {
    ok(!('ok' in good))
    const p = good as { artifactCount: number; authority: string }
    strictEqual(p.artifactCount, 2)
    strictEqual(p.authority, 'SessionActor/ReviewLedger')
  })

  // ── Access gate ──────────────────────────────────────────────
    const plan = planReview({
    runId: SOURCE_RUN,
    verdict: 'pass',
    summary: REVIEW_SUMMARY,
    artifacts: [{ artifactId: A_HASH, expectedSha256: A_HASH }],
  }) as ReviewPlanFromTest
  const denied = assertReviewAccess(
    makeFullPlan(plan),
    null,
  )
  await test('access denied without session', () => ok(!denied.ok))

    const allowed = assertReviewAccess(makeFullPlan(plan), { ownerId: 'o1', projectId: 'p1' })
  await test('access ok with session', () => ok(allowed.ok))

  // ── isVerdictStale ───────────────────────────────────────────
  const v1 = {
    reviewId: 'r1', planId: 'p1', outcome: 'pass' as const,
    summary: '', evidenceReferences: [],
    findings: [
      { artifactId: 'a1', passed: true, reason: 'ok', expectedSha256: 'abc' },
      { artifactId: 'a2', passed: true, reason: 'ok', expectedSha256: 'def' },
    ],
    supportCount: 2, contradictCount: 0, stale: false, reviewedAt: 1,
    artifactIds: ['a1', 'a2'], artifactHashes: ['abc', 'def'],
    planRef: 'p1', verdictRef: 'r1',
  }
  const { stale: notStale, mismatches: m0 } = isVerdictStale(v1, [
    { artifactId: 'a1', expectedSha256: 'abc' },
    { artifactId: 'a2', expectedSha256: 'def' },
  ])
  await test('stale: same evidence is not stale', () => {
    ok(!notStale)
    strictEqual(m0.length, 0)
  })

  const { stale: isStale, mismatches: m1 } = isVerdictStale(v1, [
    { artifactId: 'a1', expectedSha256: 'abc' },
    { artifactId: 'a2', expectedSha256: 'CHANGED' },
  ])
  await test('stale: changed hash is stale', () => {
    ok(isStale)
    ok(m1.length >= 1)
    ok(m1.some((m) => m.includes('CHANGED')))
  })

  const { stale: isStale2 } = isVerdictStale(v1, [
    { artifactId: 'a1', expectedSha256: 'abc' },
  ])
  await test('stale: missing artifact is stale', () => ok(isStale2))

  // ── Hash validation ──────────────────────────────────────────
  const hashOk = validateArtifactHashes([
    { artifactId: 'a1', expectedSha256: 'abc', actualSha256: 'abc' },
  ])
  await test('hash validate: match OK', () => ok(hashOk.ok))

  const hashBad = validateArtifactHashes([
    { artifactId: 'a1', expectedSha256: 'abc', actualSha256: 'wrong' },
  ])
  await test('hash validate: mismatch rejected', () => {
    ok(!hashBad.ok)
    strictEqual(hashBad.mismatches.length, 1)
  })

  // ── ACP payload includes artifacts ───────────────────────────
      const acpP = buildReviewAcpPayload(
    makeFullPlan(plan),
    [{ artifactId: A_HASH, expectedSha256: A_HASH }],
    { ownerId: 'o1', projectId: 'p1' },
  )
  await test('acp payload has artifacts', () => {
    strictEqual(acpP.artifacts.length, 1)
    strictEqual(acpP.run_id, SOURCE_RUN)
    strictEqual(acpP.artifacts[0].artifact_id, A_HASH)
    strictEqual(acpP.artifacts[0].expected_sha256, A_HASH)
  })

  // ── normOutcome: warn → warn ─────────────────────────────────
  const rawWarn = { report: { outcome: 'warn', artifacts: [], summary: 'x' } }
  const normW = normalizeReviewResult(rawWarn, makeFullPlan(plan))
  await test('normOutcome: warn returns warn', () => {
    strictEqual(normW.outcome, 'warn')
  })

  // ── Service submit with store hash validation ────────────────
  let acpCalls = 0
  const store = new AcpPreviewStore()
  store.put(A_HASH, {
    path: '/s/a1',
    sha256: A_HASH,
    ownerId: 'o1',
    projectId: 'p1',
    runId: SOURCE_RUN,
  })

  // The mock asserts the REAL contract. `start_review` is a Go MCP tool the
  // registry rejects — the old mock accepted it, so this suite stayed green
  // while no submission had ever reached an engine. The current contract
  // returns the typed mutation envelope and exact rehashed artifact set.
  const recorded: Record<string, unknown>[] = []
  const svc = createReviewService({
    acpCall: async (tool, args) => {
      acpCalls++
      strictEqual(tool, 'review_record')
      recorded.push(args)
      return {
        operationId: args.operationId,
        kind: 'review_record',
        projectId: args.projectId,
        replayed: false,
        runtimeAuthority: 'SessionActor-gated ACP adapter',
        result: {
          review_id: args.operationId,
          operation_id: args.operationId,
          reviewer_id: args.reviewerId,
          owner_id: args.ownerId,
          verdict: args.verdict,
          summary: args.summary,
          project_id: args.projectId,
          source_run_id: args.runId,
          authority_run_id: 'review-authority-run-1',
          evidence_fingerprint: C_HASH,
          artifacts: (args.artifactSha256s as string[]).map((sha256) => ({
            source_run_id: args.runId,
            sha256,
          })),
        },
      }
    },
    previewStore: store,
    storeRoot: 'science-store',
  })

    const noSess = await svc.submit({
    runId: SOURCE_RUN,
    verdict: 'pass',
    summary: REVIEW_SUMMARY,
    artifacts: [{ artifactId: A_HASH, expectedSha256: A_HASH }],
  }, null)
  await test('submit fails without session', () => {
    ok((noSess as { ok?: boolean }).ok === false)
  })

  // Hash mismatch: the content-addressed index key claims A but its record
  // claims B. The desktop catches this early; Rust rehashes again after Allow.
    store.put(A_HASH, {
    path: '/s/a1',
    sha256: B_HASH,
    ownerId: 'o1',
    projectId: 'p1',
    runId: SOURCE_RUN,
  })
  const badSubmit = await svc.submit({
    runId: SOURCE_RUN,
    verdict: 'pass',
    summary: REVIEW_SUMMARY,
    artifacts: [{ artifactId: A_HASH, expectedSha256: A_HASH }],
  }, { ownerId: "o1", projectId: "p1" })
  await test('submit rejects hash mismatch via store', () => {
    ok((badSubmit as { ok?: boolean }).ok === false)
    ok(
      ((badSubmit as { reason?: string }).reason ?? '').includes('hash'),
    )
  })

  store.put(A_HASH, {
    path: '/s/a1',
    sha256: A_HASH,
    ownerId: 'o1',
    projectId: 'p1',
    runId: SOURCE_RUN,
  })
  const submit = await svc.submit({
    runId: SOURCE_RUN,
    verdict: 'pass',
    summary: REVIEW_SUMMARY,
    artifacts: [{ artifactId: A_HASH, expectedSha256: A_HASH }],
  }, { ownerId: "o1", projectId: "p1" })
  await test('submit succeeds with valid store hash', () => {
    ok((submit as { ok?: boolean }).ok)
    strictEqual(acpCalls, 1)
    // 'pass' is EARNED, not asserted: a hash miss or mismatch fails closed
    // before anything is recorded, so the only verdict that can reach the
    // engine is one whose validation succeeded.
    strictEqual(svc.latest()!.outcome, 'pass')
    strictEqual(svc.latest()!.supportCount, 0)
    strictEqual(svc.latest()!.contradictCount, 0)
    strictEqual(svc.latest()!.findings[0]?.passed, null)
    const sent = recorded[0] as {
      verdict?: string
      reviewerId?: string
      projectId?: string
      ownerId?: string
      runId?: string
      artifactSha256s?: string[]
      operationId?: string
      summary?: string
    }
    strictEqual(sent.verdict, 'pass')
    strictEqual(sent.reviewerId, 'o1')
    strictEqual(sent.ownerId, 'o1')
    strictEqual(sent.projectId, 'p1')
    strictEqual(sent.runId, SOURCE_RUN)
    strictEqual(sent.summary, REVIEW_SUMMARY)
    strictEqual(sent.artifactSha256s?.[0], A_HASH)
    ok(Boolean(sent.operationId))
  })

  const looseResponseService = createReviewService({
    acpCall: async (_tool, args) => ({
      runtimeAuthority: 'SessionActor-gated ACP adapter',
      kind: 'review_record',
      result: {
        project_id: args.projectId,
        owner_id: args.ownerId,
        reviewer_id: args.reviewerId,
        verdict: args.verdict,
        summary: args.summary,
        source_run_id: args.runId,
        artifacts: (args.artifactSha256s as string[]).map((sha256) => ({ sha256 })),
      },
    }),
    previewStore: store,
  })
  const looseResponse = await looseResponseService.submit({
    runId: SOURCE_RUN,
    verdict: 'pass',
    summary: REVIEW_SUMMARY,
    artifacts: [{ artifactId: A_HASH, expectedSha256: A_HASH }],
  }, { ownerId: "o1", projectId: "p1" })
  await test('submit rejects legacy loose review response', () => {
    strictEqual((looseResponse as { ok?: boolean }).ok, false)
  })

  // ── Dossier export projection ────────────────────────────────
  const doss = svc.exportDossier({ ownerId: "o1", projectId: "p1" })
  await test('dossier export has artifacts + hashes + refs', () => {
    ok(!('ok' in doss && (doss as { ok: boolean }).ok === false))
    const d = doss as { projectId: string; verdictRefs: string[]; artifacts: { artifactId: string; sha256: string }[]; planRefs: string[] }
    strictEqual(d.projectId, 'p1')
    ok(d.verdictRefs.length >= 1)
    ok(d.planRefs.length >= 1)
    ok(d.artifacts.length >= 1)
    strictEqual(d.artifacts[0].artifactId, A_HASH)
    strictEqual(d.artifacts[0].sha256, A_HASH)
  })

  // ── Dossier gold path (shipped surfaces) ─────────────────────
  const fixture: DossierFixture = {
    projectId: 'dossier-gold-p1',
    runId: SOURCE_RUN,
    question: 'Given disease X and target Y, generate a reproducible research dossier',
    plan: '1. Literature → 2. DB query → 3. Notebook → 4. Review → 5. Export',
    ownerId: 'local-user',
    artifacts: [
      { artifactId: A_HASH, path: '/data/pubmed/41234568.json', sha256: A_HASH, ownerId: 'local-user', projectId: 'dossier-gold-p1', label: 'literature' },
      { artifactId: B_HASH, path: '/data/uniprot/P04637.fa', sha256: B_HASH, ownerId: 'local-user', projectId: 'dossier-gold-p1', label: 'uniprot_protein' },
      { artifactId: C_HASH, path: '/data/notebook/output.csv', sha256: C_HASH, ownerId: 'local-user', projectId: 'dossier-gold-p1', label: 'notebook_output' },
    ],
  }

  const catalog = new LocalProjectCatalog()
  catalog.create({ name: 'Dossier Gold', ownerId: 'local-user' })
  // Override id for fixture match
  const dsStore = new AcpPreviewStore()
  for (const a of fixture.artifacts) {
    dsStore.put(a.artifactId, {
      path: a.path,
      sha256: a.sha256,
      ownerId: a.ownerId,
      projectId: a.projectId,
      runId: SOURCE_RUN,
    })
  }

  // Use seeder store for submission
  const dsReviewSvc = createReviewService({
    acpCall: async (_tool, args) => ({
      operationId: args.operationId,
      kind: 'review_record',
      projectId: args.projectId,
      replayed: false,
      runtimeAuthority: 'SessionActor-gated ACP adapter',
      result: {
        review_id: args.operationId,
        operation_id: args.operationId,
        reviewer_id: args.reviewerId,
        owner_id: args.ownerId,
        verdict: 'pass',
        summary: args.summary,
        project_id: args.projectId,
        source_run_id: args.runId,
        authority_run_id: 'review-authority-run-2',
        evidence_fingerprint: C_HASH,
        artifacts: (args.artifactSha256s as string[]).map((sha256) => ({
          source_run_id: args.runId,
          sha256,
        })),
      },
    }),
    previewStore: dsStore,
  })


  const dossierResult = await runDossierGoldPath(fixture, {
    catalog,
    previewStore: dsStore,
    assertMembership: async (c) => (c.ownerId === 'local-user' ? { ok: true, ownerId: 'local-user', projectId: c.projectId } : { ok: false, reason: 'denied' }),
    notebookService: createNotebookService({}),
    reviewService: dsReviewSvc,
  })

  await test('dossier: all steps', () => {
    ok(dossierResult.stepResults.length >= 7, `got ${dossierResult.stepResults.length}`)
  })
  const failed = dossierResult.stepResults.filter((s) => !s.ok)
  await test('dossier: no failed steps', () => {
    strictEqual(
      failed.length,
      0,
      `failed steps: ${failed.map((s) => `${s.step}: ${s.metadata.reason ?? ''}`).join(', ')}`,
    )
  })
  await test('dossier: export projection non-empty', () => {
    ok(Object.keys(dossierResult.exportProjection).length > 0)
  })

  // ── Stubs still stubs ────────────────────────────────────────
  const ipcStub = fs.readFileSync('src/main/reviewer/ipc.ts', 'utf-8')
  await test('reviewer/ipc still STUB', () => {
    ok(ipcStub.includes('EXECUTION AUTHORITY REMOVED'))
  })

  // ── IPC registration ─────────────────────────────────────────
  for (const ch of ['review:plan', 'review:submit', 'review:history', 'review:latest', 'review:export-dossier']) {
    await test(`policy allows ${ch}`, () => ok(validateIpcChannel(ch)))
  }
  await test('still bans reviewer:run', () => strictEqual(validateIpcChannel('reviewer:run'), false))
  await test('still bans reviewer:abort-fix-loop', () => strictEqual(validateIpcChannel('reviewer:abort-fix-loop'), false))

  const handlers = new Map<string, Function>()
  const ipc: IpcMainLike = { handle(ch, h) { if (handlers.has(ch)) throw new Error(`dup ${ch}`); handlers.set(ch, h) } }
  registerScienceIpcHandlers(ipc, {
    safeHandle, getLumenBinaryHash: () => 'h', previewStore: dsStore,
    assertMembership: createOfflineCatalogMembershipAsserter({ catalog }),
    projectCatalog: catalog, reviewService: svc,
  })
  await test('ipc registers review channels', () => {
    ok(handlers.has('review:plan'))
    ok(handlers.has('review:submit'))
  })

  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

// Helper types
type ReviewPlanFromTest = {
  planId: string
  reviewId: string
  artifactCount: number
  evidenceFingerprint: string
  sourceRunId?: string
  verdict?: 'pass'
  summary?: string
}
function makeFullPlan(p: ReviewPlanFromTest) {
  return {
    planId: p.planId, reviewId: p.reviewId, artifactCount: p.artifactCount,
    artifactIds: [A_HASH], hashes: [A_HASH], sourceRunId: p.sourceRunId ?? SOURCE_RUN,
    verdict: p.verdict ?? 'pass', summary: p.summary ?? REVIEW_SUMMARY,
    rubricVersion: 'lumen-v1.0', tool: 'review_record' as const,
    authority: 'SessionActor/ReviewLedger' as const, requiresTrustedSession: true as const,
    createdAt: 0, evidenceFingerprint: p.evidenceFingerprint,
  }
}

run()
