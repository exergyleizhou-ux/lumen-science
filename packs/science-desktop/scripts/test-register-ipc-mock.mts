#!/usr/bin/env npx tsx
/**
 * Execute science IPC registration against a mock ipcMain that throws on
 * double-handle — the failure mode Electron enforces at runtime.
 *
 * Does NOT require Electron; drives shipped registerScienceIpcHandlers.
 *
 * Run: npx tsx scripts/test-register-ipc-mock.mts
 */
import { strictEqual, ok, throws } from 'node:assert/strict'
import {
  registerScienceIpcHandlers,
  type IpcMainLike,
  type SafeHandleFn,
} from '../src/main/files/science-ipc.js'
import { validateIpcChannel } from '../src/main/lumen-authority-policy.js'
import {
  setTrustedPreviewContext,
  clearTrustedPreviewContext,
} from '../src/main/files/session-identity.js'
import type { PreviewFileStore } from '../src/main/files/preview-resolver.js'

// Real fixture file: the resolver reads the bytes.
import osFix from 'node:os'
import fsFix from 'node:fs'
import pathFix from 'node:path'
const REG_FIXTURE = pathFix.join(fsFix.mkdtempSync(pathFix.join(osFix.tmpdir(), 'reg-fixture-')), 'a1.csv')
fsFix.writeFileSync(REG_FIXTURE, 'reg,a1\n')
const REG_SHA = '451ef1ee45f12e12fb943665c66d8dc13a908c4d21ba4b4a167b6c676f2c2e10'


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

/** Mirrors lumen-acp-bridge.safeHandle without importing electron. */
const safeHandle: SafeHandleFn = (ipcMain, channel, handler) => {
  if (!validateIpcChannel(channel)) {
    ipcMain.handle(channel, async () => ({
      _lumenBanned: true,
      channel,
      reason: 'EXECUTION AUTHORITY REMOVED — use Lumen bridge (acp:call)',
    }))
    return
  }
  ipcMain.handle(channel, handler)
}

function createMockIpcMain() {
  const handlers = new Map<string, Function>()
  const ipc: IpcMainLike = {
    handle(channel: string, handler: Function) {
      if (handlers.has(channel)) {
        throw new Error(`Attempted to register a second handler for '${channel}'`)
      }
      handlers.set(channel, handler)
    },
  }
  return { ipc, handlers }
}

const store: PreviewFileStore = {
  async resolveById(artifactId: string) {
    if (artifactId !== 'a1') return null
    return {
      path: REG_FIXTURE,
      sha256: REG_SHA,
      ownerId: 'o1',
      projectId: 'p1',
    }
  },
}

async function run() {
  const { ipc, handlers } = createMockIpcMain()

  registerScienceIpcHandlers(ipc, {
    safeHandle,
    getLumenBinaryHash: () => 'deadbeef',
    acpFetch: async () => ({ ok: true }),
    previewStore: store,
  })

  await test('registers acp:call exactly once', () => {
    ok(handlers.has('acp:call'))
  })
  await test('registers acp:list-tools', () => ok(handlers.has('acp:list-tools')))
  await test('registers app:get-lumen-hash', () => ok(handlers.has('app:get-lumen-hash')))
  await test('registers files:preview-by-artifact', () =>
    ok(handlers.has('files:preview-by-artifact')),
  )
  await test('registers files:bind-session', () => ok(handlers.has('files:bind-session')))
  await test('registers files:unbind-session', () => ok(handlers.has('files:unbind-session')))
  await test('registers files:list-ui-projects', () => ok(handlers.has('files:list-ui-projects')))
  await test('registers files:open-ui-project', () => ok(handlers.has('files:open-ui-project')))

  await test('all registered channels pass validateIpcChannel', () => {
    for (const ch of handlers.keys()) {
      ok(validateIpcChannel(ch), `channel ${ch} must be allowed`)
    }
  })

  await test('double-register throws (Electron contract)', () => {
    throws(
      () =>
        registerScienceIpcHandlers(ipc, {
          safeHandle,
          getLumenBinaryHash: () => 'x',
          acpFetch: async () => ({}),
          previewStore: store,
        }),
      /second handler/,
    )
  })

  clearTrustedPreviewContext()
  const previewHandler = handlers.get('files:preview-by-artifact')!
  const denied = (await previewHandler({}, { artifactId: 'a1' })) as {
    access: { ok: boolean }
  }
  await test('preview handler denies without session identity', () => {
    ok(denied && denied.access && denied.access.ok === false)
  })

  setTrustedPreviewContext({ ownerId: 'o1', projectId: 'p1' })
  const allowed = (await previewHandler({}, {
    artifactId: 'a1',
    expectedSha256: REG_SHA,
    mimeType: 'text/csv',
  })) as {
    access: { ok: boolean }
    contentBase64?: string
    byteLength?: number
    sha256?: string
    mimeType?: string
    path?: unknown
  }
  await test('preview handler allows matching trusted session', () => {
    ok(allowed.access.ok, `expected ok, got ${JSON.stringify(allowed)}`)
    strictEqual(allowed.path, undefined, 'verified preview must not return a reopenable path')
    strictEqual(
      Buffer.from(allowed.contentBase64 ?? '', 'base64').toString('utf8'),
      'reg,a1\n',
      'handler must return the exact bytes read and hashed from its open file handle',
    )
    strictEqual(allowed.byteLength, Buffer.byteLength('reg,a1\n'))
    strictEqual(allowed.sha256, REG_SHA)
    strictEqual(allowed.mimeType, 'text/csv')
  })

  setTrustedPreviewContext({ ownerId: 'evil', projectId: 'p1' })
  const blocked = (await previewHandler({}, { artifactId: 'a1' })) as {
    access: { ok: boolean }
  }
  await test('preview handler blocks cross-owner session', () => {
    ok(!blocked.access.ok)
  })
  clearTrustedPreviewContext()

  const hashHandler = handlers.get('app:get-lumen-hash')!
  const hash = await hashHandler({})
  await test('hash handler returns binary hash', () => {
    strictEqual(hash, 'deadbeef')
  })

  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

run()
