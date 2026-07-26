/**
 * Offline Dossier package projection for Target/Disease wedge.
 * Produces the file set expected by the product plan (offline fixtures).
 */

import { createHash, randomUUID } from 'node:crypto'

export type DossierPackageInput = {
  projectId: string
  question: string
  plan: string
  artifacts: { artifactId: string; sha256: string; label?: string }[]
  reviewVerdict?: string
  planRefs?: string[]
  verdictRefs?: string[]
  notebookCellHashes?: string[]
}

export type DossierPackageFiles = {
  'dossier.md': string
  'evidence-graph.json': string
  'review.json': string
  'provenance.json': string
  'replay-report.json': string
  'artifacts/manifest.json': string
}

export function buildDossierPackage(input: DossierPackageInput): {
  packageId: string
  files: DossierPackageFiles
  sha256OfManifest: string
} {
  const packageId = randomUUID()
  const now = new Date().toISOString()

  const dossierMd = [
    `# Research Dossier`,
    ``,
    `**Package:** ${packageId}`,
    `**Project:** ${input.projectId}`,
    `**Generated:** ${now}`,
    ``,
    `## Question`,
    ``,
    input.question,
    ``,
    `## Plan`,
    ``,
    input.plan,
    ``,
    `## Artifacts`,
    ``,
    ...input.artifacts.map(
      (a) => `- \`${a.artifactId}\` sha256=\`${a.sha256}\` ${a.label || ''}`,
    ),
    ``,
    `## Review`,
    ``,
    input.reviewVerdict || 'pending',
    ``,
    `## Authority`,
    ``,
    `Rust SessionActor is sole execution authority. This package is a projection.`,
    ``,
  ].join('\n')

  const evidenceGraph = {
    project_id: input.projectId,
    nodes: input.artifacts.map((a) => ({
      node_id: a.artifactId,
      kind: 'SourceArtifact',
      artifact_sha256: a.sha256,
      label: a.label || a.artifactId,
    })),
    edges: (input.verdictRefs || []).map((v) => ({
      relation: 'ReviewedBy',
      target: v,
      supporting_artifact_sha256: input.artifacts[0]?.sha256 || '',
    })),
  }

  const review = {
    verdict: input.reviewVerdict || 'inconclusive',
    plan_refs: input.planRefs || [],
    verdict_refs: input.verdictRefs || [],
    evidence_references: input.artifacts.map((a) => a.artifactId),
  }

  const provenance = {
    package_id: packageId,
    project_id: input.projectId,
    generated_at: now,
    reproducibility_level: 'fixture',
    authority: 'SessionActor',
    open_science_pin: 'd8f11e34314fdfa36f750cdb617af1cc2f30bace',
  }

  const replay = {
    package_id: packageId,
    restart_reopen: 'required',
    fixture_replay: 'required',
    steps: [
      'bind-session',
      'preview-artifact',
      'notebook-plan',
      'review-submit',
      'export',
    ],
    status: 'offline-fixture-ready',
  }

  const artManifest = {
    artifacts: input.artifacts,
    notebook_cell_hashes: input.notebookCellHashes || [],
  }

  const files: DossierPackageFiles = {
    'dossier.md': dossierMd,
    'evidence-graph.json': JSON.stringify(evidenceGraph, null, 2) + '\n',
    'review.json': JSON.stringify(review, null, 2) + '\n',
    'provenance.json': JSON.stringify(provenance, null, 2) + '\n',
    'replay-report.json': JSON.stringify(replay, null, 2) + '\n',
    'artifacts/manifest.json': JSON.stringify(artManifest, null, 2) + '\n',
  }

  const sha256OfManifest = createHash('sha256')
    .update(files['artifacts/manifest.json'])
    .digest('hex')

  return { packageId, files, sha256OfManifest }
}
