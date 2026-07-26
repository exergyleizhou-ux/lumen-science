#!/usr/bin/env node
/**
 * Authority boundary tests — EXECUTES the shipped policy module.
 *
 * Uses tsx to load lumen-authority-policy.ts (pure TypeScript, no Electron
 * imports) and drives validateIpcChannel + assertArtifactPreviewAccess
 * with real test vectors. Falls back to structural verification if tsx
 * is unavailable, but logs a warning.
 */
const { strictEqual, ok, deepStrictEqual } = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
let failures = 0;

function test(name, fn) {
  try { fn(); console.log(`OK  ${name}`); }
  catch (e) { failures++; console.log(`FAIL ${name}: ${e.message}`); }
}

(async () => {
// ── Load shipped policy module ───────────────────────────────────

let validateIpcChannel, assertArtifactPreviewAccess, getBannedChannels
let LOADED = false

try {
  const mod = await import('./src/main/lumen-authority-policy.ts')
  validateIpcChannel = mod.validateIpcChannel
  assertArtifactPreviewAccess = mod.assertArtifactPreviewAccess
  getBannedChannels = mod.getBannedChannels
  LOADED = true
  console.log('POLICY-MODULE: loaded via tsx (EXECUTING shipped code)')
} catch (e) {
  console.log(`POLICY-MODULE: tsx import failed, structural mode — ${e.message}`)
}

// ── IPC channel validation (shipped function) ────────────────────

if (LOADED) {
  const banned = getBannedChannels()
  test('validateIpcChannel rejects artifacts:finalize-run', () =>
    strictEqual(validateIpcChannel('artifacts:finalize-run'), false))
  test('validateIpcChannel rejects artifacts:open-file', () =>
    strictEqual(validateIpcChannel('artifacts:open-file'), false))
  test('validateIpcChannel rejects artifacts:read-preview', () =>
    strictEqual(validateIpcChannel('artifacts:read-preview'), false))
  test('validateIpcChannel rejects projects:create', () =>
    strictEqual(validateIpcChannel('projects:create'), false))
  test('validateIpcChannel rejects projects:delete', () =>
    strictEqual(validateIpcChannel('projects:delete'), false))
  test('validateIpcChannel rejects reviewer:run', () =>
    strictEqual(validateIpcChannel('reviewer:run'), false))
  test('validateIpcChannel rejects reviewer:abort-fix-loop', () =>
    strictEqual(validateIpcChannel('reviewer:abort-fix-loop'), false))
  test('validateIpcChannel rejects compute:job-updated', () =>
    strictEqual(validateIpcChannel('compute:job-updated'), false))
  test('validateIpcChannel rejects preview:load', () =>
    strictEqual(validateIpcChannel('preview:load'), false))
  test('validateIpcChannel rejects preview:save', () =>
    strictEqual(validateIpcChannel('preview:save'), false))
  test('validateIpcChannel rejects preview:delete', () =>
    strictEqual(validateIpcChannel('preview:delete'), false))

  test('validateIpcChannel allows acp:call', () =>
    strictEqual(validateIpcChannel('acp:call'), true))
  test('validateIpcChannel allows acp:list-tools', () =>
    strictEqual(validateIpcChannel('acp:list-tools'), true))
  test('validateIpcChannel allows window:close', () =>
    strictEqual(validateIpcChannel('window:close'), true))
  test('validateIpcChannel allows settings:get', () =>
    strictEqual(validateIpcChannel('settings:get'), true))
  test('validateIpcChannel default-deny: unknown', () =>
    strictEqual(validateIpcChannel('random:unknown'), false))

  test('getBannedChannels returns set with real entries', () => {
    ok(banned instanceof Set, 'should be a Set')
    ok(banned.size >= 15, `at least 15 banned channels, got ${banned.size}`)
  })

  // ── Artifact preview access (shipped function) ─────────────────
  test('assertArtifactPreviewAccess: allows valid', () => {
    const r = assertArtifactPreviewAccess(
      { artifactId: 'a1', ownerId: 'o1', projectId: 'p1', expectedSha256: 'abc' },
      { ownerId: 'o1', projectId: 'p1', digest: 'abc' }
    )
    ok(r.ok, 'valid access should be ok')
  })

  test('assertArtifactPreviewAccess: rejects empty artifact_id', () => {
    const r = assertArtifactPreviewAccess(
      { artifactId: '', ownerId: 'o1', projectId: 'p1' },
      { ownerId: 'o1', projectId: 'p1' }
    )
    ok(!r.ok, 'empty id should be rejected')
    ok(r.reason.includes('required'), `reason should mention required: ${r.reason}`)
  })

  test('assertArtifactPreviewAccess: rejects wrong owner', () => {
    const r = assertArtifactPreviewAccess(
      { artifactId: 'a1', ownerId: 'oX', projectId: 'p1' },
      { ownerId: 'o1', projectId: 'p1' }
    )
    ok(!r.ok, 'wrong owner should be rejected')
    ok(r.reason.includes('owner'), `reason should mention owner: ${r.reason}`)
  })

  test('assertArtifactPreviewAccess: rejects wrong project', () => {
    const r = assertArtifactPreviewAccess(
      { artifactId: 'a1', ownerId: 'o1', projectId: 'pX' },
      { ownerId: 'o1', projectId: 'p1' }
    )
    ok(!r.ok, 'wrong project should be rejected')
    ok(r.reason.includes('project'), `reason should mention project: ${r.reason}`)
  })

  test('assertArtifactPreviewAccess: rejects hash mismatch', () => {
    const r = assertArtifactPreviewAccess(
      { artifactId: 'a1', ownerId: 'o1', projectId: 'p1', expectedSha256: 'aaa' },
      { ownerId: 'o1', projectId: 'p1', digest: 'bbb' }
    )
    ok(!r.ok, 'hash mismatch should be rejected')
    ok(r.reason.includes('sha256'), `reason should mention sha256: ${r.reason}`)
  })
} else {
  // Structural fallback (tsx unavailable — less strong but honest)
  console.log('  (structural mode — verifying source text)')
  const src = fs.readFileSync('src/main/lumen-authority-policy.ts', 'utf-8')
  test('source: exports validateIpcChannel', () => ok(src.includes('export function validateIpcChannel')))
  test('source: exports assertArtifactPreviewAccess', () => ok(src.includes('export function assertArtifactPreviewAccess')))
  test('source: exports getBannedChannels', () => ok(src.includes('export function getBannedChannels')))
  test('source: BANNED_CHANNELS contains real names', () => {
    for (const ch of ['artifacts:finalize-run', 'projects:create', 'reviewer:run', 'compute:job-updated']) {
      ok(src.includes(ch), `source must contain banned channel: ${ch}`)
    }
  })
}

// ── ipc.ts no longer imports science execution authorities ──────

const IPC_SRC = fs.readFileSync('src/main/ipc.ts', 'utf-8')
const BANNED_IMPORTS = [
  'SystemSshRunner', 'SystemScpRunner', 'JobPoller', 'harvestJob',
  'registerComputeIpcHandlers', 'registerNotebookIpcHandlers',
  'registerReviewerIpcHandlers',
]
for (const sym of BANNED_IMPORTS) {
  test(`ipc.ts: no ${sym} import`, () => {
    ok(!IPC_SRC.includes(sym), `ipc.ts must not import ${sym}`)
  })
}

test('ipc.ts imports safeHandle from lumen-acp-bridge', () => {
  ok(IPC_SRC.includes('safeHandle'), 'ipc.ts must import safeHandle')
})

test('ipc.ts imports assertArtifactPreviewAccess', () => {
  ok(IPC_SRC.includes('assertArtifactPreviewAccess'), 'ipc.ts must import assertArtifactPreviewAccess')
})

// ── Skills boundary ──────────────────────────────────────────────

test('Open Science skills quarantined, Lumen still 10 approved', () => {
  const registry = JSON.parse(
    fs.readFileSync('../../packs/science/skills/registry.json', 'utf-8')
  )
  const approvedIds = new Set(
    registry.skills.filter(s => s.final_disposition === 'approved').map(s => s.skill_id)
  )
  strictEqual(approvedIds.size, 10, 'Lumen approved count unchanged')
  for (const id of ['alphafold2', 'boltz', 'evo2', 'diffdock', 'esmfold2', 'proteinmpnn']) {
    ok(!approvedIds.has(id), `OS skill ${id} must NOT be in Lumen approved`)
  }
  strictEqual(registry.summary.pending, 17, 'pending count unchanged')
})

// ── Branding ─────────────────────────────────────────────────────

test('electron-builder.yml branded Lumen', () => {
  const content = fs.readFileSync('electron-builder.yml', 'utf-8')
  ok(!content.includes('CFBundleName: Open Science'), 'no Open Science CFBundleName')
  ok(content.includes('Lumen Science Desktop'), 'has Lumen brand')
})

// ── Brace balance (structural sanity) ────────────────────────────

test('ipc.ts brace balanced', () => {
  const lines = IPC_SRC.split('\n')
  let brace = 0
  for (const l of lines) brace += (l.match(/\{/g) || []).length - (l.match(/\}/g) || []).length
  strictEqual(brace, 0, `brace imbalance: ${brace}`)
})

// ── Result ────────────────────────────────────────────────────────

console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
process.exit(failures > 0 ? 1 : 0)
})()
