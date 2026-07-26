#!/usr/bin/env npx tsx
/**
 * OSF-9 product-path + adversarial tests (shipped composition).
 * Run: npx tsx scripts/test-osf9-product-path.mts
 */
import { ok, strictEqual } from 'node:assert/strict'
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

  console.log(`\nsteps=${report.steps.length} failures=${failures}`)
  console.log(failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`)
  process.exit(failures > 0 ? 1 : 0)
}

main()
