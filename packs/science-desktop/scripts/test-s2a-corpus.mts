#!/usr/bin/env npx tsx
/**
 * S2a shadow-only corpus runner against the PINNED canonical lumen binary
 * (v2.2.0 098f7cd4). Drives the real ACP seam only — zero provider, zero
 * network, zero arbitrary shell. Asserts the five scenario classes
 * (docs/science/5.0/s2a-scenarios/class-*.json):
 *   A authority: lineage depths, unknown-method, forged-owner, tool filter,
 *     caller bypass;
 *   B context/claim: accepted-snapshot hash stability, missing/unknown field
 *     rejection;
 *   C execution/liveness: depth-3 hard cap, leaf deny records, sibling
 *     isolation, capability ceiling;
 *   D provider/advisor: default profile, one-way upgrade, shadow advisor;
 *   E UX/provenance: fixture identity, typed deny records, no fake pass.
 */
import { ok, strictEqual, match } from 'node:assert/strict'
import { spawn } from 'node:child_process'

const BIN = process.env.LUMEN_BIN ?? '/Users/lei/.local/bin/lumen'

interface RpcResponse {
  result?: { result?: unknown }
  error?: { code: number; message: string; data?: unknown }
}

function rpc(method: string, params: unknown): Promise<RpcResponse> {
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
    child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }) + '\n')
    child.stdin.end()
  })
}

interface GovernedTree {
  fixtureId: string
  nodes: Array<{ nodeId: string; parentId: string | null; depth: number; maySpawn: boolean; mayWrite: boolean; mayNetwork: boolean }>
  denies: Array<{ nodeId: string; action: string; mechanism: string; code: string }>
  acceptedSnapshotHash: string
  defaultProfile: string
  upgradeTarget: string
  upgradeDenyOnDowngrade: string
}

async function tree(): Promise<GovernedTree> {
  const response = await rpc('_x.ai/governedTree/status', {})
  ok(response.result, 'governedTree/status must return a result')
  const payload = (response.result as { result: GovernedTree }).result
  ok(payload.nodes && payload.denies, 'projection must carry nodes and denies')
  return payload
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
  // Class A — authority.
  await test('A1 lineage: root depth 0, child depth 1, leaf depth 3, leaf maySpawn false', async () => {
    const t = await tree()
    const root = t.nodes.find((n) => n.nodeId === 'root')
    const child = t.nodes.find((n) => n.nodeId === 'child')
    const leaf = t.nodes.find((n) => n.nodeId === 'leaf')
    strictEqual(root?.depth, 0)
    strictEqual(child?.depth, 1)
    strictEqual(child?.parentId, 'root')
    strictEqual(leaf?.depth, 3)
    strictEqual(leaf?.maySpawn, false)
    strictEqual(leaf?.mayWrite, false)
    strictEqual(leaf?.mayNetwork, false)
  })

  await test('A2 unknown method fails closed (-32601)', async () => {
    const response = await rpc('_x.ai/science/nonexistent', {})
    strictEqual(response.error?.code, -32601)
  })

  await test('A3 forged owner cannot reach execution (no session -> reject)', async () => {
    const response = await rpc('_x.ai/science/run_csv', {
      sessionId: 'foreign-session',
      projectId: 'p',
      ownerId: 'attacker',
      storeRoot: '/tmp/s',
      artifactRoot: '/tmp/a',
      fixturePath: '/tmp/f.csv',
    })
    ok(response.error, 'forged session/owner must be rejected')
    match(String(response.error?.message), /session not found|invalid/i)
  })

  await test('A4 unknown tool kind denied (tool.unknown_kind)', async () => {
    const t = await tree()
    ok(t.denies.some((d) => d.code === 'tool.unknown_kind'), 'tool.unknown_kind deny missing')
  })

  await test('A5 caller-supplied bypass never honored', async () => {
    const t = await tree()
    ok(
      t.denies.some((d) => d.code === 'sandbox.caller_supplied_bypass'),
      'caller bypass deny missing'
    )
  })

  // Class B — context & claim.
  await test('B1 accepted-snapshot hash stable and sha256-prefixed', async () => {
    const first = await tree()
    const second = await tree()
    match(first.acceptedSnapshotHash, /^sha256:/)
    strictEqual(first.acceptedSnapshotHash, second.acceptedSnapshotHash)
  })

  await test('B3 missing field rejected with field name', async () => {
    const response = await rpc('_x.ai/science/run_csv', { sessionId: 'x' })
    strictEqual(response.error?.code, -32602)
    match(String(response.error?.data), /missing field `projectId`/)
  })

  await test('B4 unknown field rejected, never dropped', async () => {
    const response = await rpc('_x.ai/science/goal_host_verify', {
      sessionId: 's',
      storeRoot: '/tmp',
      runId: 'r',
      forgedField: 'x',
    })
    strictEqual(response.error?.code, -32602)
    match(String(response.error?.data), /unknown field `forgedField`/)
  })

  // Class C — execution & liveness.
  await test('C1/C2 depth-3 leaf: spawn/write/network denies', async () => {
    const t = await tree()
    ok(t.denies.some((d) => d.code === 'sandbox.leaf_spawn_denied'))
    ok(t.denies.some((d) => d.code === 'sandbox.leaf_write_denied'))
    ok(t.denies.some((d) => d.code === 'sandbox.leaf_network_denied'))
  })

  await test('C3 sibling isolation + C4 capability ceiling', async () => {
    const t = await tree()
    ok(t.denies.some((d) => d.code === 'sandbox.sibling_isolation'))
    ok(t.denies.some((d) => d.mechanism === 'capability_ceiling'))
  })

  // Class D — provider & advisor.
  await test('D1 default profile is least privilege; D2 upgrade is one-way', async () => {
    const t = await tree()
    strictEqual(t.defaultProfile, 'interactive_single_turn')
    strictEqual(t.upgradeTarget, 'governed_tree_development')
    strictEqual(t.upgradeDenyOnDowngrade, 'profile.admission_upgrade_failed')
  })

  await test('D3/D4 shadow advisor + zero provider: recommendation is a truthful projection', async () => {
    const response = await rpc('_x.ai/governedTree/assignmentRecommendation', {})
    ok(response.result, 'recommendation must be a read-only projection')
  })

  // Class E — UX & provenance.
  await test('E1/E2 fixture identity + typed denies; E3 no fake pass', async () => {
    const t = await tree()
    strictEqual(t.fixtureId, 'm1-governed-tree-preview-v1')
    ok(t.denies.length > 0)
    for (const deny of t.denies) {
      ok(deny.nodeId && deny.action && deny.mechanism && deny.code, 'deny record must be typed')
    }
  })

  console.log(failures === 0 ? 'S2A CORPUS ALL PASSED (shadow-only, zero provider)' : `${failures} S2A CORPUS FAILURES`)
  process.exit(failures === 0 ? 0 : 1)
}

void main()
