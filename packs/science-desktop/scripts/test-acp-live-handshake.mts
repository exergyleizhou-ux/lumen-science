#!/usr/bin/env npx tsx
/**
 * Live ACP handshake against the REAL Rust binary (LS5-D2-01).
 *
 * Drives the shipped client — lumen-process-manager, acp-stdio-transport,
 * acp-session-manager, science-method-registry — through
 * spawn → initialize → authenticate → session/new → x.ai/science/project_list
 * against an actual `lumen agent stdio` process. Nothing here is stubbed: if
 * the wire format is wrong, this fails.
 *
 * It exists because the module this replaced was never once run against an
 * engine. It spoke HTTP to a port nothing listens on, so every unit test it
 * could have had would have passed while the product was inert.
 *
 * Binary discovery: LUMEN_BINARY, then agent/target/{debug,release}/lumen,
 * then `lumen` on PATH. No binary => SKIP with exit 0, never an invented pass.
 *
 * The child gets a temp HOME so the run cannot read or write the developer's
 * real ~/.grok state.
 *
 * Run: npx tsx scripts/test-acp-live-handshake.mts
 */
import { ok, strictEqual, match } from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

import { AcpSessionManager } from '../src/main/acp-session-manager.js'
import { sha256OfFile } from '../src/main/lumen-process-manager.js'
import { resolveScienceMethod } from '../src/main/science-method-registry.js'

const HERE = path.dirname(fileURLToPath(import.meta.url))
const REPO = path.resolve(HERE, '../../..')

function findBinary(): string | null {
  const fromEnv = process.env.LUMEN_BINARY?.trim()
  if (fromEnv) {
    return fs.existsSync(fromEnv) ? path.resolve(fromEnv) : null
  }
  for (const profile of ['debug', 'release']) {
    const candidate = path.join(REPO, 'agent', 'target', profile, 'lumen')
    if (fs.existsSync(candidate)) return candidate
  }
  for (const entry of (process.env.PATH ?? '').split(path.delimiter).filter(Boolean)) {
    const candidate = path.join(entry, 'lumen')
    if (fs.existsSync(candidate)) return candidate
  }
  return null
}

const binaryPath = findBinary()
if (!binaryPath) {
  console.log('SKIP  no lumen binary found (set LUMEN_BINARY or build agent/target/debug/lumen)')
  process.exit(0)
}

let failures = 0
async function test(name: string, fn: () => void | Promise<void>): Promise<void> {
  try {
    await fn()
    console.log(`OK  ${name}`)
  } catch (e: unknown) {
    failures++
    console.log(`FAIL ${name}: ${(e as Error).message}`)
  }
}

const home = fs.mkdtempSync(path.join(os.tmpdir(), 'lumen-live-home-'))
const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'lumen-live-cwd-'))
const storeRoot = path.join(workspace, 'science-store')
const interpreterPath = path.join(workspace, 'test-python')
const deniedInterpreterPath = path.join(workspace, 'denied-python')
const deniedMarkerPath = path.join(workspace, 'denied-probe-ran')
fs.writeFileSync(interpreterPath, "#!/bin/sh\necho 'Python 3.12.0 live-fixture'\n", {
  mode: 0o755,
})
fs.writeFileSync(
  deniedInterpreterPath,
  `#!/bin/sh\nprintf ran > '${deniedMarkerPath}'\necho 'Python 3.12.0 denied-fixture'\n`,
  { mode: 0o755 },
)

function countNamedFiles(root: string, name: string): number {
  if (!fs.existsSync(root)) return 0
  let count = 0
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const candidate = path.join(root, entry.name)
    if (entry.isDirectory()) count += countNamedFiles(candidate, name)
    else if (entry.isFile() && entry.name === name) count++
  }
  return count
}

console.log(`LIVE binary=${binaryPath}`)
console.log(`LIVE sha256=${sha256OfFile(binaryPath)}`)
console.log(`LIVE workspace=${workspace}`)

const manager = new AcpSessionManager({
  cwd: workspace,
  handshakeTimeoutMs: 120_000,
  requestTimeoutMs: 60_000,
  clientType: 'lumen-science-desktop-live-test',
  process: {
    env: { ...process.env, LUMEN_BINARY: binaryPath },
    childEnv: {
      HOME: home,
      GROK_HOME: path.join(home, '.grok'),
      XAI_API_KEY: 'live-handshake-test-key',
      GROK_TELEMETRY_ENABLED: 'false',
      GROK_DISABLE_AUTOUPDATER: '1',
    },
    shutdownGraceMs: 3_000,
  },
})

