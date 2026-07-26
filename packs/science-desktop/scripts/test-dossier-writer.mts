#!/usr/bin/env npx tsx
/**
 * Round-trip test: export a dossier, then verify it with the INDEPENDENT verifier.
 *
 * This is the only test in the repo that closes the loop. Everything else
 * checks one side: the exporter produces plausible files, or the verifier
 * rejects tampered ones. Neither says the two agree.
 *
 * They did not. `buildDossierPackage` emits digests and no artifact bytes, and
 * nothing wrote it to disk at all, so an exported dossier substantiated none of
 * the digests it listed — and the verifier, finding no bytes, printed PASS.
 * That combination is worse than either bug alone.
 *
 * So this shells out to `scripts/verify-dossier.py` — the real one, the same
 * file a stranger would run — and requires a genuine pass, not an absence of
 * errors.
 *
 *   npx tsx scripts/test-dossier-writer.mts
 */
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

import { writeDossier, type ArtifactSource } from '../src/main/files/dossier-writer.js'

const HERE = path.dirname(fileURLToPath(import.meta.url))
const REPO = path.resolve(HERE, '../../..')
const VERIFIER = path.join(REPO, 'scripts/verify-dossier.py')

let passed = 0
const failures: string[] = []

function check(label: string, condition: boolean, detail = ''): void {
  if (condition) {
    passed += 1
    console.log(`  ok    ${label}`)
  } else {
    failures.push(label)
    console.log(`  FAIL  ${label}${detail ? ` — ${detail}` : ''}`)
  }
}

const sha256 = (b: Buffer): string => createHash('sha256').update(b).digest('hex')

type VerifierResult = { verdict: string; failed: { check: string }[]; unverifiable: string[] }

function runVerifier(dir: string): VerifierResult {
  try {
    const out = execFileSync('python3', [VERIFIER, dir, '--json'], { encoding: 'utf8' })
    return JSON.parse(out) as VerifierResult
  } catch (error: unknown) {
    const stdout = (error as { stdout?: string }).stdout
    if (stdout) return JSON.parse(stdout) as VerifierResult
    return { verdict: 'unreadable', failed: [], unverifiable: [] }
  }
}

/** A package in the shape buildDossierPackage produces, with real artifacts. */
async function fixture(root: string): Promise<{ files: Record<string, string>; sources: ArtifactSource[] }> {
  const blobs = path.join(root, 'blobs')
  await mkdir(blobs, { recursive: true })

  const sources: ArtifactSource[] = []
  for (const [name, content] of [
    ['input', Buffer.from('col_a,col_b\n1,2\n')],
    ['result', Buffer.from('{"mean": 1.5}\n')]
  ] as const) {
    const digest = sha256(content)
    const p = path.join(blobs, name)
    await writeFile(p, content)
    sources.push({ artifactId: `art-${name}`, sha256: digest, path: p })
  }

  const files = {
    'dossier.md': '# Research Dossier\n',
    'evidence-graph.json': JSON.stringify({
      nodes: [
        { id: 'n1', sha256: sources[0].sha256 },
        { id: 'n2', sha256: sources[1].sha256 },
        { id: 'claim1' }
      ],
      edges: [
        { from: 'n1', to: 'n2' },
        { from: 'n2', to: 'claim1' }
      ]
    }),
    'review.json': JSON.stringify({ outcome: 'pass' }),
    'provenance.json': JSON.stringify({
      environment: {
        interpreter: '/usr/bin/python3',
        version: '3.11.9',
        sha256: 'a'.repeat(64),
        os: 'darwin-arm64'
      },
      policyHash: 'b'.repeat(64)
    }),
    'replay-report.json': JSON.stringify({ outcome: 'identical' }),
    'artifacts/manifest.json': JSON.stringify({
      artifacts: sources.map((s) => ({ artifactId: s.artifactId, sha256: s.sha256 }))
    })
  }
  return { files, sources }
}

async function main(): Promise<void> {
  console.log('test-dossier-writer')

  if (!existsSync(VERIFIER)) {
    console.error(`FAIL: verifier missing at ${VERIFIER}`)
    process.exit(1)
  }

  const root = await mkdtemp(path.join(tmpdir(), 'dossier-writer-'))
  try {
    const { files, sources } = await fixture(root)
    const readArtifact = async (s: ArtifactSource): Promise<Buffer> => readFile(s.path)

    // ── the round trip ──────────────────────────────────────────
    const out = path.join(root, 'export')
    const result = await writeDossier(out, { files } as never, sources, {
      readArtifact,
      verifierPath: VERIFIER
    })

    check('every artifact was written', result.artifactsWritten === 2, String(result.artifactsWritten))
    check('the verifier travels with the package', result.verifierIncluded)
    check('nothing was omitted', result.artifactsOmitted.length === 0)

    const verdict = runVerifier(out)
    check(
      'the INDEPENDENT verifier passes the exported dossier',
      verdict.verdict === 'pass',
      JSON.stringify(verdict.failed)
    )
    // Before this exporter existed, a package would pass while substantiating
    // nothing. Assert the byte check actually ran.
    check(
      'the verifier re-hashed real bytes, rather than finding none',
      !verdict.failed.some((f) => f.check.includes('artifact bytes to verify')) &&
        !verdict.unverifiable.some((u) => u.includes('not in the package')),
      JSON.stringify(verdict.unverifiable)
    )

    // ── an unavailable artifact is recorded, not silently dropped ──
    const partialDir = path.join(root, 'partial')
    const partial = await writeDossier(partialDir, { files } as never, sources, {
      readArtifact: async (s) => {
        if (s.artifactId === 'art-input') throw new Error('bytes are gone')
        return readFile(s.path)
      }
    })
    check('an unreadable artifact is recorded as omitted', partial.artifactsOmitted.length === 1)
    check(
      'the omission is written where a reader will see it',
      existsSync(path.join(partialDir, 'artifacts', 'OMITTED.json'))
    )
    check('a partial export still verifies', runVerifier(partialDir).verdict === 'pass')

    // ── a digest that does not match its bytes aborts the export ──
    let aborted = false
    try {
      await writeDossier(path.join(root, 'bad'), { files } as never, [
        { artifactId: 'art-lying', sha256: 'c'.repeat(64), path: sources[0].path }
      ], { readArtifact })
    } catch (error: unknown) {
      aborted = /hashes to/.test((error as Error).message)
    }
    check(
      'an artifact whose bytes contradict its digest aborts the export',
      aborted,
      'shipping it would fail verification and look like tampering in transit'
    )

    // ── a non-canonical digest never becomes a filename ──
    const shortDir = path.join(root, 'short')
    const short = await writeDossier(shortDir, { files } as never, [
      { artifactId: 'art-short', sha256: 'abc123', path: sources[0].path }
    ], { readArtifact })
    check(
      'a truncated digest is refused rather than used as a path',
      short.artifactsWritten === 0 && short.artifactsOmitted.length === 1
    )
  } finally {
    await rm(root, { recursive: true, force: true })
  }

  if (failures.length > 0) {
    console.error(`\nFAILED: ${failures.length} of ${passed + failures.length}`)
    process.exit(1)
  }
  console.log(`\nALL DOSSIER ROUND-TRIP TESTS PASSED (${passed} checks)`)
}

await main()
