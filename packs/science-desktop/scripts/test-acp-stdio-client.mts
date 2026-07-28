#!/usr/bin/env npx tsx
/**
 * ACP-over-stdio client tests (LS5-D2-01).
 *
 * These EXECUTE the shipped modules — science-method-registry,
 * acp-stdio-transport, lumen-process-manager, acp-session-manager — against
 * real pipes and a real child process. They are weighted toward the failure
 * modes, because the defect being fixed was a client that could not tell a
 * dead engine from a working one:
 *
 *   - a method no engine implements is rejected by the registry, not attempted
 *   - an oversized frame terminates the transport
 *   - malformed JSON on stdout fails closed
 *   - a crashed child surfaces an explicit unavailable state, never a mock
 *   - a binary whose hash does not match the pin is never spawned
 *
 * Error KIND is asserted on the stable `code` field rather than `instanceof`.
 * That is deliberate on two counts: the code is the contract that survives the
 * IPC boundary (prototypes do not), and `instanceof` across a tsx script and
 * the module graph under test compares two different class identities.
 *
 * Run: npx tsx scripts/test-acp-stdio-client.mts
 */
import { ok, strictEqual, deepStrictEqual, match } from 'node:assert/strict'
import { PassThrough } from 'node:stream'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

import {
  ACP_EXT_WIRE_PREFIX,
  SCIENCE_METHODS,
  SCIENCE_METHOD_NAMESPACE,
  explainRejection,
  isScienceMethod,
  listScienceMethods,
  resolveScienceMethod,
} from '../src/main/science-method-registry.js'
import { AcpStdioTransport } from '../src/main/acp-stdio-transport.js'
import {
  LUMEN_AGENT_STDIO_ARGS,
  LumenProcessManager,
  resolveLumenBinary,
  sha256OfFile,
} from '../src/main/lumen-process-manager.js'
import { AcpSessionManager } from '../src/main/acp-session-manager.js'

const HERE = path.dirname(fileURLToPath(import.meta.url))
const FAKE_AGENT = path.join(HERE, 'fixtures', 'fake-lumen-agent.mjs')

const CODES = {
  methodNotAllowed: 'LUMEN_METHOD_NOT_ALLOWED',
  remote: 'LUMEN_ACP_REMOTE_ERROR',
  violation: 'LUMEN_ACP_PROTOCOL_VIOLATION',
  closed: 'LUMEN_ACP_TRANSPORT_CLOSED',
  timeout: 'LUMEN_ACP_REQUEST_TIMEOUT',
  cancelled: 'LUMEN_ACP_REQUEST_CANCELLED',
  binaryMissing: 'LUMEN_BINARY_NOT_FOUND',
  hashMismatch: 'LUMEN_BINARY_HASH_MISMATCH',
  unavailable: 'LUMEN_ENGINE_UNAVAILABLE',
} as const

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

function codeOf(error: unknown): string {
  return String((error as { code?: unknown })?.code ?? '(no code)')
}

/** Assert `fn` rejects with `code`, and hand the error back for inspection. */
async function rejectsWith(code: string, fn: () => Promise<unknown>): Promise<Error> {
  let value: unknown
  try {
    value = await fn()
  } catch (e: unknown) {
    strictEqual(codeOf(e), code, `expected ${code}, got ${codeOf(e)}: ${(e as Error).message}`)
    return e as Error
  }
  throw new Error(`expected ${code}, resolved with ${JSON.stringify(value)}`)
}

/** Assert a synchronous call throws with `code`. */
function throwsWith(code: string, fn: () => unknown): Error {
  try {
    fn()
  } catch (e: unknown) {
    strictEqual(codeOf(e), code, `expected ${code}, got ${codeOf(e)}: ${(e as Error).message}`)
    return e as Error
  }
  throw new Error(`expected ${code}, nothing thrown`)
}

async function waitFor(
  predicate: () => boolean,
  what: string,
  timeoutMs = 5_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    if (predicate()) return
    await new Promise((r) => setTimeout(r, 20))
  }
  throw new Error(`timed out waiting for ${what}`)
}

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'lumen-acp-test-'))

// ── registry ─────────────────────────────────────────────────────

// The count is the point: it fails when the engine gains a method nobody
// mirrored here. That is exactly how it caught workflow_execute, which LS5-K8
// added to the engine while the desktop had no way to call it.
await test('registry lists exactly the 30 engine methods', () => {
  strictEqual(SCIENCE_METHODS.length, 30)
  strictEqual(new Set(SCIENCE_METHODS).size, 30, 'no duplicates')
  strictEqual(listScienceMethods().length, 30)
})

