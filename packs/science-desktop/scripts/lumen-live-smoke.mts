#!/usr/bin/env npx tsx
/**
 * Optional live smoke against a real lumen-science binary on PATH / LUMEN_BINARY.
 *
 * Never fails the suite if binary missing — exits 0 with SKIP.
 * Fails only if binary present and required commands fail.
 *
 * Run: npx tsx scripts/lumen-live-smoke.mts
 */
import { spawnSync } from 'node:child_process'
import path from 'node:path'
import {
  isSha256Hex,
  resolveAndProbeLumenScienceBinary,
  sha256File,
} from '../src/main/files/lumen-binary.js'

const probe = resolveAndProbeLumenScienceBinary()
if (!probe) {
  console.log('SKIP  lumen-science binary not found (set LUMEN_BINARY to enable live smoke)')
  process.exit(0)
}

console.log(`PROBE binary=${probe.binaryPath}`)
console.log(`PROBE sha256=${probe.binaryHash}`)

let failures = 0

if (!isSha256Hex(probe.binaryHash)) {
  failures++
  console.log(`FAIL binaryHash not 64-hex: ${probe.binaryHash}`)
} else {
  const recompute = sha256File(probe.binaryPath)
  if (recompute !== probe.binaryHash) {
    failures++
    console.log(`FAIL binaryHash mismatch recompute=${recompute}`)
  } else {
    console.log('OK  binaryHash')
  }
}

if (!probe.versionOk) {
  failures++
  console.log(`FAIL version: ${probe.detail || probe.versionOutput.slice(0, 200)}`)
} else {
  console.log(`OK  version (${probe.versionOutput.split('\n')[0]})`)
}

if (!probe.helpOk) {
  failures++
  console.log(`FAIL help: ${probe.detail || probe.helpOutput.slice(0, 200)}`)
} else {
  console.log('OK  root help + agent stdio help')
}

// doctor may need repo root — try without, allow soft fail as SKIP message
{
  const r = spawnSync(probe.binaryPath, ['doctor'], {
    encoding: 'utf-8',
    timeout: 60_000,
    cwd: path.resolve(process.cwd(), '../..'),
  })
  if (r.status === 0) {
    console.log('OK  doctor')
  } else {
    console.log(
      `WARN doctor exit ${r.status} (non-fatal for smoke if version/help pass)\n${(r.stdout || r.stderr || '').slice(0, 200)}`,
    )
  }
}

if (!probe.ok && failures === 0) {
  // defensive: probe.ok false should have failed version/help above
  failures++
  console.log(`FAIL probe.ok=false detail=${probe.detail}`)
}

console.log(`\n${failures === 0 ? 'ALL LIVE SMOKE PASSED' : `${failures} LIVE SMOKE FAILED`}`)
process.exit(failures > 0 ? 1 : 0)
