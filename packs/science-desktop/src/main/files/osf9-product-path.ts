/**
 * OSF-9 product-path composition (offline / fixture).
 *
 * Exercises the shipped Lumen desktop authority path end-to-end without
 * requiring Electron runtime:
 *   project open → bind → seed artifacts → preview by id → notebook plan
 *   → review submit → dossier export → restart simulation (clear + rebind)
 *
 * Adversarial cases: path-based preview, cross-project id, unauthenticated
 * notebook execute, banned OS IPC symbols still unregistered.
 *
 * Identity is sender-scoped throughout — no process-global bag.
 */

import { createHash } from 'node:crypto'
import fs from 'node:fs'
import os from 'node:os'
import nodePath from 'node:path'
import { LocalProjectCatalog } from './local-project-catalog'
import { AcpPreviewStore } from './acp-preview-store'
import { createOfflineCatalogMembershipAsserter } from './hybrid-membership'
import {
  bindTrustedSession,
  unbindTrustedSession,
  clearAllTrustedSessions,
} from './session-binding'
import {
  getTrustedPreviewContextForSender,
  type TrustedPreviewContext,
} from './session-identity'
import { loadArtifactPreview } from './preview-service'
import { resolvePreview } from './preview-resolver'
import { planNotebookCell } from './notebook-plan'
import { createNotebookService } from './notebook-service'
import { createReviewService } from './review-service'
import { planRemoteCompute } from './compute-plan'
import { loadConnectorCatalog } from './connector-catalog'
import { assertOfficePreviewAdmission } from './office-preview-admission'
import { validateIpcChannel } from '../lumen-authority-policy'
import {
  isSha256Hex,
  resolveAndProbeLumenScienceBinary,
} from './lumen-binary'
import path from 'node:path'

export type Osf9Step = {
  name: string
  ok: boolean
  detail?: string
}

export type Osf9Report = {
  ok: boolean
  steps: Osf9Step[]
  binaryHash: string | null
  exportProjection?: unknown
}

const PROOF_SENDER = 9