await test('registry wire form carries the ACP ext prefix', () => {
  const resolved = resolveScienceMethod('project_list')
  strictEqual(resolved.name, 'project_list')
  strictEqual(resolved.qualified, `${SCIENCE_METHOD_NAMESPACE}project_list`)
  strictEqual(
    resolved.wireMethod,
    `${ACP_EXT_WIRE_PREFIX}${SCIENCE_METHOD_NAMESPACE}project_list`,
  )
  // agent-client-protocol strips the leading `_` before dispatch; without it
  // the real binary answers -32601. Verified live against the built binary.
  strictEqual(resolved.wireMethod, '_x.ai/science/project_list')
})

await test('registry accepts bare, qualified and wire spellings alike', () => {
  for (const spelling of [
    'project_get',
    'x.ai/science/project_get',
    '_x.ai/science/project_get',
  ]) {
    strictEqual(resolveScienceMethod(spelling).name, 'project_get')
    ok(isScienceMethod(spelling))
  }
})

await test('registry rejects an unknown method', () => {
  const error = throwsWith(CODES.methodNotAllowed, () => resolveScienceMethod('totally_made_up'))
  match(error.message, /rejected by registry/)
  strictEqual(isScienceMethod('totally_made_up'), false)
})

await test('project_assert_membership is a real method now', () => {
  // It exists in the Rust dispatch table (extensions/science.rs), so it must
  // resolve to a wire name rather than be refused. A registry that still
  // rejected it would leave the workspace unreachable for a method that works.
  strictEqual(isScienceMethod('project_assert_membership'), true)
  strictEqual(
    resolveScienceMethod('project_assert_membership').wireMethod,
    '_x.ai/science/project_assert_membership',
  )
})

await test('artifact_list is served by the Rust engine now', () => {
  strictEqual(isScienceMethod('artifact_list'), true)
  strictEqual(
    resolveScienceMethod('artifact_list').wireMethod,
    '_x.ai/science/artifact_list',
  )
})

await test('registry rejects the methods that exist in NEITHER engine', () => {
  // project_assert_membership was on this list until LS5-K18 implemented it in
  // the Rust engine. The fix was to add the method, not to keep routing round
  // a name the desktop had invented — so it moved to the allowlist and is
  // asserted there instead, below.
  for (const invented of ['artifact_resolve', 'compute_plan']) {
    throwsWith(CODES.methodNotAllowed, () => resolveScienceMethod(invented))
    const reason = explainRejection(invented)
    ok(reason, `${invented} must be rejected`)
    match(reason, /in either engine/i)
    strictEqual(isScienceMethod(invented), false)
  }
})

await test('registry rejects Go MCP tools with the surface distinction named', () => {
  for (const goTool of ['notebook_execute', 'start_review']) {
    const reason = explainRejection(goTool)
    ok(reason, `${goTool} must be rejected`)
    match(reason, /Go MCP tool/)
    match(reason, /not a Rust ACP extension method/)
  }
})

await test('registry rejects empty and non-string names', () => {
  for (const bad of ['', '   ', null, undefined, 42, {}]) {
    ok(explainRejection(bad as unknown as string), `${JSON.stringify(bad)} must be rejected`)
  }
})

// ── transport ────────────────────────────────────────────────────

type Wired = {
  transport: AcpStdioTransport
  toClient: PassThrough
  sent: string[]
  closed: Error[]
  notifications: Array<{ method: string; params: unknown }>
}

function wire(
  opts: Partial<{ maxFrameBytes: number; timeoutMs: number }> = {},
): Wired {
  const toClient = new PassThrough()
  const fromClient = new PassThrough()
  const sent: string[] = []
  const closed: Error[] = []
  const notifications: Array<{ method: string; params: unknown }> = []
  let pendingOut = ''
  fromClient.on('data', (chunk: Buffer) => {
    pendingOut += chunk.toString('utf8')
    let i = pendingOut.indexOf('\n')
    while (i >= 0) {
      sent.push(pendingOut.slice(0, i))
      pendingOut = pendingOut.slice(i + 1)
      i = pendingOut.indexOf('\n')
    }
  })
  const transport = new AcpStdioTransport({
    input: toClient,
    output: fromClient,
    maxFrameBytes: opts.maxFrameBytes,
    defaultRequestTimeoutMs: opts.timeoutMs ?? 2_000,
    onNotification: (method, params) => notifications.push({ method, params }),
    onClose: (error) => closed.push(error),
  })
  return { transport, toClient, sent, closed, notifications }
}