try {
  await test('spawn -> initialize -> authenticate -> session/new', async () => {
    const state = await manager.start()
    strictEqual(state.status, 'ready', `state: ${JSON.stringify(state)}`)
    ok(state.sessionId, 'session/new must return a sessionId')
    match(state.binaryHash ?? '', /^[a-f0-9]{64}$/)
    strictEqual(state.binaryHash, sha256OfFile(binaryPath))
    console.log(`LIVE sessionId=${state.sessionId}`)
  })

  await test('x.ai/science/project_list answers over the real wire', async () => {
    const result = await manager.callScience('project_list', { storeRoot })
    console.log(`LIVE project_list -> ${JSON.stringify(result)}`)
    ok(Array.isArray(result), `expected an array of projects, got ${JSON.stringify(result)}`)
  })

  await test('the wire method carries the ACP ext underscore prefix', async () => {
    // The unprefixed spelling is what the agent-client-protocol crate refuses:
    // only `_`-prefixed methods reach ext_method. Sending the bare name gets
    // -32601 from this very binary — that is how the prefix was established.
    strictEqual(resolveScienceMethod('project_list').wireMethod, '_x.ai/science/project_list')
  })

  // compute_plan, not project_assert_membership: the latter became a real
  // method in LS5-K18, and an "unknown method" test must use one that is still
  // genuinely unknown or it proves nothing.
  await test('an unknown science method is rejected without a round trip', async () => {
    let rejected = false
    try {
      await manager.callScience('compute_plan', {})
    } catch (e: unknown) {
      rejected = true
      strictEqual((e as { code?: string }).code, 'LUMEN_METHOD_NOT_ALLOWED')
    }
    ok(rejected, 'a method no engine implements must never reach the wire')
  })

  await test('engine state stays ready after a rejected method', () => {
    strictEqual(manager.getState().status, 'ready')
  })

  await test('a mutation is refused while the desktop offers no permission UI', async () => {
    // The bridge deliberately passes no onServerRequest, so the agent's
    // session/request_permission is answered -32601 and the mutation is
    // denied. That is fail-closed, not a bug to route around: auto-approving
    // in the main process would grant execution authority with no user in the
    // loop, which is the one thing this pack's authority model forbids.
    let refused = false
    try {
      await manager.callScience('project_create', {
        ownerId: 'live-test-owner',
        storeRoot,
        title: 'must not be created',
        researchQuestion: 'is the permission seam closed?',
        operationId: 'op-live-no-responder',
        approvalTimeoutMs: 8_000,
      })
    } catch (e: unknown) {
      refused = true
      strictEqual((e as { code?: string }).code, 'LUMEN_ACP_REMOTE_ERROR')
    }
    ok(refused, 'a mutation must not succeed without an answered permission request')
  })
} finally {
  await manager.stop()
}

// A second engine, this time WITH a permission responder, to prove the
// transport's agent->client request path really works and that the only thing
// standing between this client and a live SessionActor mutation is the missing
// desktop permission UI.
let approvingDecision: 'allow_once' | 'reject_once' = 'allow_once'
const approving = new AcpSessionManager({
  cwd: workspace,
  handshakeTimeoutMs: 120_000,
  requestTimeoutMs: 60_000,
  clientType: 'lumen-science-desktop-live-test',
  onServerRequest: async (method, params) => {
    if (method !== 'session/request_permission') {
      throw new Error(`unexpected agent request '${method}'`)
    }
    const options =
      (params as { options?: Array<{ optionId?: string; kind?: string }> })?.options ?? []
    const chosen = options.find((o) => o.kind === approvingDecision) ?? options[0]
    return { outcome: { outcome: 'selected', optionId: chosen?.optionId } }
  },
  process: {
    env: { ...process.env, LUMEN_BINARY: binaryPath },
    childEnv: {
      HOME: home,
      GROK_HOME: path.join(home, '.grok'),
      XAI_API_KEY: 'live-handshake-test-key',
      GROK_TELEMETRY_ENABLED: 'false',
      GROK_DISABLE_AUTOUPDATER: '1',
    },
    shutdownGraceMs: 3_000,
  },
})

