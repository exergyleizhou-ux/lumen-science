#!/usr/bin/env npx tsx
/**
 * Production-dependency smoke: test the shipped environment discovery with
 * real host enumeration (not the fully-mocked unit fixture).
 *
 * This test runs on the CI runner and proves that:
 * 1. At least one absolute pinned Python interpreter is discovered.
 * 2. The canonical path is stable (not PATH-relative 'python3').
 * 3. Discovery does not probe or claim runnability before actor approval.
 * 4. The exact notebook resolver forwards the first pinned candidate.
 *
 * Not a substitute for full Rust admission — that remains the engine's job.
 * Used in Desktop CI before E2E to catch "no runnable Python" root cause.
 *
 * Run: npx tsx scripts/test-environment-smoke.mts
 */

import { strictEqual, ok } from 'node:assert/strict'
import {
  createProductionHostEnumeration,
  defaultCandidatePaths,
} from '../src/main/notebook/environment-discovery.js'
import { createEnvironmentService } from '../src/main/environment/service.js'
import { resolveNotebookInterpreter } from '../src/main/files/science-ipc.js'

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

async function run() {
  const runtimeRoot = process.env.HOME
    ? `${process.env.HOME}/.lumen-science/runtime`
    : '/tmp/lumen-smoke-runtime'
  const host = createProductionHostEnumeration()

  // Step 1: candidatePaths with real which/PATH/well-known
  await test('candidatePaths returns at least one pinned absolute Python path', async () => {
    const pathsFn = defaultCandidatePaths(runtimeRoot, undefined, undefined, host)
    const paths = await pathsFn('python')
    ok(paths.length > 0, `no candidates; host platform=${host.platform}`)
    for (const p of paths) {
      ok(p.startsWith('/'), `candidate not absolute: ${p}`)
    }
    console.log(`  candidates: ${paths.length} (${paths.slice(0,5).join(', ')})`)
  })

  const service = createEnvironmentService({ runtimeRoot })

  // Step 2: the product service enumerates without executing candidates.
  await test('environment service reports pinned candidates as unprobed', async () => {
    const report = await service.discover('python')
    ok(report.interpreters.length > 0, 'no interpreters discovered')
    for (const candidate of report.interpreters) {
      ok(candidate.interpreterPath.startsWith('/'),
        `candidate path not absolute: ${candidate.interpreterPath}`)
      strictEqual(candidate.runnable, false, 'observation-only discovery claimed runnable')
      strictEqual(candidate.version, undefined, 'observation-only discovery leaked a version probe')
      ok(candidate.detail?.includes('has not probed'), candidate.detail)
      console.log(`  candidate: ${candidate.interpreterPath} provenance=${candidate.provenance}`)
    }
  })

  // Step 3: the same resolver wired into notebook execution is stable.
  await test('notebook resolver forwards a stable pinned candidate', async () => {
    const firstReport = await service.discover('python')
    const first = await resolveNotebookInterpreter(service)
    const secondReport = await service.discover('python')
    const second = await resolveNotebookInterpreter(service)
    ok(first.ok, first.ok ? undefined : first.reason)
    ok(second.ok, second.ok ? undefined : second.reason)
    if (!first.ok || !second.ok) return
    strictEqual(first.interpreterPath, second.interpreterPath,
      'first candidate changed between identical enumerations')
    strictEqual(firstReport.interpreters[0].envId, secondReport.interpreters[0].envId,
      'canonical identity changed between enumerations')
  })

  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

run()
