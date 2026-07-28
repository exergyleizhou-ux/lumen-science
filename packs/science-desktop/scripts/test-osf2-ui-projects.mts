#!/usr/bin/env npx tsx
/**
 * UI project catalog + open-ui-project (bind + seed) product path.
 * Run: npx tsx scripts/test-osf2-ui-projects.mts
 */
import { strictEqual, ok } from 'node:assert/strict'
import { LocalProjectCatalog } from '../src/main/files/local-project-catalog.js'
import { createOfflineCatalogMembershipAsserter } from '../src/main/files/hybrid-membership.js'
import { AcpPreviewStore } from '../src/main/files/acp-preview-store.js'
import {
  registerScienceIpcHandlers,
  type IpcMainLike,
  type SafeHandleFn,
} from '../src/main/files/science-ipc.js'
import { validateIpcChannel } from '../src/main/lumen-authority-policy.js'
import {
  clearTrustedPreviewContext,
  getTrustedPreviewContext,
} from '../src/main/files/session-identity.js'

// Real fixture file: the resolver reads the bytes.
import osFix from 'node:os'
import fsFix from 'node:fs'
import pathFix from 'node:path'
const LIST_FIXTURE = pathFix.join(fsFix.mkdtempSync(pathFix.join(osFix.tmpdir(), 'list-fixture-')), 'from-list.json')
fsFix.writeFileSync(LIST_FIXTURE, '{"from": "list"}\n')
const LIST_SHA = '4f10580de4828369a65aea0b62757eaae3e887f5b1c215696585ed53e59b3773'


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

const safeHandle: SafeHandleFn = (ipc, ch, h) => {
  if (!validateIpcChannel(ch)) throw new Error(`banned ${ch}`)
  ipc.handle(ch, h)
}

