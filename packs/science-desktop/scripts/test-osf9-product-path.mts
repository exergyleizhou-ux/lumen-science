#!/usr/bin/env npx tsx
/**
 * OSF-9 product-path + adversarial tests (shipped composition).
 * Run: npx tsx scripts/test-osf9-product-path.mts
 */
import { ok, strictEqual } from 'node:assert/strict'
import { createHash } from 'node:crypto'
import fs from 'node:fs'
import {
  isSha256Hex,
  resolveLumenScienceBinary,
} from '../src/main/files/lumen-binary.js'
import { runOsf9ProductPath } from '../src/main/files/osf9-product-path.js'

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

async function main() {
  const report = await runOsf9ProductPath()
  for (const step of report.steps) {
    await test(`osf9:${step.name}`, () => {
      ok(step.ok, step.detail || step.name)
    })
  }
  await test('osf9 overall ok', () => ok(report.ok))
  await test('dossier has artifacts', () => {
    const d = report.exportProjection as { artifacts?: unknown[] }
    ok(Array.isArray(d.artifacts) && d.artifacts.length >= 3)
  })

  const liveStep = report.steps.find((s) => s.name === 'live-binary')
  await test('osf9 live-binary step present', () => {
    ok(liveStep, 'missing live-binary step')
  })

  if (report.binaryHash) {
    await test('binaryHash is 64-hex when live', () => {
      ok(isSha256Hex(report.binaryHash))
    })
    await test('binaryHash matches file contents when live', () => {
      const bin = resolveLumenScienceBinary()
      ok(bin, 'binaryHash set but resolve returned null')
      const fileHash = createHash('sha256').update(fs.readFileSync(bin!)).digest('hex')
      strictEqual(report.binaryHash, fileHash)
    })
    console.log(`REPORT binaryHash=${report.binaryHash}`)
  } else {
    await test('binaryHash null when offline skip', () => {
      strictEqual(report.binaryHash, null)
      ok(liveStep?.detail?.includes('skip') || liveStep?.detail?.includes('no lumen'), liveStep?.detail)
    })
    console.log('REPORT binaryHash=null (offline skip)')
  }

  console.log(`\nsteps=${report.steps.length} failures=${failures}`)
  console.log(failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`)
  process.exit(failures > 0 ? 1 : 0)
}

main()
