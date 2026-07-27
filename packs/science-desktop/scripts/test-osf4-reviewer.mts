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
import {
  setTrustedPreviewContext,
  clearTrustedPreviewContext,
} from '../src/main/files/session-identity.js'
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
  const empty = planReview({ artifacts: [] })
  await test('plan rejects empty artifacts', () => {
    ok('ok' in empty && empty.ok === false)
  })

  const badHash = planReview({ artifacts: [{ artifactId: 'a1', expectedSha256: 'short' }] })
  await test('plan rejects short sha256', () => {
    ok('ok' in badHash && badHash.ok === false)
  })

  const good = planReview({
    artifacts: [
      { artifactId: 'a1', expectedSha256: 'abc123def4567890abc123def4567890abc123de' },
      { artifactId: 'a2', expectedSha256: 'xyz789abc1234567xyz789abc1234567xyz789ab' },
    ],
  })
  await test('plan accepts valid evidence', () => {
    ok(!('ok' in good))
    const p = good as { artifactCount: number; authority: string }
    strictEqual(p.artifactCount, 2)
    strictEqual(p.authority, 'SessionActor/EvidenceGraph')
  })

  // ── Access gate ──────────────────────────────────────────────
  clearTrustedPreviewContext()
  const plan = planReview({
    artifacts: [{ artifactId: 'a1', expectedSha256: 'abc123def4567890abc123def4567890abc123de' }],
  }) as ReviewPlanFromTest
  const denied = assertReviewAccess(
    makeFullPlan(plan),
    null,
  )
  await test('access denied without session', () => ok(!denied.ok))

  setTrustedPreviewContext({ ownerId: 'o1', projectId: 'p1' })
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
  clearTrustedPreviewContext()
  setTrustedPreviewContext({ ownerId: 'o1', projectId: 'p1' })
  const acpP = buildReviewAcpPayload(
    makeFullPlan(plan),
    [{ artifactId: 'a1', expectedSha256: 'abc123def4567890abc123def4567890abc123de' }],
    { ownerId: 'o1', projectId: 'p1' },
    'run-1',
  )
  await test('acp payload has artifacts', () => {
    strictEqual(acpP.artifacts.length, 1)
    strictEqual(acpP.artifacts[0].artifact_id, 'a1')
    strictEqual(acpP.artifacts[0].expected_sha256, 'abc123def4567890abc123def4567890abc123de')
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
  store.put('a1', { path: '/s/a1', sha256: 'abc123def4567890abc123def4567890abc123de', ownerId: 'o1', projectId: 'p1' })

  // The mock asserts the REAL contract. `start_review` is a Go MCP tool the
  // registry rejects — the old mock accepted it, so this suite stayed green
  // while no submission had ever reached an engine. `review_record` RECORDS a
  // desktop-verified verdict under actor authority; it does not judge.
  const recorded: Record<string, unknown>[] = []
  const svc = createReviewService({
    acpCall: async (tool, args) => {
      acpCalls++
      strictEqual(tool, 'review_record')
      recorded.push(args)
      return {
        reviewer_id: args.reviewerId,
        verdict: args.verdict,
        project_id: args.projectId,
        notes: ['Reviewer operates under SessionActor authority only.'],
      }
    },
    previewStore: store,
    storeRoot: 'science-store',
  })

  clearTrustedPreviewContext()
  const noSess = await svc.submit({
    artifacts: [{ artifactId: 'a1', expectedSha256: 'abc123def4567890abc123def4567890abc123de' }],
  })
  await test('submit fails without session', () => {
    ok((noSess as { ok?: boolean }).ok === false)
  })

  // Hash mismatch: store has abc* but client sends wrong hash
  setTrustedPreviewContext({ ownerId: 'o1', projectId: 'p1' })
  const badSubmit = await svc.submit({
    artifacts: [{ artifactId: 'a1', expectedSha256: 'wrong1234567890123456789012345678xx' }],
  })
  await test('submit rejects hash mismatch via store', () => {
    ok((badSubmit as { ok?: boolean }).ok === false)
    ok(
      ((badSubmit as { reason?: string }).reason ?? '').includes('hash'),
    )
  })

  const submit = await svc.submit({
    artifacts: [{ artifactId: 'a1', expectedSha256: 'abc123def4567890abc123def4567890abc123de' }],
  })
  await test('submit succeeds with valid store hash', () => {
    ok((submit as { ok?: boolean }).ok)
    strictEqual(acpCalls, 1)
    // 'pass' is EARNED, not asserted: a hash miss or mismatch fails closed
    // before anything is recorded, so the only verdict that can reach the
    // engine is one whose validation succeeded.
    strictEqual(svc.latest()!.outcome, 'pass')
    const sent = recorded[0] as { verdict?: string; reviewerId?: string; projectId?: string }
    strictEqual(sent.verdict, 'pass')
    strictEqual(sent.reviewerId, 'o1')
    strictEqual(sent.projectId, 'p1')
  })

  // ── Dossier export projection ────────────────────────────────
  const doss = svc.exportDossier()
  await test('dossier export has artifacts + hashes + refs', () => {
    ok(!('ok' in doss && (doss as { ok: boolean }).ok === false))
    const d = doss as { projectId: string; verdictRefs: string[]; artifacts: { artifactId: string; sha256: string }[]; planRefs: string[] }
    strictEqual(d.projectId, 'p1')
    ok(d.verdictRefs.length >= 1)
    ok(d.planRefs.length >= 1)
    ok(d.artifacts.length >= 1)
    strictEqual(d.artifacts[0].artifactId, 'a1')
    strictEqual(d.artifacts[0].sha256, 'abc123def4567890abc123def4567890abc123de')
  })

  // ── Dossier gold path (shipped surfaces) ─────────────────────
  const fixture: DossierFixture = {
    projectId: 'dossier-gold-p1',
    question: 'Given disease X and target Y, generate a reproducible research dossier',
    plan: '1. Literature → 2. DB query → 3. Notebook → 4. Review → 5. Export',
    ownerId: 'local-user',
    artifacts: [
      { artifactId: 'lit-1', path: '/data/pubmed/41234568.json', sha256: 'lit1hash0123456789abcdef0123456789abc', ownerId: 'local-user', projectId: 'dossier-gold-p1', label: 'literature' },
      { artifactId: 'db-1', path: '/data/uniprot/P04637.fa', sha256: 'db1hash0123456789abcdef0123456789abc', ownerId: 'local-user', projectId: 'dossier-gold-p1', label: 'uniprot_protein' },
      { artifactId: 'nb-1', path: '/data/notebook/output.csv', sha256: 'nb1hash0123456789abcdef0123456789abc', ownerId: 'local-user', projectId: 'dossier-gold-p1', label: 'notebook_output' },
    ],
  }

  const catalog = new LocalProjectCatalog()
  catalog.create({ name: 'Dossier Gold', ownerId: 'local-user' })
  // Override id for fixture match
  const dsStore = new AcpPreviewStore()
  for (const a of fixture.artifacts) {
    dsStore.put(a.artifactId, { path: a.path, sha256: a.sha256, ownerId: a.ownerId, projectId: a.projectId })
  }

  // Use seeder store for submission
  const dsReviewSvc = createReviewService({
    acpCall: async () => ({
      report: { outcome: 'pass', artifacts: fixture.artifacts.map((a) => ({ artifact_id: a.artifactId, passed: true, reason: 'ok', expected_sha256: a.sha256 })), summary: 'all checks pass' },
    }),
    previewStore: dsStore,
  })

  clearTrustedPreviewContext()

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
type ReviewPlanFromTest = { planId: string; reviewId: string; artifactCount: number; evidenceFingerprint: string }
function makeFullPlan(p: ReviewPlanFromTest) {
  return {
    planId: p.planId, reviewId: p.reviewId, artifactCount: p.artifactCount,
    artifactIds: ['a1'], hashes: ['abc123def4567890abc123def4567890abc123de'],
    rubricVersion: 'lumen-v1.0', tool: 'start_review' as const,
    authority: 'SessionActor/EvidenceGraph' as const, requiresTrustedSession: true as const,
    createdAt: 0, evidenceFingerprint: p.evidenceFingerprint,
  }
}

run()
