#!/usr/bin/env npx tsx
/**
 * OSF-4 Reviewer + Dossier gold path tests.
 * Run: npx tsx scripts/test-osf4-reviewer.mts
 */
import { strictEqual, ok } from 'node:assert/strict'
import fs from 'node:fs'
import {
  planReview,
  assertReviewAccess,
  normalizeReviewResult,
  hashEvidenceFingerprint,
} from '../src/main/files/review-plan.js'
import { createReviewService } from '../src/main/files/review-service.js'
import { createDossierRunner, type DossierFixture } from '../src/main/files/dossier-service.js'
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
import { createHybridMembershipAsserter } from '../src/main/files/hybrid-membership.js'
import { AcpPreviewStore } from '../src/main/files/acp-preview-store.js'

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
  }) as { planId: string; reviewId: string; artifactCount: number }
  const denied = assertReviewAccess(
    {
      planId: plan.planId,
      reviewId: plan.reviewId,
      artifactCount: plan.artifactCount,
      artifactIds: ['a1'],
      hashes: ['abc123def4567890abc123def4567890abc123de'],
      rubricVersion: 'lumen-v1.0',
      tool: 'start_review',
      authority: 'SessionActor/EvidenceGraph',
      requiresTrustedSession: true,
      createdAt: 0,
      evidenceFingerprint: 'fp',
    },
    null,
  )
  await test('access denied without session', () => ok(!denied.ok))

  setTrustedPreviewContext({ ownerId: 'o1', projectId: 'p1' })
  const allowed = assertReviewAccess(
    {
      planId: 'p',
      reviewId: 'r',
      artifactCount: 1,
      artifactIds: ['a1'],
      hashes: ['abc123def4567890abc123def4567890abc123de'],
      rubricVersion: 'lumen-v1.0',
      tool: 'start_review',
      authority: 'SessionActor/EvidenceGraph',
      requiresTrustedSession: true,
      createdAt: 0,
      evidenceFingerprint: 'fp',
    },
    { ownerId: 'o1', projectId: 'p1' },
  )
  await test('access ok with session', () => ok(allowed.ok))

  // ── Service submit (mock ACP) ────────────────────────────────
  let acpCalls = 0
  const svc = createReviewService({
    acpCall: async (tool, args) => {
      acpCalls++
      strictEqual(tool, 'start_review')
      return {
        report: {
          outcome: 'pass',
          artifacts: [
            {
              artifact_id: 'a1',
              passed: true,
              reason: 'hash matches',
              expected_sha256: 'abc123def4567890abc123def4567890abc123de',
            },
          ],
          summary: 'all evidence verified',
        },
      }
    },
  })

  clearTrustedPreviewContext()
  const noSess = await svc.submit({
    artifacts: [{ artifactId: 'a1', expectedSha256: 'abc123def4567890abc123def4567890abc123de' }],
  })
  await test('submit fails without session', () => {
    ok((noSess as { ok?: boolean }).ok === false)
    strictEqual(acpCalls, 0)
  })

  setTrustedPreviewContext({ ownerId: 'o1', projectId: 'p1' })
  const submit = await svc.submit({
    artifacts: [{ artifactId: 'a1', expectedSha256: 'abc123def4567890abc123def4567890abc123de' }],
  })
  await test('submit succeeds with session', () => {
    ok((submit as { ok?: boolean }).ok)
    strictEqual(acpCalls, 1)
    strictEqual(svc.history().length, 1)
    strictEqual(svc.latest()!.outcome, 'pass')
  })

  const doss = svc.exportDossier()
  await test('dossier export after review', () => {
    ok(!('ok' in doss && (doss as { ok: boolean }).ok === false))
    const d = doss as { projectId: string; verdicts: unknown[] }
    strictEqual(d.projectId, 'p1')
    strictEqual(d.verdicts.length, 1)
  })

  // ── Dossier gold path (fixture) ──────────────────────────────
  const fixture: DossierFixture = {
    projectId: 'dossier-p1',
    question: 'Given disease X and target Y, generate a reproducible research dossier',
    plan: '1. Literature search → 2. Biological DB query → 3. Notebook analysis → 4. Review → 5. Export',
    artifacts: [
      { artifactId: 'lit-1', path: '/data/pubmed/41234568.json', sha256: 'lit1hash0123456789abcdef0123456789abc', label: 'PubMed literature' },
      { artifactId: 'db-1', path: '/data/uniprot/P04637.fa', sha256: 'db1hash0123456789abcdef0123456789abc', label: 'UniProt protein record' },
      { artifactId: 'nb-1', path: '/data/notebook/output.csv', sha256: 'nb1hash0123456789abcdef0123456789abc', label: 'Notebook CSV output' },
    ],
  }

  const runner = createDossierRunner(fixture)
  await test('dossier: question', () => strictEqual(runner.runQuestion(), fixture.question))
  await test('dossier: plan', () => strictEqual(runner.runPlan(), fixture.plan))
  await test('dossier: literature ok', () => {
    runner.runLiterature()
    const s = runner.getSteps().find((s) => s.step === 'literature')!
    ok(s.ok)
    strictEqual(s.metadata.count as number, 3)
  })
  await test('dossier: database', () => {
    runner.runDatabase()
    ok(runner.getSteps().find((s) => s.step === 'database')!.ok)
  })
  await test('dossier: notebook', () => {
    runner.runNotebook(true)
    ok(runner.getSteps().find((s) => s.step === 'notebook')!.ok)
  })
  await test('dossier: review', () => {
    runner.runReview('pass')
    ok(runner.getSteps().find((s) => s.step === 'review')!.ok)
  })
  const exportRes = runner.export()
  await test('dossier: export complete', () => {
    strictEqual(exportRes.artifactIds.length, 3)
    strictEqual(exportRes.steps.length, 7)
    strictEqual(exportRes.reproducibilityLevel, 'fixture')
    strictEqual(exportRes.reviewVerdict, 'pass')
  })

  // ── Stubs still stubs ────────────────────────────────────────
  const ipcStub = fs.readFileSync('src/main/reviewer/ipc.ts', 'utf-8')
  await test('reviewer/ipc still STUB', () => {
    ok(ipcStub.includes('EXECUTION AUTHORITY REMOVED'))
    ok(ipcStub.includes('registerReviewerIpcHandlers'))
  })

  // ── IPC registration ─────────────────────────────────────────
  for (const ch of ['review:plan', 'review:submit', 'review:history', 'review:latest', 'review:export-dossier']) {
    await test(`policy allows ${ch}`, () => ok(validateIpcChannel(ch)))
  }

  const handlers = new Map<string, Function>()
  const ipc: IpcMainLike = {
    handle(ch, h) {
      if (handlers.has(ch)) throw new Error(`dup ${ch}`)
      handlers.set(ch, h)
    },
  }
  const catalog = new LocalProjectCatalog()
  clearTrustedPreviewContext()
  registerScienceIpcHandlers(ipc, {
    safeHandle,
    getLumenBinaryHash: () => 'h',
    previewStore: new AcpPreviewStore(),
    assertMembership: createHybridMembershipAsserter({ catalog }),
    projectCatalog: catalog,
    reviewService: svc,
  })

  await test('ipc registers review channels', () => {
    ok(handlers.has('review:plan'))
    ok(handlers.has('review:submit'))
    ok(handlers.has('review:export-dossier'))
  })

  await test('still bans reviewer:run', () => {
    strictEqual(validateIpcChannel('reviewer:run'), false)
  })
  await test('still bans reviewer:abort-fix-loop', () => {
    strictEqual(validateIpcChannel('reviewer:abort-fix-loop'), false)
  })

  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

run()