const settle = (): Promise<void> => new Promise((r) => setImmediate(r))

await test('transport correlates responses by id, out of order', async () => {
  const w = wire()
  const a = w.transport.request('alpha', { n: 1 })
  const b = w.transport.request('beta', { n: 2 })
  await settle()
  strictEqual(w.sent.length, 2)
  deepStrictEqual(JSON.parse(w.sent[0]), {
    jsonrpc: '2.0',
    id: 1,
    method: 'alpha',
    params: { n: 1 },
  })
  // Answer beta first: correlation must be by id, not arrival order.
  w.toClient.write(`${JSON.stringify({ jsonrpc: '2.0', id: 2, result: 'B' })}\n`)
  w.toClient.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, result: 'A' })}\n`)
  strictEqual(await b, 'B')
  strictEqual(await a, 'A')
  strictEqual(w.transport.inFlight, 0, 'no request may stay pending')
  w.transport.close('done')
})

await test('transport handles a frame split across chunk boundaries', async () => {
  const w = wire()
  const call = w.transport.request('x', {})
  await settle()
  const line = JSON.stringify({ jsonrpc: '2.0', id: 1, result: { deep: 'value' } })
  w.toClient.write(line.slice(0, 7))
  await settle()
  w.toClient.write(`${line.slice(7)}\n`)
  deepStrictEqual(await call, { deep: 'value' })
  w.transport.close('done')
})

await test('transport surfaces a JSON-RPC error as a rejection with its code', async () => {
  const w = wire()
  const call = w.transport.request('x', {})
  await settle()
  w.toClient.write(
    `${JSON.stringify({ jsonrpc: '2.0', id: 1, error: { code: -32601, message: 'Method not found' } })}\n`,
  )
  const error = await rejectsWith(CODES.remote, () => call)
  strictEqual((error as unknown as { rpcCode: number }).rpcCode, -32601)
  w.transport.close('done')
})

await test('transport delivers notifications without disturbing correlation', async () => {
  const w = wire()
  const call = w.transport.request('x', {})
  await settle()
  w.toClient.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'note', params: { a: 1 } })}\n`)
  w.toClient.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, result: 'ok' })}\n`)
  strictEqual(await call, 'ok')
  deepStrictEqual(w.notifications, [{ method: 'note', params: { a: 1 } }])
  w.transport.close('done')
})

await test('MALFORMED JSON on stdout fails closed', async () => {
  const w = wire()
  const call = w.transport.request('x', {})
  await settle()
  w.toClient.write('not json at all\n')
  const error = await rejectsWith(CODES.violation, () => call)
  match(error.message, /stdout must carry protocol only/)
  strictEqual(w.transport.isOpen, false, 'transport must be dead')
  strictEqual(w.closed.length, 1, 'onClose fires exactly once')
  // And it stays closed: no silent recovery.
  await rejectsWith(CODES.violation, () => w.transport.request('y', {}))
})

await test('a JSON array line is also a violation (must be an object)', async () => {
  const w = wire()
  const call = w.transport.request('x', {})
  await settle()
  w.toClient.write('[1,2,3]\n')
  const error = await rejectsWith(CODES.violation, () => call)
  match(error.message, /not a JSON-RPC object/)
})

await test('OVERSIZED unterminated frame terminates the transport', async () => {
  const w = wire({ maxFrameBytes: 256 })
  const call = w.transport.request('x', {})
  await settle()
  w.toClient.write('x'.repeat(1024)) // no newline: would grow forever
  const error = await rejectsWith(CODES.violation, () => call)
  match(error.message, /unterminated frame exceeded 256 bytes/)
  strictEqual(w.transport.isOpen, false)
})

await test('OVERSIZED complete frame terminates the transport', async () => {
  const w = wire({ maxFrameBytes: 256 })
  const call = w.transport.request('x', {})
  await settle()
  w.toClient.write(`${'y'.repeat(1024)}\n`)
  const error = await rejectsWith(CODES.violation, () => call)
  match(error.message, /exceeds 256/)
})

await test('outbound frame over the cap is refused before it is written', async () => {
  const w = wire({ maxFrameBytes: 256 })
  const error = await rejectsWith(CODES.violation, () =>
    w.transport.request('x', { blob: 'z'.repeat(1024) }),
  )
  match(error.message, /outbound frame/)
  strictEqual(w.sent.length, 0, 'nothing may reach the child')
  w.transport.close('done')
})

await test('a response for an id never issued is a violation', async () => {
  const w = wire()
  const call = w.transport.request('x', {})
  await settle()
  w.toClient.write(`${JSON.stringify({ jsonrpc: '2.0', id: 4242, result: 'stray' })}\n`)
  const error = await rejectsWith(CODES.violation, () => call)
  match(error.message, /unknown id 4242/)
})

await test('a late response for a timed-out id is dropped, not a violation', async () => {
  const w = wire({ timeoutMs: 25 })
  const call = w.transport.request('slow', {})
  await rejectsWith(CODES.timeout, () => call)
  w.toClient.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, result: 'late' })}\n`)
  await settle()
  strictEqual(w.transport.isOpen, true, 'a late answer must not kill the transport')
  strictEqual(w.closed.length, 0)
  w.transport.close('done')
})

