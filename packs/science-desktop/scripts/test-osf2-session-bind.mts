#!/usr/bin/env npx tsx
/**
 * OSF-2 session bind + artifact store seed.
 *
 * Trusted identity is set only after membership assertion (ACP or fixture).
 * Renderer cannot self-attest into another owner's project.
 *
 * Run: npx tsx scripts/test-osf2-session-bind.mts
 */
import { strictEqual, ok } from 'node:assert/strict'
import {
  bindTrustedSession,
  unbindTrustedSession,
  seedPreviewStoreFromList,
  type MembershipAsserter,
  type ArtifactListItem,
} from '../src/main/files/session-binding.js'
import {
  getTrustedPreviewContext,
  clearTrustedPreviewContext,
} from '../src/main/files/session-identity.js'
import { AcpPreviewStore } from '../src/main/files/acp-preview-store.js'
import { loadArtifactPreview } from '../src/main/files/preview-service.js'
import {
  registerScienceIpcHandlers,
  type IpcMainLike,
  type SafeHandleFn,
} from '../src/main/files/science-ipc.js'
import { validateIpcChannel } from '../src/main/lumen-authority-policy.js'

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

const allowO1P1: MembershipAsserter = async (claim) => {
  if (claim.projectId === 'p1' && claim.ownerId === 'o1') {
    return { ok: true, ownerId: 'o1', projectId: 'p1' }
  }
  return { ok: false, reason: 'membership denied' }
}

const denyAll: MembershipAsserter = async () => ({
  ok: false,
  reason: 'membership denied',
})

async function run() {
  clearTrustedPreviewContext()

  // ── Pure bind ────────────────────────────────────────────────
  const denied = await bindTrustedSession(
    { ownerId: 'evil', projectId: 'p1' },
    { assertMembership: denyAll },
  )
  await test('bind: rejects failed membership', () => {
    ok(!denied.ok)
    strictEqual(getTrustedPreviewContext(), null)
  })

  const bound = await bindTrustedSession(
    { ownerId: 'o1', projectId: 'p1' },
    { assertMembership: allowO1P1 },
  )
  await test('bind: accepts asserted membership', () => {
    ok(bound.ok)
    const ctx = getTrustedPreviewContext()
    ok(ctx)
    strictEqual(ctx!.ownerId, 'o1')
    strictEqual(ctx!.projectId, 'p1')
  })

  unbindTrustedSession()
  await test('unbind: clears identity', () => {
    strictEqual(getTrustedPreviewContext(), null)
  })

  // ── Seed store from artifact_list shape ──────────────────────
  const store = new AcpPreviewStore()
  const items: ArtifactListItem[] = [
    {
      artifact_id: 'art-1',
      path: '/data/p1/run1/out.csv',
      sha256: 'hash1',
      project_id: 'p1',
    },
    {
      artifact_id: 'art-2',
      path: '/data/p1/run1/seq.fa',
      sha256: 'hash2',
    },
  ]
  const seeded = seedPreviewStoreFromList(store, items, {
    ownerId: 'o1',
    projectId: 'p1',
  })
  await test('seed: indexes two artifacts', () => {
    strictEqual(seeded, 2)
  })

  await bindTrustedSession(
    { ownerId: 'o1', projectId: 'p1' },
    { assertMembership: allowO1P1 },
  )
  const preview = await loadArtifactPreview(
    { artifactId: 'art-1', expectedSha256: 'hash1' },
    { store },
  )
  await test('seed+bind: preview resolves seeded artifact', () => {
    ok(preview.access.ok, JSON.stringify(preview))
    strictEqual(preview.path, '/data/p1/run1/out.csv')
  })

  // Cross-owner still blocked even with seed
  unbindTrustedSession()
  await bindTrustedSession(
    { ownerId: 'o2', projectId: 'p2' },
    {
      assertMembership: async (c) =>
        c.ownerId === 'o2' && c.projectId === 'p2'
          ? { ok: true, ownerId: 'o2', projectId: 'p2' }
          : { ok: false, reason: 'no' },
    },
  )
  const blocked = await loadArtifactPreview({ artifactId: 'art-1' }, { store })
  await test('seed: other session cannot preview p1 artifact', () => {
    ok(!blocked.access.ok)
    ok((blocked.access.reason ?? '').includes('owner'))
  })
  unbindTrustedSession()

  // ── IPC product path ─────────────────────────────────────────
  const handlers = new Map<string, Function>()
  const ipc: IpcMainLike = {
    handle(ch, h) {
      if (handlers.has(ch)) throw new Error(`second handler for ${ch}`)
      handlers.set(ch, h)
    },
  }
  const safeHandle: SafeHandleFn = (m, ch, h) => {
    if (!validateIpcChannel(ch)) throw new Error(`banned ${ch}`)
    m.handle(ch, h)
  }

  const ipcStore = new AcpPreviewStore()
  let membershipCalls = 0
  let listCalls = 0

  registerScienceIpcHandlers(ipc, {
    safeHandle,
    getLumenBinaryHash: () => 'h',
    previewStore: ipcStore,
    assertMembership: async (claim) => {
      membershipCalls++
      if (claim.ownerId === 'o1' && claim.projectId === 'p1') {
        return { ok: true, ownerId: 'o1', projectId: 'p1' }
      }
      return { ok: false, reason: 'no membership' }
    },
    listArtifacts: async ({ projectId, runId }) => {
      listCalls++
      strictEqual(projectId, 'p1')
      strictEqual(runId, 'run-1')
      return [
        {
          artifact_id: 'ipc-a1',
          path: '/ipc/a1.csv',
          sha256: 'ipc-hash',
          project_id: 'p1',
        },
      ]
    },
  })

  await test('policy: allows files:bind-session', () => {
    ok(validateIpcChannel('files:bind-session'))
  })
  await test('policy: allows files:unbind-session', () => {
    ok(validateIpcChannel('files:unbind-session'))
  })
  await test('ipc: registers bind/unbind', () => {
    ok(handlers.has('files:bind-session'))
    ok(handlers.has('files:unbind-session'))
  })

  clearTrustedPreviewContext()
  const bindHandler = handlers.get('files:bind-session')!
  const bindDeny = await bindHandler({}, {
    ownerId: 'evil',
    projectId: 'p1',
    runId: 'run-1',
  })
  await test('ipc bind: membership denial does not set identity', () => {
    ok(bindDeny && bindDeny.ok === false)
    strictEqual(getTrustedPreviewContext(), null)
    ok(membershipCalls >= 1)
  })

  const bindOk = await bindHandler({}, {
    ownerId: 'o1',
    projectId: 'p1',
    runId: 'run-1',
  })
  await test('ipc bind: success sets identity and seeds', () => {
    ok(bindOk.ok, JSON.stringify(bindOk))
    strictEqual(bindOk.seeded, 1)
    ok(listCalls >= 1)
    const ctx = getTrustedPreviewContext()
    strictEqual(ctx?.ownerId, 'o1')
  })

  const previewHandler = handlers.get('files:preview-by-artifact')!
  const prev = await previewHandler({}, {
    artifactId: 'ipc-a1',
    expectedSha256: 'ipc-hash',
  })
  await test('ipc: preview after bind+seed works', () => {
    ok(prev.access.ok, JSON.stringify(prev))
    strictEqual(prev.path, '/ipc/a1.csv')
  })

  const unbindHandler = handlers.get('files:unbind-session')!
  await unbindHandler({})
  const afterUnbind = await previewHandler({}, { artifactId: 'ipc-a1' })
  await test('ipc unbind: preview denied without session', () => {
    ok(!afterUnbind.access.ok)
  })

  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

run()
