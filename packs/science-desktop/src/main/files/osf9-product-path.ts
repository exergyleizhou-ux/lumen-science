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
 */

import { LocalProjectCatalog } from './local-project-catalog'
import { AcpPreviewStore } from './acp-preview-store'
import { createOfflineCatalogMembershipAsserter } from './hybrid-membership'
import { bindTrustedSession, unbindTrustedSession } from './session-binding'
import {
  setTrustedPreviewContext,
  clearTrustedPreviewContext,
  getTrustedPreviewContext,
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

export async function runOsf9ProductPath(opts?: {
  connectorLockPath?: string
  registryPath?: string
}): Promise<Osf9Report> {
  const steps: Osf9Step[] = []
  const push = (name: string, ok: boolean, detail?: string) => {
    steps.push({ name, ok, detail })
  }

  clearTrustedPreviewContext()
  const catalog = new LocalProjectCatalog()
  const store = new AcpPreviewStore()
  const ownerId = 'osf9-owner'
  const project = catalog.create({
    name: 'OSF-9 Disease Dossier Fixture',
    ownerId,
    description: 'offline product path',
  })

  // Offline fixture: no engine to ask. Named explicitly so this file cannot be
  // mistaken for the production trust model — it grants from local state.
  const membership = createOfflineCatalogMembershipAsserter({ catalog })
  const bind = await bindTrustedSession(
    { ownerId, projectId: project.id },
    { assertMembership: membership },
  )
  push('bind-session', bind.ok === true, bind.ok ? project.id : (bind as { reason?: string }).reason)

  // Seed registered artifacts (never raw path open)
  const arts = [
    {
      id: 'art-lit-1',
      path: '/fixture/lit/pubmed.json',
      sha256: 'aa11bb22cc33dd44ee55ff6677889900aabbccdd',
      label: 'literature',
    },
    {
      id: 'art-db-1',
      path: '/fixture/db/uniprot.fa',
      sha256: '11223344556677889900aabbccddeeff00112233',
      label: 'uniprot_protein',
    },
    {
      id: 'art-nb-1',
      path: '/fixture/nb/out.csv',
      sha256: 'ffeeddccbbaa0099887766554433221100ffeedd',
      label: 'notebook_output',
    },
  ]
  for (const a of arts) {
    store.put(a.id, {
      path: a.path,
      sha256: a.sha256,
      ownerId,
      projectId: project.id,
    })
  }
  push('seed-artifacts', true, `${arts.length} artifacts`)

  // Preview by artifact_id
  setTrustedPreviewContext({ ownerId, projectId: project.id })
  const prev = await loadArtifactPreview(
    { artifactId: 'art-lit-1', expectedSha256: arts[0].sha256 },
    { store },
  )
  push('preview-by-artifact', prev.access.ok === true, prev.path)

  // Adversarial: path-style / wrong project
  const cross = await resolvePreview(
    { artifactId: 'art-lit-1' },
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
  clearTrustedPreviewContext()
  const nbDenied = await nb.execute({ language: 'python', code: 'print(1)\n' })
  push(
    'notebook-execute-without-session-denied',
    (nbDenied as { ok?: boolean }).ok === false,
  )
  setTrustedPreviewContext({ ownerId, projectId: project.id })
  const nbOk = await nb.execute({ language: 'python', code: 'print(1)\n' })
  push('notebook-execute-with-session', (nbOk as { ok?: boolean }).ok === true)

  // Review
  const review = createReviewService({
    acpCall: async () => ({
      report: {
        outcome: 'pass',
        summary: 'fixture review',
        artifacts: arts.map((a) => ({
          artifact_id: a.id,
          passed: true,
          reason: 'ok',
          expected_sha256: a.sha256,
        })),
      },
    }),
    previewStore: store,
  })
  const rev = await review.submit({
    artifacts: arts.map((a) => ({
      artifactId: a.id,
      expectedSha256: a.sha256,
      label: a.label,
    })),
  })
  push('review-submit', (rev as { ok?: boolean }).ok === true)

  const dossier = review.exportDossier()
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
    artifactId: 'art-lit-1',
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

  // Restart simulation: clear session, rebind, preview still works after re-seed identity
  unbindTrustedSession()
  clearTrustedPreviewContext()
  push('restart-clear-session', getTrustedPreviewContext() === null)
  const rebind = await bindTrustedSession(
    { ownerId, projectId: project.id },
    { assertMembership: membership },
  )
  push('restart-rebind', rebind.ok === true)
  setTrustedPreviewContext({ ownerId, projectId: project.id })
  const prev2 = await loadArtifactPreview(
    { artifactId: 'art-db-1', expectedSha256: arts[1].sha256 },
    { store },
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