try {
  let createdProjectId = ''
  await test('an approved mutation routes through the SessionActor', async () => {
    await approving.start()
    const created = (await approving.callScience('project_create', {
      ownerId: 'live-test-owner',
      storeRoot,
      title: 'Live handshake project',
      researchQuestion: 'does the desktop reach the SessionActor?',
      operationId: 'op-live-approved-1',
      approvalTimeoutMs: 15_000,
    })) as Record<string, unknown>
    console.log(
      `LIVE project_create -> projectId=${String(created.projectId)} ` +
        `replayed=${String(created.replayed)} authority=${String(created.runtimeAuthority)}`,
    )
    strictEqual(created.runtimeAuthority, 'SessionActor-gated ACP adapter')
    strictEqual(created.replayed, false)
    ok(typeof created.projectId === 'string' && created.projectId.length > 0)
    createdProjectId = String(created.projectId)
  })

  await test('approved kernel admission probes and commits only through the SessionActor', async () => {
    ok(createdProjectId, 'project creation must complete before kernel admission')
    const result = (await approving.callScience('kernel_admission', {
      ownerId: 'live-test-owner',
      projectId: createdProjectId,
      storeRoot,
      kernelId: 'live-python-fixture',
      kind: 'python',
      interpreterPath,
      allowedRoot: workspace,
      probeTimeoutMs: 10_000,
      approvalTimeoutMs: 15_000,
    })) as {
      state?: string
      admission?: {
        admission_status?: string
        executable_hash?: string
        exact_version?: string
      }
      artifacts?: Array<{ sha256?: string; relative_path?: string }>
      approvals?: Array<{ decision?: string }>
      runtimeAuthority?: string
    }
    strictEqual(result.runtimeAuthority, 'SessionActor-gated ACP adapter')
    strictEqual(result.state, 'succeeded')
    strictEqual(result.admission?.admission_status, 'Admitted')
    match(result.admission?.executable_hash ?? '', /^[a-f0-9]{64}$/)
    match(result.admission?.exact_version ?? '', /^Python 3\.12\.0 live-fixture/)
    strictEqual(result.artifacts?.length, 1)
    match(result.artifacts?.[0]?.sha256 ?? '', /^[a-f0-9]{64}$/)
    strictEqual(result.approvals?.[0]?.decision, 'allow')
  })

  await test('denied kernel admission neither executes nor creates an artifact', async () => {
    approvingDecision = 'reject_once'
    const artifactsBefore = countNamedFiles(storeRoot, 'kernel-admission.json')
    let refused = false
    try {
      await approving.callScience('kernel_admission', {
        ownerId: 'live-test-owner',
        projectId: createdProjectId,
        storeRoot,
        kernelId: 'denied-python-fixture',
        kind: 'python',
        interpreterPath: deniedInterpreterPath,
        allowedRoot: workspace,
        probeTimeoutMs: 10_000,
        approvalTimeoutMs: 15_000,
      })
    } catch (e: unknown) {
      refused = true
      strictEqual((e as { code?: string }).code, 'LUMEN_ACP_REMOTE_ERROR')
    }
    ok(refused, 'a rejected permission must terminate the admission call')
    ok(!fs.existsSync(deniedMarkerPath), 'denied admission executed its interpreter')
    strictEqual(
      countNamedFiles(storeRoot, 'kernel-admission.json'),
      artifactsBefore,
      'denied admission created a kernel artifact',
    )
  })

  await test('project_list now returns the project the desktop created', async () => {
    const listed = (await approving.callScience('project_list', {
      storeRoot,
    })) as unknown[]
    console.log(`LIVE project_list -> ${listed.length} project(s)`)
    strictEqual(listed.length, 1, `expected 1 project, got ${JSON.stringify(listed)}`)
  })
} finally {
  await approving.stop()
  for (const dir of [home, workspace]) {
    try {
      fs.rmSync(dir, { recursive: true, force: true })
    } catch {
      /* ignore */
    }
  }
}

console.log(
  `\n${failures === 0 ? 'ALL LIVE HANDSHAKE TESTS PASSED' : `${failures} LIVE TESTS FAILED`}`,
)
process.exit(failures > 0 ? 1 : 0)
