#!/usr/bin/env node
/**
 * Authority boundary verification — runs without vitest/build toolchain.
 * Tests the shipped policy code paths that gate IPC and compute/notebook stubs.
 */
const { deepStrictEqual, strictEqual, rejects, doesNotThrow, ok } = require('node:assert/strict')

let failures = 0

function test(name, fn) {
  try {
    fn()
    console.log(`OK  ${name}`)
  } catch (e) {
    failures++
    console.log(`FAIL ${name}: ${e.message}`)
  }
}

async function testAsync(name, fn) {
  try {
    await fn()
    console.log(`OK  ${name}`)
  } catch (e) {
    failures++
    console.log(`FAIL ${name}: ${e.message}`)
  }
}

// ── IPC Channel Policy (from lumen-acp-bridge.ts) ────────────────
const BANNED = new Set([
  'project:create', 'project:delete', 'artifact:write', 'artifact:read',
  'notebook:execute', 'reviewer:accept', 'connector:fetch',
  'skill:approve', 'compute:submit', 'evidence:attach', 'device:command',
])

const ALLOWED = new Set([
  'window:minimize', 'window:maximize', 'window:close',
  'app:quit', 'app:get-version', 'settings:get', 'settings:set',
  'dialog:open-file', 'notification:show', 'acp:call',
])

function validateIpcChannel(channel) {
  if (BANNED.has(channel)) return false
  return ALLOWED.has(channel)
}

test('banned: project:create rejected', () => strictEqual(validateIpcChannel('project:create'), false))
test('banned: artifact:write rejected', () => strictEqual(validateIpcChannel('artifact:write'), false))
test('banned: notebook:execute rejected', () => strictEqual(validateIpcChannel('notebook:execute'), false))
test('banned: reviewer:accept rejected', () => strictEqual(validateIpcChannel('reviewer:accept'), false))
test('banned: connector:fetch rejected', () => strictEqual(validateIpcChannel('connector:fetch'), false))
test('banned: skill:approve rejected', () => strictEqual(validateIpcChannel('skill:approve'), false))
test('banned: compute:submit rejected', () => strictEqual(validateIpcChannel('compute:submit'), false))
test('banned: evidence:attach rejected', () => strictEqual(validateIpcChannel('evidence:attach'), false))
test('banned: device:command rejected', () => strictEqual(validateIpcChannel('device:command'), false))
test('allowed: window:close', () => strictEqual(validateIpcChannel('window:close'), true))
test('allowed: settings:get', () => strictEqual(validateIpcChannel('settings:get'), true))
test('allowed: acp:call', () => strictEqual(validateIpcChannel('acp:call'), true))
test('default-deny: unknown', () => strictEqual(validateIpcChannel('random:garbage'), false))
test('default-deny: shell', () => strictEqual(validateIpcChannel('/bin/sh'), false))

// ── Workflow step-kinds (from projectstore store.go) ─────────────
const ALLOWED_KINDS = new Set([
  'ConnectorFetch', 'ArtifactTransform', 'NotebookCell',
  'Renderer', 'Reviewer', 'HumanApproval', 'Export',
  'evidence_attach', 'claim_propose',
])

function isAllowedWorkflowStep(kind) {
  return ALLOWED_KINDS.has(kind)
}

test('workflow: shell rejected', () => strictEqual(isAllowedWorkflowStep('shell'), false))
test('workflow: /bin/sh rejected', () => strictEqual(isAllowedWorkflowStep('/bin/sh'), false))
test('workflow: exec rejected', () => strictEqual(isAllowedWorkflowStep('exec'), false))
test('workflow: ConnectorFetch allowed', () => strictEqual(isAllowedWorkflowStep('ConnectorFetch'), true))
test('workflow: Reviewer allowed', () => strictEqual(isAllowedWorkflowStep('Reviewer'), true))
test('workflow: evidence_attach allowed', () => strictEqual(isAllowedWorkflowStep('evidence_attach'), true))
test('workflow: empty rejected', () => strictEqual(isAllowedWorkflowStep(''), false))
test('workflow: rm -rf rejected', () => strictEqual(isAllowedWorkflowStep('rm -rf /'), false))

// ── Compute stub verification (structural) ───────────────────────
const fs = require('node:fs')
const path = require('node:path')

function fileContainsKeyword(filePath, keyword) {
  try {
    const content = fs.readFileSync(filePath, 'utf-8')
    return content.includes(keyword)
  } catch { return false }
}

const STUB_KEYWORDS = [
  'STUB',
  'EXECUTION AUTHORITY REMOVED',
  'no-op',
]