await test('request timeout rejects and leaves no pending entry', async () => {
  const w = wire({ timeoutMs: 20 })
  const error = await rejectsWith(CODES.timeout, () => w.transport.request('slow', {}))
  match(error.message, /timed out after 20ms/)
  strictEqual(w.transport.inFlight, 0)
  w.transport.close('done')
})

await test('cancellation via AbortSignal rejects the request', async () => {
  const w = wire()
  const controller = new AbortController()
  const call = w.transport.request('x', {}, { signal: controller.signal })
  await settle()
  controller.abort()
  await rejectsWith(CODES.cancelled, () => call)
  strictEqual(w.transport.inFlight, 0)
  strictEqual(w.transport.isOpen, true, 'cancelling one request must not kill the transport')
  w.transport.close('done')
})

await test('an already-aborted signal rejects without writing anything', async () => {
  const w = wire()
  const controller = new AbortController()
  controller.abort()
  await rejectsWith(CODES.cancelled, () =>
    w.transport.request('x', {}, { signal: controller.signal }),
  )
  await settle()
  strictEqual(w.sent.length, 0)
  w.transport.close('done')
})

await test('agent->client request is refused with -32601 when no responder', async () => {
  const w = wire()
  w.toClient.write(
    `${JSON.stringify({ jsonrpc: '2.0', id: 77, method: 'session/request_permission', params: {} })}\n`,
  )
  await settle()
  const reply = JSON.parse(w.sent[0]) as {
    id: number
    error: { code: number; message: string }
  }
  strictEqual(reply.id, 77)
  strictEqual(reply.error.code, -32601)
  match(reply.error.message, /client does not implement/)
  w.transport.close('done')
})

await test('closing rejects every in-flight request', async () => {
  const w = wire()
  const a = w.transport.request('a', {})
  const b = w.transport.request('b', {})
  await settle()
  w.transport.close('shutdown for test')
  const ea = await rejectsWith(CODES.closed, () => a)
  const eb = await rejectsWith(CODES.closed, () => b)
  match(ea.message, /shutdown for test/)
  match(eb.message, /shutdown for test/)
  strictEqual(w.transport.inFlight, 0)
})

// ── process manager: binary resolution + hash pinning ────────────

const fixtureBin = path.join(tmp, 'fake-bin')
fs.writeFileSync(fixtureBin, '#!/bin/sh\nexit 0\n', { mode: 0o755 })
const fixtureHash = sha256OfFile(fixtureBin)

await test('resolveLumenBinary prefers LUMEN_BINARY and hashes it', () => {
  const resolved = resolveLumenBinary({ env: { LUMEN_BINARY: fixtureBin } })
  strictEqual(resolved.source, 'env')
  strictEqual(resolved.binaryPath, path.resolve(fixtureBin))
  strictEqual(resolved.sha256, fixtureHash)
  match(resolved.sha256, /^[a-f0-9]{64}$/)
})

await test('a LUMEN_BINARY that does not exist is an error, not a PATH fallback', () => {
  const binDir = path.join(tmp, 'pathbin')
  fs.mkdirSync(binDir, { recursive: true })
  fs.copyFileSync(fixtureBin, path.join(binDir, 'lumen'))
  fs.chmodSync(path.join(binDir, 'lumen'), 0o755)
  const error = throwsWith(CODES.binaryMissing, () =>
    resolveLumenBinary({ env: { LUMEN_BINARY: path.join(tmp, 'nope'), PATH: binDir } }),
  )
  match(error.message, /is not a file/, 'must not silently run a different binary')
})

