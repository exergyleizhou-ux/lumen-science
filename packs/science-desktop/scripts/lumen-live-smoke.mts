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
import fs from 'node:fs'
import path from 'node:path'

function resolveBinary(): string | null {
  if (process.env.LUMEN_BINARY && fs.existsSync(process.env.LUMEN_BINARY)) {
    return process.env.LUMEN_BINARY
  }
  const candidates = [
    path.join(process.env.HOME || '', '.local/bin/lumen-science'),
    'lumen-science',
  ]
  for (const c of candidates) {
    if (c === 'lumen-science') {
      const which = spawnSync('which', ['lumen-science'], { encoding: 'utf-8' })
      if (which.status === 0 && which.stdout.trim()) return which.stdout.trim()
      continue
    }
    if (fs.existsSync(c)) return c
  }
  return null
}

const bin = resolveBinary()
if (!bin) {
  console.log('SKIP  lumen-science binary not found (set LUMEN_BINARY to enable live smoke)')
  process.exit(0)
}

console.log(`PROBE binary=${bin}`)

let failures = 0
function run(name: string, args: string[], expectSubstring?: string) {
  const r = spawnSync(bin!, args, { encoding: 'utf-8', timeout: 30_000 })
  const out = `${r.stdout || ''}${r.stderr || ''}`
  if (r.error) {
    failures++
    console.log(`FAIL ${name}: ${r.error.message}`)
    return
  }
  if (r.status !== 0) {
    failures++
    console.log(`FAIL ${name}: exit ${r.status}\n${out.slice(0, 500)}`)
    return
  }
  if (expectSubstring && !out.includes(expectSubstring)) {
    failures++
    console.log(`FAIL ${name}: expected substring ${expectSubstring}\n${out.slice(0, 300)}`)
    return
  }
  console.log(`OK  ${name}`)
}

run('version', ['version'], '1.')
run('help mentions SessionActor', ['--help'], 'SessionActor')
// doctor may need repo root — try without, allow soft fail as SKIP message
{
  const r = spawnSync(bin, ['doctor'], {
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

console.log(`\n${failures === 0 ? 'ALL LIVE SMOKE PASSED' : `${failures} LIVE SMOKE FAILED`}`)
process.exit(failures > 0 ? 1 : 0)