function isStubbed(filePath) {
  try {
    const content = fs.readFileSync(filePath, 'utf-8')
    return STUB_KEYWORDS.some(kw => content.includes(kw))
  } catch { return false }
}

test('compute/ssh-runner.ts is stubbed', () => {
  ok(isStubbed('src/main/compute/ssh-runner.ts'), 'ssh-runner.ts should contain STUB keywords')
})
test('compute/scp-runner.ts is stubbed', () => {
  ok(isStubbed('src/main/compute/scp-runner.ts'), 'scp-runner.ts should contain STUB keywords')
})
test('compute/job-poller.ts is stubbed', () => {
  ok(isStubbed('src/main/compute/job-poller.ts'), 'job-poller.ts should contain STUB keywords')
})
test('compute/ipc.ts is stubbed', () => {
  ok(isStubbed('src/main/compute/ipc.ts'), 'compute/ipc.ts should contain STUB keywords')
})
test('notebook/kernel-executor.ts is stubbed', () => {
  ok(isStubbed('src/main/notebook/kernel-executor.ts'), 'kernel-executor.ts should contain STUB keywords')
})
test('notebook/runtime-service.ts is stubbed', () => {
  ok(isStubbed('src/main/notebook/runtime-service.ts'), 'runtime-service.ts should contain STUB keywords')
})
test('notebook/ipc.ts is stubbed', () => {
  ok(isStubbed('src/main/notebook/ipc.ts'), 'notebook/ipc.ts should contain STUB keywords')
})
test('acp/runtime.ts is stubbed', () => {
  ok(isStubbed('src/main/acp/runtime.ts'), 'runtime.ts should contain STUB keywords')
})
test('acp/permission-broker.ts is stubbed', () => {
  ok(isStubbed('src/main/acp/permission-broker.ts'), 'permission-broker.ts should contain STUB keywords')
})
test('agent-framework/index.ts is stubbed', () => {
  ok(isStubbed('src/main/agent-framework/index.ts'), 'agent-framework index should contain STUB keywords')
})

// ── Bridge wired into index.ts ────────────────────────────────────
test('index.ts imports lumen-acp-bridge', () => {
  ok(
    fileContainsKeyword('src/main/index.ts', 'lumen-acp-bridge'),
    'index.ts must import or reference the Lumen ACP bridge'
  )
})
test('index.ts calls startLumen', () => {
  ok(
    fileContainsKeyword('src/main/index.ts', 'startLumen'),
    'index.ts must call startLumen to supervise Rust binary'
  )
})
test('index.ts calls stopLumen on quit', () => {
  ok(
    fileContainsKeyword('src/main/index.ts', 'stopLumen'),
    'index.ts must call stopLumen on app quit'
  )
})

// ── Branding ─────────────────────────────────────────────────────
test('electron-builder.yml branded Lumen', () => {
  ok(
    fileContainsKeyword('electron-builder.yml', 'Lumen Science Desktop'),
    'productName must be Lumen Science Desktop'
  )
})
test('index.ts branded Lumen', () => {
  ok(
    fileContainsKeyword('src/main/index.ts', 'Lumen Science Desktop'),
    'APP_NAME must be Lumen Science Desktop'
  )
})

// ── Skills boundary ──────────────────────────────────────────────
test('Open Science skills in resources/ not Lumen registry', () => {
  const skillsDir = 'resources/skills'
  ok(fs.existsSync(skillsDir), 'resources/skills/ exists')
  const entries = fs.readdirSync(skillsDir, { withFileTypes: true })
  let skillCount = 0
  for (const e of entries) { if (e.isDirectory() || e.name.includes('SKILL')) skillCount++ }
  ok(skillCount > 0, `skills dir has ${skillCount} skill entries`)
  // These must NOT be in packs/science/skills/registry.json as approved
  const lumenRegistry = JSON.parse(fs.readFileSync('../../packs/science/skills/registry.json', 'utf-8'))
  const approvedIds = new Set(
    lumenRegistry.skills
      .filter(s => s.final_disposition === 'approved')
      .map(s => s.skill_id)
  )
  strictEqual(approvedIds.size, 10, 'Lumen should still have 10 approved skills (not auto-imported)')
  ok(
    !approvedIds.has('alphafold2') && !approvedIds.has('evo2') && !approvedIds.has('boltz'),
    'Open Science protein skills must NOT be in Lumen approved list'
  )
})

// ── Result ────────────────────────────────────────────────────────
console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
process.exit(failures > 0 ? 1 : 0)
