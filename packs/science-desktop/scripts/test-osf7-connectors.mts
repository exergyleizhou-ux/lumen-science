#!/usr/bin/env npx tsx
/**
 * OSF-7 Connector catalog tests — shipped loadConnectorCatalog.
 * Run: npx tsx scripts/test-osf7-connectors.mts
 */
import { strictEqual, ok } from 'node:assert/strict'
import path from 'node:path'
import {
  loadConnectorCatalog,
  rejectDesktopConnectorFetch,
} from '../src/main/files/connector-catalog.js'
import {
  registerScienceIpcHandlers,
  type IpcMainLike,
  type SafeHandleFn,
} from '../src/main/files/science-ipc.js'
import { validateIpcChannel } from '../src/main/lumen-authority-policy.js'
import { AcpPreviewStore } from '../src/main/files/acp-preview-store.js'

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

const LOCK = path.resolve(process.cwd(), '../../docs/science/fusion-sources.lock.json')

async function run() {
  const cat = loadConnectorCatalog(LOCK)
  await test('catalog total 42', () => strictEqual(cat.summary.total, 42))
  await test('catalog implemented 40', () => strictEqual(cat.summary.implemented, 40))
  await test('catalog rejected 2', () => strictEqual(cat.summary.rejected, 2))

  await test('pubmed callable', () => {
    const p = cat.connectors.find((c) => c.connectorId === 'pubmed')
    ok(p)
    ok(p!.callable)
  })
  await test('biogrid not callable', () => {
    const b = cat.connectors.find((c) => c.connectorId === 'biogrid')
    ok(b)
    ok(!b!.callable)
    ok(b!.disposition.startsWith('rejected'))
  })
  await test('kegg not callable', () => {
    const k = cat.connectors.find((c) => c.connectorId === 'kegg')
    ok(k)
    ok(!k!.callable)
  })

  const deny = rejectDesktopConnectorFetch('pubmed')
  await test('desktop fetch always denied', () => {
    ok(!deny.ok)
    ok(deny.reason.includes('SessionActor'))
  })

  for (const ch of ['connectors:list', 'connectors:fetch']) {
    await test(`policy allows ${ch}`, () => ok(validateIpcChannel(ch)))
  }

  const handlers = new Map<string, Function>()
  const ipc: IpcMainLike = {
    handle(ch, h) {
      if (handlers.has(ch)) throw new Error(`dup ${ch}`)
      handlers.set(ch, h)
    },
  }
  registerScienceIpcHandlers(ipc, {
    safeHandle,
    getLumenBinaryHash: () => 'h',
    previewStore: new AcpPreviewStore(),
    connectorLockPath: LOCK,
  })
  await test('ipc registers connector channels', () => {
    ok(handlers.has('connectors:list'))
    ok(handlers.has('connectors:fetch'))
  })

  const list = await handlers.get('connectors:list')!({})
  await test('ipc list ok 42', () => {
    ok(list.ok)
    strictEqual(list.summary.total, 42)
  })
  const fetch = await handlers.get('connectors:fetch')!({}, { connectorId: 'pubmed' })
  await test('ipc fetch denied', () => ok(fetch.ok === false))

  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

run()