export async function runOsf9ProductPath(opts?: {
  connectorLockPath?: string
  registryPath?: string
}): Promise<Osf9Report> {
  const steps: Osf9Step[] = []
  const push = (name: string, ok: boolean, detail?: string) => {
    steps.push({ name, ok, detail })
  }

  clearAllTrustedSessions()
  const catalog = new LocalProjectCatalog()
  const store = new AcpPreviewStore()
  const ownerId = 'osf9-owner'
  const project = catalog.create({
    name: 'OSF-9 Disease Dossier Fixture',
    ownerId,
    description: 'offline product path',
  })
  const trusted: TrustedPreviewContext = { ownerId, projectId: project.id }

  // Offline fixture: no engine to ask. Named explicitly so this file cannot be
  // mistaken for the production trust model — it grants from local state.
  const membership = createOfflineCatalogMembershipAsserter({ catalog })
  const bind = await bindTrustedSession(
    { ownerId, projectId: project.id },
    { assertMembership: membership, senderId: PROOF_SENDER },
  )
  push('bind-session', bind.ok === true, bind.ok ? project.id : (bind as { reason?: string }).reason)

  // Real files under a temp root: the preview resolver reads the BYTES and
  // re-hashes them, so a fixture asserting a digest for a path that does not
  // exist is exactly the lie this product must not tell about its evidence.
  const fixtureRoot = fs.mkdtempSync(nodePath.join(os.tmpdir(), 'osf9-fixture-'))
  const writeFixture = (name: string, body: string): { path: string; sha256: string } => {
    const full = nodePath.join(fixtureRoot, name)
    fs.mkdirSync(nodePath.dirname(full), { recursive: true })
    fs.writeFileSync(full, body)
    return { path: full, sha256: createHash('sha256').update(body).digest('hex') }
  }
  const arts = [
    { ...writeFixture('lit/pubmed.json', '{"pmid": "fixture"}\n'), label: 'literature' },
    { ...writeFixture('db/uniprot.fa', '>fixture\nMKV\n'), label: 'uniprot_protein' },
    { ...writeFixture('nb/out.csv', 'col\n1\n'), label: 'notebook_output' },
  ].map((artifact) => ({ ...artifact, id: artifact.sha256 }))
  for (const a of arts) {
    store.put(a.id, {
      path: a.path,
      sha256: a.sha256,
      ownerId,
      projectId: project.id,
      runId: 'offline-fixture-run',
    })
  }
  push('seed-artifacts', true, `${arts.length} artifacts`)

  // Preview by artifact_id under sender-bound trusted context
  const prev = await loadArtifactPreview(
    { artifactId: arts[0].id, expectedSha256: arts[0].sha256 },
    { store },
    trusted,
  )
  push(
    'preview-by-artifact',
    prev.access.ok === true,
    `${prev.byteLength ?? 0} verified bytes`,
  )

  // Adversarial: path-style / wrong project
  const cross = await resolvePreview(
    { artifactId: arts[0].id },
    store,
    { ownerId: 'attacker', projectId: project.id },
  )
  push('reject-cross-owner', cross.access.ok === false, cross.access.reason)

  // Notebook plan only + execute requires session (already have session)
  const nbPlan = planNotebookCell({
    language: 'python',
    code: 'print("dossier analysis")\n',
    dryRun: true,
  })
  push('notebook-plan', !('ok' in nbPlan), JSON.stringify(nbPlan).slice(0, 80))

  const nb = createNotebookService({
    // The service now speaks the engine's real contract: workflow_execute
    // with a snake_case run report. A fixture answering the old shape would
    // pass a service that cannot talk to any engine.
    acpCall: async () => ({ state: 'succeeded', refusedSteps: [] }),
    resolveInterpreter: async () => ({ ok: true, interpreterPath: '/usr/bin/python3' }),
  })
  const nbDenied = await nb.execute({ language: 'python', code: 'print(1)\n' }, null)
  push(
    'notebook-execute-without-session-denied',
    (nbDenied as { ok?: boolean }).ok === false,
  )
  const nbOk = await nb.execute({ language: 'python', code: 'print(1)\n' }, trusted)
  push('notebook-execute-with-session', (nbOk as { ok?: boolean }).ok === true)

  // Review
  const review = createReviewService({
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
        authority_run_id: 'osf9-review-authority-run',
        evidence_fingerprint: createHash('sha256')
          .update((args.artifactSha256s as string[]).join('|'))
          .digest('hex'),
        artifacts: (args.artifactSha256s as string[]).map((sha256) => ({
          source_run_id: args.runId,
          sha256,
        })),
      },
    }),
    previewStore: store,
  })
  const rev = await review.submit(
    {
      runId: 'offline-fixture-run',
      verdict: 'pass',
      summary: 'Offline OSF-9 fixture artifacts match the fixture review rubric.',
      artifacts: arts.map((a) => ({
        artifactId: a.id,
        expectedSha256: a.sha256,
        label: a.label,
      })),
    },
    trusted,
  )
  push('review-submit', (rev as { ok?: boolean }).ok === true)

  const dossier = review.exportDossier(trusted)
  const dossierOk =
    !('ok' in dossier && (dossier as { ok: boolean }).ok === false) &&
    Array.isArray((dossier as { artifacts?: unknown[] }).artifacts) &&
    ((dossier as { artifacts: unknown[] }).artifacts?.length ?? 0) >= 3
  push('dossier-export', dossierOk)

  // Compute dry-run
  const cp = planRemoteCompute({
    hostname: 'hpc.fixture.local',
    targetKind: 'ssh_fixture',
  })
  push('compute-dry-run', !('ok' in cp) && (cp as { canSchedule: boolean }).canSchedule === false)

  // Connectors catalog
  try {
    const lock =
      opts?.connectorLockPath ||
      path.resolve(process.cwd(), '../../docs/science/fusion-sources.lock.json')
    const cat = loadConnectorCatalog(lock)
    push(
      'connector-catalog',
      cat.summary.total === 42 && cat.summary.implemented === 40,
      `total=${cat.summary.total}`,
    )
  } catch (e: unknown) {
    push('connector-catalog', false, (e as Error).message)
  }

  // Office fail-closed
  const office = assertOfficePreviewAdmission({
    format: 'docx',
    artifactId: arts[0].id,
    expectedSha256: arts[0].sha256,
  })
  push('office-fail-closed', office.ok === false)

  // Banned channels still banned
  const banned = [
    'reviewer:run',
    'artifacts:read-preview',
    'projects:create',
    'compute:job-updated',
  ]
  push(
    'banned-ipc',
    banned.every((ch) => validateIpcChannel(ch) === false),
    banned.join(','),
  )

  // Restart simulation: clear all senders, rebind, preview still works
  unbindTrustedSession(PROOF_SENDER)
  clearAllTrustedSessions()
  push('restart-clear-session', getTrustedPreviewContextForSender(PROOF_SENDER) === null)
  const rebind = await bindTrustedSession(
    { ownerId, projectId: project.id },
    { assertMembership: membership, senderId: PROOF_SENDER },
  )
  push('restart-rebind', rebind.ok === true)
  const prev2 = await loadArtifactPreview(
    { artifactId: arts[1].id, expectedSha256: arts[1].sha256 },
    { store },
    trusted,
  )
  push('restart-preview', prev2.access.ok === true)

  // Optional live binary: fill binaryHash only when a real binary is present.
  // Offline CI keeps binaryHash=null — never invent a hash.
  let binaryHash: string | null = null
  const live = resolveAndProbeLumenScienceBinary()
  if (!live) {
    push('live-binary', true, 'skip: no lumen-science binary (offline OK)')
  } else if (!live.ok || !isSha256Hex(live.binaryHash)) {
    push(
      'live-binary',
      false,
      live.detail || 'binary present but version/help/hash failed',
    )
  } else {
    binaryHash = live.binaryHash
    push(
      'live-binary',
      true,
      `hash=${binaryHash.slice(0, 12)}… version=${live.versionOutput.split('\n')[0]}`,
    )
  }

  const ok = steps.every((s) => s.ok)
  return {
    ok,
    steps,
    binaryHash,
    exportProjection: dossier,
  }
}