await test('resolveLumenBinary falls back bundled -> PATH', () => {
  const resources = path.join(tmp, 'resources')
  fs.mkdirSync(path.join(resources, 'bin'), { recursive: true })
  const bundled = path.join(resources, 'bin', 'lumen')
  fs.copyFileSync(fixtureBin, bundled)
  const fromBundle = resolveLumenBinary({ env: {}, resourcesPath: resources, platform: 'darwin' })
  strictEqual(fromBundle.source, 'bundled')
  strictEqual(fromBundle.binaryPath, bundled)

  const binDir = path.join(tmp, 'pathbin')
  const fromPath = resolveLumenBinary({ env: { PATH: binDir }, platform: 'darwin' })
  strictEqual(fromPath.source, 'path')
  strictEqual(fromPath.binaryPath, path.join(binDir, 'lumen'))
})

await test('no binary anywhere is an explicit error', () => {
  const error = throwsWith(CODES.binaryMissing, () =>
    resolveLumenBinary({ env: { PATH: path.join(tmp, 'empty-dir') } }),
  )
  match(error.message, /set LUMEN_BINARY/)
})

await test('BINARY HASH MISMATCH is rejected before the child is spawned', async () => {
  const argvFile = path.join(tmp, 'argv-hash-mismatch.json')
  const manager = new LumenProcessManager({
    cwd: tmp,
    env: { LUMEN_BINARY: FAKE_AGENT },
    expectedSha256: 'f'.repeat(64),
    childEnv: { FAKE_LUMEN_ARGV_FILE: argvFile },
  })
  const error = throwsWith(CODES.hashMismatch, () => manager.start())
  match(error.message, /refusing to spawn/)
  strictEqual(fs.existsSync(argvFile), false, 'the child must never have run')
  strictEqual(manager.running, false)
})

await test('a malformed expected hash is also rejected', () => {
  const manager = new LumenProcessManager({
    cwd: tmp,
    env: { LUMEN_BINARY: FAKE_AGENT },
    expectedSha256: 'not-a-hash',
  })
  throwsWith(CODES.hashMismatch, () => manager.start())
})

await test('a matching expected hash spawns, and spawns `agent stdio`', async () => {
  const argvFile = path.join(tmp, 'argv-good.json')
  const manager = new LumenProcessManager({
    cwd: tmp,
    env: { LUMEN_BINARY: FAKE_AGENT },
    expectedSha256: sha256OfFile(FAKE_AGENT),
    childEnv: { FAKE_LUMEN_MODE: 'silent', FAKE_LUMEN_ARGV_FILE: argvFile },
    shutdownGraceMs: 200,
  })
  manager.start()
  await waitFor(() => fs.existsSync(argvFile), 'the child to record its argv')
  deepStrictEqual(
    JSON.parse(fs.readFileSync(argvFile, 'utf8')),
    [...LUMEN_AGENT_STDIO_ARGS],
    'production args must be `agent stdio` — `serve --port 17000` never existed',
  )
  await manager.stop()
  ok(manager.exited, 'stop must observe the exit')
})

await test('a crashing child is observed with its exit code and stderr tail', async () => {
  let observed: Error | null = null
  const manager = new LumenProcessManager({
    cwd: tmp,
    env: { LUMEN_BINARY: FAKE_AGENT },
    childEnv: { FAKE_LUMEN_MODE: 'crash' },
    onExit: (error) => {
      observed = error
    },
  })
  manager.start()
  await waitFor(() => observed !== null, 'the crash to be observed')
  const error = observed as unknown as Error
  strictEqual(codeOf(error), 'LUMEN_PROCESS_EXITED')
  match(error.message, /exited with code 3/)
  match(error.message, /scripted crash/, 'stderr tail must be carried, not dropped')
  await manager.stop()
})

// ── session manager: crash surfaces unavailable, never a mock ────

function sessionManager(mode: string, extra: Record<string, string> = {}): AcpSessionManager {
  return new AcpSessionManager({
    cwd: tmp,
    handshakeTimeoutMs: 8_000,
    requestTimeoutMs: 4_000,
    process: {
      env: { LUMEN_BINARY: FAKE_AGENT },
      childEnv: { FAKE_LUMEN_MODE: mode, ...extra },
      shutdownGraceMs: 200,
    },
  })
}

