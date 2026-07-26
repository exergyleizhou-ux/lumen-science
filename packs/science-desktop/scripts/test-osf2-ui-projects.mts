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
    projectCatalog: catalog,
    defaultOwnerId: 'local-user',
    listArtifacts: async () => [
      {
        artifact_id: 'from-list',
        path: '/data/from-list.json',
        sha256: 'fl',
      },
    ],
  })

  const create = handlers.get('files:create-ui-project')!
  const created = await create({}, { name: 'Dossier Alpha' })
  await test('create-ui-project', () => {
    ok(created.ok)
    ok(created.project?.id)
    strictEqual(created.project.name, 'Dossier Alpha')
    strictEqual(created.authority, 'ui-local')
  })

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
    expectedSha256: 'fl',
  })
  await test('preview after open', () => {
    ok(prev.access.ok, JSON.stringify(prev))
    strictEqual(prev.path, '/data/from-list.json')
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
