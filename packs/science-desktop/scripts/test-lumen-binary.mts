#!/usr/bin/env npx tsx
/**
 * Unit tests for shipped lumen-binary helpers (real fs + crypto + optional live binary).
 * Run: npx tsx scripts/test-lumen-binary.mts
 */
import { ok, strictEqual, notEqual } from 'node:assert/strict'
import { createHash } from 'node:crypto'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import {
  isSha256Hex,
  probeLumenScienceBinary,
  resolveAndProbeLumenScienceBinary,
  resolveLumenScienceBinary,
  sha256File,
} from '../src/main/files/lumen-binary.js'

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

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'lumen-bin-test-'))
const fixture = path.join(tmp, 'fixture.bin')
const payload = Buffer.from('lumen-science-binary-fixture-v1\n')
fs.writeFileSync(fixture, payload)
const expectedHash = createHash('sha256').update(payload).digest('hex')

test('sha256File matches node crypto on real file', () => {
  strictEqual(sha256File(fixture), expectedHash)
  ok(isSha256Hex(sha256File(fixture)))
})

test('isSha256Hex rejects short/invalid', () => {
  strictEqual(isSha256Hex('abc'), false)
  strictEqual(isSha256Hex(null), false)
  strictEqual(isSha256Hex(expectedHash.toUpperCase()), false) // lowercase only
  ok(isSha256Hex(expectedHash))
})

test('resolveLumenScienceBinary respects LUMEN_BINARY', () => {
  const resolved = resolveLumenScienceBinary({
    ...process.env,
    LUMEN_BINARY: fixture,
  })
  strictEqual(resolved, path.resolve(fixture))
})

test('resolveLumenScienceBinary returns null when missing', () => {
  const resolved = resolveLumenScienceBinary({
    HOME: path.join(tmp, 'no-home'),
    USERPROFILE: path.join(tmp, 'no-home'),
    LUMEN_BINARY: path.join(tmp, 'does-not-exist'),
    PATH: '/nonexistent/bin',
  })
  strictEqual(resolved, null)
})

test('probe on non-executable content fails closed (no fake ok)', () => {
  const probe = probeLumenScienceBinary(fixture)
  strictEqual(probe.binaryHash, expectedHash)
  ok(isSha256Hex(probe.binaryHash))
  // fixture is not a real lumen binary — must not claim ok
  strictEqual(probe.ok, false)
  ok(probe.versionOk === false || probe.helpOk === false)
})

// Live path: only when a real binary exists (do not invent)
const live = resolveAndProbeLumenScienceBinary()
if (!live) {
  console.log('SKIP live probe (no lumen-science on PATH / LUMEN_BINARY)')
} else {
  test('live binaryHash is 64-hex matching shasum of file', () => {
    ok(isSha256Hex(live.binaryHash))
    strictEqual(live.binaryHash, sha256File(live.binaryPath))
    // independent recompute via createHash stream equivalent
    const buf = fs.readFileSync(live.binaryPath)
    strictEqual(live.binaryHash, createHash('sha256').update(buf).digest('hex'))
  })
  test('live probe version/help ok', () => {
    ok(live.ok, live.detail || 'live probe failed')
    ok(live.versionOk)
    ok(live.helpOk)
    ok(/1\.\d/.test(live.versionOutput))
    ok(/Lumen TUI/i.test(live.helpOutput))
    ok(/Run the agent over stdio/i.test(live.helpOutput))
  })
  test('live binary path is absolute existing file', () => {
    ok(path.isAbsolute(live.binaryPath))
    ok(fs.existsSync(live.binaryPath))
    notEqual(live.binaryPath, '')
  })
  console.log(`LIVE binary=${live.binaryPath}`)
  console.log(`LIVE hash=${live.binaryHash}`)
  console.log(`LIVE version=${live.versionOutput.split('\n')[0]}`)
}

// cleanup
try {
  fs.rmSync(tmp, { recursive: true, force: true })
} catch {
  /* ignore */
}

console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
process.exit(failures > 0 ? 1 : 0)
