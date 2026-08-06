#!/usr/bin/env npx tsx
/**
 * X-C1 live seam negatives against the PINNED canonical lumen binary
 * (v2.2.0, A=098f7cd4, sha256 f1aa4061...). These drive the REAL engine
 * dispatch (acp_agent.rs prefix match -> extensions/science.rs /
 * extensions/governed_tree.rs), not a mock or re-implementation:
 *
 *   - an unknown method fails closed (-32601 Method not found)
 *   - a known method with missing required fields fails closed (-32602
 *     Invalid params, serde missing-field message)
 *   - governedTree/status returns the real three-node tree projection with
 *     the typed deny records (depth cap, capability ceiling, tool filter,
 *     caller-supplied bypass) — proving the v1 catalog's read surface.
 *
 * Run: LUMEN_BIN=/Users/lei/.local/bin/lumen npx tsx scripts/test-platform-api-live.mts
 */
import { ok, strictEqual, match } from 'node:assert/strict'
import { spawn } from 'node:child_process'

const BIN = process.env.LUMEN_BIN ?? '/Users/lei/.local/bin/lumen'

function rpc(method: string, params: unknown): Promise<{ result?: unknown; error?: { code: number; message: string } }> {
  return new Promise((resolve, reject) => {
    const child = spawn(BIN, ['agent', 'stdio'], { stdio: ['pipe', 'pipe', 'pipe'] })
    let stdout = ''
    child.stdout.on('data', (chunk) => {
      stdout += chunk.toString()
      const line = stdout.split('\n').find((part) => part.trim().startsWith('{"jsonrpc"'))
      if (line) {
        child.kill()
        resolve(JSON.parse(line))
      }
    })
    child.on('error', reject)
    child.stdin.write(
      JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }) + '\n'
    )
    child.stdin.end()
  })
}

let failures = 0
function test(name: string, fn: () => Promise<void> | void) {
  return Promise.resolve()
    .then(fn)
    .then(() => console.log(`OK  ${name}`))
    .catch((error: unknown) => {
      failures++
      console.log(`FAIL ${name}: ${(error as Error).message}`)
    })
}

const main = async (): Promise<void> => {
  await test('unknown method fails closed (-32601)', async () => {
    const response = await rpc('_x.ai/science/nonexistent', { sessionId: 'x' })
    strictEqual(response.error?.code, -32601)
    match(response.error?.message ?? '', /Method not found/)
  })

  await test('unknown governedTree method fails closed (-32601)', async () => {
    const response = await rpc('_x.ai/governedTree/nope', {})
    strictEqual(response.error?.code, -32601)
  })

  await test('known method with missing required field fails closed (-32602)', async () => {
    const response = await rpc('_x.ai/science/run_csv', { sessionId: 'x' })
    strictEqual(response.error?.code, -32602)
    match(response.error?.data as string, /missing field `projectId`/)
  })

  await test('known method with unknown field fails closed (-32602 deny_unknown_fields)', async () => {
    const response = await rpc('_x.ai/science/goal_host_verify', {
      sessionId: 's',
      storeRoot: '/tmp',
      runId: 'r',
      forgedField: 'x',
    })
    strictEqual(response.error?.code, -32602)
    match(response.error?.data as string, /unknown field `forgedField`/)
  })

  await test('governedTree/status returns the real three-node projection with typed denies', async () => {
    const response = await rpc('_x.ai/governedTree/status', {})
    ok(response.result, 'governedTree/status must return a result')
    const result = (response.result as { result: { nodes: Array<{ nodeId: string; depth: number; maySpawn: boolean }>; denies: Array<{ code: string }>; acceptedSnapshotHash: string } }).result
    strictEqual(result.nodes.length, 3)
    strictEqual(result.nodes[0].nodeId, 'root')
    strictEqual(result.nodes[2].nodeId, 'leaf')
    strictEqual(result.nodes[2].depth, 3)
    strictEqual(result.nodes[2].maySpawn, false)
    ok(result.acceptedSnapshotHash.startsWith('sha256:'))
    const codes = result.denies.map((deny) => deny.code)
    ok(codes.includes('sandbox.leaf_spawn_denied'))
    ok(codes.includes('tool.unknown_kind'))
    ok(codes.includes('sandbox.caller_supplied_bypass'))
  })

  console.log(failures === 0 ? 'ALL LIVE SEAM NEGATIVES PASSED' : `${failures} LIVE SEAM NEGATIVES FAILED`)
  process.exit(failures === 0 ? 0 : 1)
}

void main()