await test('handshake reaches ready and injects sessionId into science calls', async () => {
  const manager = sessionManager('good')
  const state = await manager.start()
  strictEqual(state.status, 'ready')
  strictEqual(state.sessionId, 'fake-session-1')
  match(state.binaryHash ?? '', /^[a-f0-9]{64}$/)

  const result = (await manager.callScience('project_list', { storeRoot: '/tmp/store' })) as {
    method: string
    params: Record<string, unknown>
  }
  strictEqual(result.method, '_x.ai/science/project_list')
  strictEqual(result.params.sessionId, 'fake-session-1')
  strictEqual(result.params.storeRoot, '/tmp/store')
  await manager.stop()
})

await test('a caller-supplied sessionId is never overwritten', async () => {
  const manager = sessionManager('good')
  await manager.start()
  const result = (await manager.callScience('project_get', { sessionId: 'caller-owned' })) as {
    params: Record<string, unknown>
  }
  strictEqual(result.params.sessionId, 'caller-owned')
  await manager.stop()
})

await test('the registry rejects before the wire: no engine needed', async () => {
  const manager = sessionManager('good')
  // Deliberately NOT started. A rejected name must fail on the name, not on
  // "engine unavailable" — that distinction is the whole point.
  const error = await rejectsWith(CODES.methodNotAllowed, () =>
    manager.callScience('compute_plan', {}),
  )
  match(error.message, /in either engine/i)
  await manager.stop()
})

await test('CHILD CRASH surfaces an explicit unavailable state, not a mock', async () => {
  const manager = sessionManager('crash')
  await rejectsWith(CODES.closed, () => manager.start())
  const state = manager.getState()
  strictEqual(state.status, 'unavailable')
  ok(state.reason && state.reason.length > 0, 'unavailable must carry a reason')

  const callError = await rejectsWith(CODES.unavailable, () =>
    manager.callScience('project_list', {}),
  )
  strictEqual(
    (callError as unknown as { state: { status: string } }).state.status,
    'unavailable',
  )
  await manager.stop()
})

await test('a mid-call crash rejects the in-flight call and marks unavailable', async () => {
  const manager = sessionManager('crash-mid')
  await manager.start()
  await rejectsWith(CODES.closed, () => manager.callScience('project_list', {}))
  await waitFor(() => manager.getState().status === 'unavailable', 'unavailable state')
  await rejectsWith(CODES.unavailable, () => manager.callScience('project_list', {}))
  await manager.stop()
})

await test('garbage on the engine stdout marks the engine unavailable', async () => {
  const manager = sessionManager('garbage')
  await manager.start()
  await rejectsWith(CODES.violation, () => manager.callScience('project_list', {}))
  strictEqual(manager.getState().status, 'unavailable')
  await manager.stop()
})

await test('an oversized engine frame marks the engine unavailable', async () => {
  const manager = new AcpSessionManager({
    cwd: tmp,
    handshakeTimeoutMs: 8_000,
    requestTimeoutMs: 4_000,
    maxFrameBytes: 1024,
    process: {
      env: { LUMEN_BINARY: FAKE_AGENT },
      childEnv: { FAKE_LUMEN_MODE: 'huge' },
      shutdownGraceMs: 200,
    },
  })
  await manager.start()
  const error = await rejectsWith(CODES.violation, () =>
    manager.callScience('project_list', {}),
  )
  match(error.message, /exceeded 1024 bytes/)
  strictEqual(manager.getState().status, 'unavailable')
  await manager.stop()
})

await test('an agent->client request during a call is refused, not hung', async () => {
  const manager = sessionManager('ask')
  await manager.start()
  const result = (await manager.callScience('project_list', {})) as { asked: boolean }
  strictEqual(result.asked, true)
  await manager.stop()
})

await test('a silent engine times the handshake out and stays diagnosable', async () => {
  const manager = new AcpSessionManager({
    cwd: tmp,
    handshakeTimeoutMs: 400,
    process: {
      env: { LUMEN_BINARY: FAKE_AGENT },
      childEnv: { FAKE_LUMEN_MODE: 'silent' },
      shutdownGraceMs: 200,
    },
  })
  await rejectsWith(CODES.timeout, () => manager.start())
  strictEqual(manager.getState().status, 'unavailable')
  await manager.stop()
})

await test('stop() is graceful and leaves nothing running', async () => {
  const manager = sessionManager('good')
  await manager.start()
  await manager.stop()
  strictEqual(manager.getState().status, 'stopped')
  await rejectsWith(CODES.unavailable, () => manager.callScience('project_list', {}))
})

try {
  fs.rmSync(tmp, { recursive: true, force: true })
} catch {
  /* ignore */
}

console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
process.exit(failures > 0 ? 1 : 0)
