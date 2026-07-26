/**
 * OSF Dossier gold path — composition over OSF-2/3/4 surfaces.
 *
 * Fixture-backed Target/Disease Research Dossier that exercises
 * Question→Plan→Evidence→Result→Review end-to-end on Lumen IPC.
 *
 * Does NOT import any banned OS execution path.
 */

import { randomUUID } from 'node:crypto'

export type DossierStep =
  | 'question'
  | 'plan'
  | 'literature'
  | 'database'
  | 'notebook'
  | 'review'
  | 'export'

export type DossierFixture = {
  projectId: string
  question: string
  plan: string
  /** Artifacts seeded into preview store */
  artifacts: {
    artifactId: string
    path: string
    sha256: string
    label: string
  }[]
}

export type DossierStepResult = {
  step: DossierStep
  ok: boolean
  metadata: Record<string, unknown>
  warnings: string[]
}

export type DossierVerdictRef = string

export type DossierExport = {
  dossierId: string
  projectId: string
  question: string
  plan: string
  steps: DossierStepResult[]
  artifactIds: string[]
  reviewVerdict?: string
  generatedAt: number
  reproducibilityLevel: 'fixture' | 'replay' | 'independent'
}

export function createDossierRunner(fixture: DossierFixture) {
  const steps: DossierStepResult[] = []
  const warnings: string[] = []

  const addStep = (
    step: DossierStep,
    ok: boolean,
    meta: Record<string, unknown> = {},
    wrn: string[] = [],
  ) => {
    steps.push({ step, ok, metadata: meta, warnings: wrn })
    warnings.push(...wrn)
  }

  return {
    runQuestion() {
      addStep('question', Boolean(fixture.question && fixture.question.length > 10), {
        question: fixture.question,
      })
      return fixture.question
    },

    runPlan() {
      addStep('plan', Boolean(fixture.plan && fixture.plan.length > 5), {
        plan: fixture.plan,
      })
      return fixture.plan
    },

    runLiterature() {
      const hasArtifacts = fixture.artifacts.length >= 2
      const litWarnings: string[] = []
      if (!hasArtifacts) litWarnings.push('fewer than 2 literature artifacts seeded')
      addStep(
        'literature',
        hasArtifacts,
        { count: fixture.artifacts.length },
        litWarnings,
      )
    },

    runDatabase() {
      const dbArti = fixture.artifacts.filter(
        (a) => a.label.toLowerCase().includes('gene') || a.label.toLowerCase().includes('protein'),
      )
      addStep('database', true, {
        databasesQueried: dbArti.length,
        artifactIds: dbArti.map((a) => a.artifactId),
      })
    },

    runNotebook(pythonOk: boolean = true) {
      addStep('notebook', pythonOk, {
        cells: 3,
        kernel: 'SessionActor/KernelAdapter',
        notebookExecuted: pythonOk,
      })
    },

    runReview(verdict: 'pass' | 'warn' | 'fail' | 'inconclusive' = 'pass') {
      addStep('review', verdict !== 'fail', {
        verdict,
        artifactCount: fixture.artifacts.length,
        evidenceRefs: fixture.artifacts.map((a) => a.artifactId),
      })
    },

    export(): DossierExport {
      addStep('export', steps.filter((s) => !s.ok).length === 0, {
        totalSteps: steps.length,
        failingSteps: steps.filter((s) => !s.ok).length,
      })
      return {
        dossierId: randomUUID(),
        projectId: fixture.projectId,
        question: fixture.question,
        plan: fixture.plan,
        steps: [...steps],
        artifactIds: fixture.artifacts.map((a) => a.artifactId),
        reviewVerdict: steps.find((s) => s.step === 'review')?.ok
          ? 'pass'
          : 'inconclusive',
        generatedAt: Date.now(),
        reproducibilityLevel: 'fixture',
      }
    },

    getSteps() {
      return [...steps]
    },
  }
}