async function run() {
  for (const ch of [
    'files:list-ui-projects',
    'files:create-ui-project',
    'files:open-ui-project',
    'files:delete-ui-project',
  ]) {
    await test(`policy allows ${ch}`, () => ok(validateIpcChannel(ch)))
  }

  const engineCalls: Record<string, unknown>[] = []
  const catalog = new LocalProjectCatalog()
  const store = new AcpPreviewStore()
  store.put('seed-1', {
    path: '/tmp/seed-1.csv',
    sha256: 's1',
    ownerId: 'local-user',
    projectId: 'will-replace',
  })

  const handlers = new Map<string, Function>()
  const ipc: IpcMainLike = {
    handle(ch, h) {
      if (handlers.has(ch)) throw new Error(`dup ${ch}`)
      handlers.set(ch, h)
    },
  }

  clearTrustedPreviewContext()
  registerScienceIpcHandlers(ipc, {
    safeHandle,
    getLumenBinaryHash: () => 'abc',
    previewStore: store,
    assertMembership: createOfflineCatalogMembershipAsserter({ catalog }),
    // Projects are created in the ENGINE first, so a test without one is a
    // test of the fail-closed path (asserted below), not of creation.
    callScienceTool: async (tool: string, args: Record<string, unknown>) => {
      if (tool !== 'project_create') throw new Error(`unexpected tool ${tool}`)
      engineCalls.push(args)
      return { projectId: 'engine-assigned-id' }
    },
    projectCatalog: catalog,
    defaultOwnerId: 'local-user',
    listArtifacts: async () => [
      {
        artifact_id: 'from-list',
        path: LIST_FIXTURE,
        sha256: LIST_SHA,
        run_id: 'ui-default-run',
      },
    ],
  })

  const create = handlers.get('files:create-ui-project')!
  const created = await create({}, { name: 'Dossier Alpha' })
  await test('create-ui-project', () => {
    ok(created.ok, JSON.stringify(created))
    ok(created.project?.id)
    strictEqual(created.project.name, 'Dossier Alpha')
    // NOT 'ui-local'. A project the engine has never heard of is one it will
    // refuse to open, which made the workspace unreachable for everything the
    // UI could create. The engine assigns the identity; the catalog records it.
    strictEqual(created.authority, 'session-actor')
    strictEqual(created.project.id, 'engine-assigned-id')
  })

  await test('creation asks the engine before writing anything', () => {
    strictEqual(engineCalls.length, 1)
    const call = engineCalls[0] as { operationId?: string; approvalTimeoutMs?: number }
    // An idempotency key, so a retried IPC cannot produce a second project.
    ok(typeof call.operationId === 'string' && call.operationId.length > 0)
    // An explicit approval window, so the engine's patience and the prompt's
    // do not merely coincide.
    ok(typeof call.approvalTimeoutMs === 'number' && call.approvalTimeoutMs > 0)
  })


  // ── creation fails closed ──────────────────────────────────────
  // Two ways the engine can decline, and neither may leave a catalog row: a
  // listed project the engine will refuse to open is worse than no project,
  // because the failure surfaces later and looks like corruption.
  for (const [label, callScienceTool] of [
    ['no engine', async () => { throw new Error('ECONNREFUSED') }],
    ['permission denied', async () => { throw new Error('science run 019f finished Denied') }],
  ] as const) {
    const isolatedCatalog = new LocalProjectCatalog()
    const isolatedHandlers = new Map<string, Function>()
    registerScienceIpcHandlers(
      {
        handle(ch: string, h: Function) {
          isolatedHandlers.set(ch, h)
        },
      } as IpcMainLike,
      {
        safeHandle,
        getLumenBinaryHash: () => 'abc',
        previewStore: new AcpPreviewStore(),
        assertMembership: createOfflineCatalogMembershipAsserter({ catalog: isolatedCatalog }),
        projectCatalog: isolatedCatalog,
        defaultOwnerId: 'local-user',
        callScienceTool,
      },
    )
    const attempt = await isolatedHandlers.get('files:create-ui-project')!({}, { name: 'Ghost' })
    await test(`${label}: creation refuses`, () => {
      ok(!attempt.ok, JSON.stringify(attempt))
      // And says why, verbatim — "denied" and "could not reach it" are
      // different facts and a user who cannot tell them apart cannot act.
      ok((attempt.reason ?? '').length > 0)
    })
    await test(`${label}: no catalog row is left behind`, () => {
      strictEqual(isolatedCatalog.list('local-user').length, 0)
    })
  }

  const list = handlers.get('files:list-ui-projects')!
  const listed = await list({})
  await test('list-ui-projects', () => {
    strictEqual(listed.projects.length, 1)
  })

  // Foreign owner cannot open catalog project
  const open = handlers.get('files:open-ui-project')!
  const denied = await open({}, {
    projectId: created.project.id,
    ownerId: 'attacker',
  })
  await test('open denies foreign owner', () => {
    ok(!denied.ok)
    strictEqual(getTrustedPreviewContext(), null)
  })

  const opened = await open({}, {
    projectId: created.project.id,
    ownerId: 'local-user',
  })
  await test('open binds + seeds', () => {
    ok(opened.ok, JSON.stringify(opened))
    strictEqual(opened.seeded, 1)
    const ctx = getTrustedPreviewContext()
    strictEqual(ctx?.projectId, created.project.id)
    strictEqual(ctx?.ownerId, 'local-user')
  })

  const preview = handlers.get('files:preview-by-artifact')!
  const prev = await preview({}, {
    artifactId: 'from-list',
    expectedSha256: LIST_SHA,
  })
  await test('preview after open', () => {
    ok(prev.access.ok, JSON.stringify(prev))
    strictEqual(
      Buffer.from(prev.contentBase64 ?? '', 'base64').toString(),
      '{"from": "list"}\n',
    )
  })

  // OS banned channels still banned
  await test('still bans projects:list', () => {
    strictEqual(validateIpcChannel('projects:list'), false)
  })
  await test('still bans artifacts:read-preview', () => {
    strictEqual(validateIpcChannel('artifacts:read-preview'), false)
  })

  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

run()
