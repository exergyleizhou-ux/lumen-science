#!/usr/bin/env npx tsx
/**
 * Offline dossier package projection tests.
 * Run: npx tsx scripts/test-dossier-package.mts
 */
import { ok, strictEqual } from 'node:assert/strict'
import { buildDossierPackage } from '../src/main/files/dossier-package.js'

let failures = 0
function test(name: string, fn: () => void) {
  try {
    fn()
    console.log(`OK  ${name}`)
  } catch (e: unknown) {
    failures++
    console.log(`FAIL ${name}: ${(e as Error).message}`)
  }
}

const pkg = buildDossierPackage({
  projectId: 'p-dossier',
  question: 'Given disease X and target Y, assemble a reproducible research dossier.',
  plan: '1. Literature\n2. UniProt/ChEMBL\n3. Notebook\n4. Review\n5. Export',
  artifacts: [
    { artifactId: 'lit-1', sha256: 'aa'.repeat(20), label: 'PubMed' },
    { artifactId: 'db-1', sha256: 'bb'.repeat(20), label: 'UniProt' },
    { artifactId: 'nb-1', sha256: 'cc'.repeat(20), label: 'analysis' },
  ],
  reviewVerdict: 'pass',
  planRefs: ['plan-1'],
  verdictRefs: ['verdict-1'],
})

test('package has required files', () => {
  const keys = Object.keys(pkg.files)
  for (const f of [
    'dossier.md',
    'evidence-graph.json',
    'review.json',
    'provenance.json',
    'replay-report.json',
    'artifacts/manifest.json',
  ]) {
    ok(keys.includes(f), `missing ${f}`)
  }
})

test('dossier.md contains question and artifacts', () => {
  ok(pkg.files['dossier.md'].includes('disease X'))
  ok(pkg.files['dossier.md'].includes('lit-1'))
  ok(pkg.files['dossier.md'].includes('SessionActor'))
})

test('evidence-graph has 3 nodes', () => {
  const g = JSON.parse(pkg.files['evidence-graph.json'])
  strictEqual(g.nodes.length, 3)
})

test('review.json pass + refs', () => {
  const r = JSON.parse(pkg.files['review.json'])
  strictEqual(r.verdict, 'pass')
  strictEqual(r.evidence_references.length, 3)
})

test('manifest sha256 recorded', () => {
  ok(pkg.sha256OfManifest.length === 64)
})

console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
process.exit(failures > 0 ? 1 : 0)
