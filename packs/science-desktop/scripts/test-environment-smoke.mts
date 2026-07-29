#!/usr/bin/env npx tsx
/**
 * Production-dependency smoke: test the shipped environment discovery with
 * real host enumeration (not the fully-mocked unit fixture).
 *
 * This test runs on the CI runner and proves that:
 * 1. At least one absolute pinned Python interpreter is discovered.
 * 2. The canonical path is stable (not PATH-relative 'python3').
 * 3. probeInterpreterVersion returns a Python 3.x version string.
 * 4. discoverInterpreters returns runnable=true for at least one candidate.
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
  defaultDiscoveryDeps,
  discoverInterpreters,
} from '../src/main/notebook/environment-discovery.js'

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

  // Step 2: discoverInterpreters with real version probe
  await test('discoverInterpreters reports at least one runnable Python', async () => {
    const deps = defaultDiscoveryDeps(runtimeRoot)
    const interpreters = await discoverInterpreters('python', deps)
    ok(interpreters.length > 0, 'no interpreters discovered')
    const runnable = interpreters.filter((i) => i.runnable)
    ok(runnable.length > 0, `no runnable Python among ${interpreters.length} candidates`)
    for (const r of runnable) {
      ok(typeof r.version === 'string' && r.version.startsWith('3.'),
        `version not Python 3.x: ${r.version}`)
      ok(r.interpreterPath.startsWith('/'),
        `runnable path not absolute: ${r.interpreterPath}`)
      console.log(`  runnable: ${r.interpreterPath} v${r.version} provenance=${r.provenance}`)
    }
  })

  // Step 3: the first runnable candidate is canonical and stable
  await test('first runnable candidate has a stable canonical identity', async () => {
    const deps = defaultDiscoveryDeps(runtimeRoot)
    const first = await discoverInterpreters('python', deps)
    const runnable = first.filter((i) => i.runnable)
    ok(runnable.length > 0, 'no runnable after first scan')
    const second = await discoverInterpreters('python', deps)
    const runnable2 = second.filter((i) => i.runnable)
    ok(runnable2.length > 0, 'no runnable after second scan')
    strictEqual(runnable[0].interpreterPath, runnable2[0].interpreterPath,
      'first runnable changed between identical enumerations')
    strictEqual(runnable[0].envId, runnable2[0].envId,
      'canonical identity changed between enumerations')
  })

  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

run()
