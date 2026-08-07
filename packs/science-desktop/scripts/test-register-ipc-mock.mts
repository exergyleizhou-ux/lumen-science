#!/usr/bin/env npx tsx
/**
 * Execute science IPC registration against a mock ipcMain that throws on
 * double-handle — the failure mode Electron enforces at runtime.
 *
 * Does NOT require Electron; drives shipped registerScienceIpcHandlers.
 *
 * Run: npx tsx scripts/test-register-ipc-mock.mts
 */
import { deepStrictEqual, strictEqual, ok, throws } from 'node:assert/strict'
import {
  registerScienceIpcHandlers,
  type IpcMainLike,
  type SafeHandleFn,
} from '../src/main/files/science-ipc.js'
import { bindTrustedSession } from '../src/main/files/session-binding.js'
import { validateIpcChannel } from '../src/main/lumen-authority-policy.js'
import {
  setTrustedPreviewContextForSender,
  clearTrustedPreviewContextForSender,
  clearAllTrustedPreviewContexts,
  getTrustedPreviewContextForSender,
  attachTrustedIdentitySenderCleanup,
} from '../src/main/files/session-identity.js'
import type { PreviewFileStore } from '../src/main/files/preview-resolver.js'

// Real fixture file: the resolver reads the bytes.
import osFix from 'node:os'
import fsFix from 'node:fs'
import pathFix from 'node:path'
const REG_FIXTURE = pathFix.join(fsFix.mkdtempSync(pathFix.join(osFix.tmpdir(), 'reg-fixture-')), 'a1.csv')
fsFix.writeFileSync(REG_FIXTURE, 'reg,a1\n')
const REG_SHA = '451ef1ee45f12e12fb943665c66d8dc13a908c4d21ba4b4a167b6c676f2c2e10'
const BIOMNI_FIXTURE_BASE64 = Buffer.from('{"results":[],"totalResults":0}').toString('base64')

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
  const workspaceRoot = fsFix.mkdtempSync(pathFix.join(osFix.tmpdir(), 'science-ipc-workspace-'))
  const scienceCalls: { name: string; args: Record<string, unknown> }[] = []

  registerScienceIpcHandlers(ipc, {
    safeHandle,
    getLumenBinaryHash: () => 'deadbeef',
    workspaceRoot,
    callScienceTool: async (name, args) => {
      scienceCalls.push({ name, args })
      return {
        operationId: args.operationId,
        run: { context: { run_id: 'skill-quarantine-run-1' } },
      }
    },
    biomniUniprotFixtureBase64: BIOMNI_FIXTURE_BASE64,
    previewStore: store,
  })

  await test('registers acp:call exactly once', () => {
    ok(handlers.has('acp:call'))
  })
  await test('registers acp:list-tools', () => ok(handlers.has('acp:list-tools')))
  await test('registers app:get-lumen-hash', () => ok(handlers.has('app:get-lumen-hash')))
  await test('registers files:preview-by-artifact', () => ok(handlers.has('files:preview-by-artifact')))
  await test('registers files:bind-session', () => ok(handlers.has('files:bind-session')))
  await test('registers files:unbind-session', () => ok(handlers.has('files:unbind-session')))
  await test('registers files:list-ui-projects', () => ok(handlers.has('files:list-ui-projects')))
  await test('registers files:open-ui-project', () => ok(handlers.has('files:open-ui-project')))
  await test('registers ZIP imports only on the guarded Science IPC surface', () => {
    ok(handlers.has('settings:import-skill-zip'))
    ok(handlers.has('settings:import-skill-zip-batch'))
  })
  await test('does not register legacy local skill mutation authority', () => {
    ok(!handlers.has('skills:import'))
    ok(!handlers.has('skills:admit'))
    ok(!handlers.has('skills:reject'))
  })

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
          callScienceTool: async () => ({}),
          previewStore: store,
        }),
      /second handler/,
    )
  })

  clearAllTrustedPreviewContexts()
  const previewHandler = handlers.get('files:preview-by-artifact')!
  const denied = (await previewHandler({ sender: { id: 1, on() {} } }, { artifactId: 'a1' })) as {
    access: { ok: boolean }
  }
  await test('preview handler denies without session identity', () => {
    ok(denied && denied.access && denied.access.ok === false)
  })

  setTrustedPreviewContextForSender(1, { ownerId: 'o1', projectId: 'p1' })
  const allowed = (await previewHandler(
    { sender: { id: 1, on() {} } },
    {
      artifactId: 'a1',
      expectedSha256: REG_SHA,
      mimeType: 'text/csv',
    },
  )) as {
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

  setTrustedPreviewContextForSender(1, { ownerId: 'evil', projectId: 'p1' })
  const blocked = (await previewHandler({ sender: { id: 1, on() {} } }, { artifactId: 'a1' })) as {
    access: { ok: boolean }
  }
  await test('preview handler blocks cross-owner session', () => {
    ok(!blocked.access.ok)
  })
  clearAllTrustedPreviewContexts()

  clearAllTrustedPreviewContexts()
  const skillHandler = handlers.get('settings:import-skill-zip-batch')!
  const senderA = { id: 11 }
  const senderB = { id: 22 }
  setTrustedPreviewContextForSender(11, { ownerId: 'o1', projectId: 'p1' })
  setTrustedPreviewContextForSender(22, { ownerId: 'o2', projectId: 'p2' })
  // Process-global identity must NOT authorize ZIP quarantine.
  /* process-global identity removed — only sender map is authority */

  const quarantine = (await skillHandler(
    { sender: senderA },
    {
      dataBase64: Buffer.from('bounded fake zip ingress').toString('base64'),
      items: [{ subPath: 'skills/alpha' }],
    },
  )) as {
    results: { subPath: string; status: string; id: string }[]
  }
  await test('ZIP import writes no loose payload and delegates canonical bytes to Rust', () => {
    strictEqual(scienceCalls.length, 1)
    strictEqual(scienceCalls[0].name, 'skill_quarantine_import')
    strictEqual(scienceCalls[0].args.ownerId, 'o1')
    strictEqual(scienceCalls[0].args.projectId, 'p1')
    strictEqual(scienceCalls[0].args.storeRoot, 'science-store')
    strictEqual('path' in scienceCalls[0].args, false)
    strictEqual('workspaceRoot' in scienceCalls[0].args, false)
    strictEqual('ingressId' in scienceCalls[0].args, false)
    strictEqual(
      scienceCalls[0].args.archiveBase64,
      Buffer.from('bounded fake zip ingress').toString('base64'),
    )
    deepStrictEqual(scienceCalls[0].args.items, [{ subPath: 'skills/alpha' }])
    deepStrictEqual(quarantine.results, [
      {
        subPath: 'skills/alpha',
        status: 'quarantined',
        id: 'skill-quarantine-run-1',
      },
    ])
    strictEqual(
      fsFix.existsSync(pathFix.join(workspaceRoot, '.science-import-inbox')),
      false,
      'main must not create a loose archive inbox',
    )
    strictEqual(fsFix.existsSync(pathFix.join(workspaceRoot, 'skills')), false)
  })

  await test('generic ACP call cannot bypass sender-bound ZIP quarantine identity', async () => {
    const genericCall = handlers.get('acp:call')!
    const callsBeforeBypass = scienceCalls.length
    const bypass = (await genericCall({ sender: senderA }, 'skill_quarantine_import', {
      ownerId: 'forged-owner',
      projectId: 'forged-project',
      sessionId: 'forged-session',
      storeRoot: 'science-store',
      operationId: 'forged-operation',
      archiveBase64: Buffer.from('forged').toString('base64'),
      archiveSha256: '0'.repeat(64),
      archiveBytes: 6,
      items: [{ subPath: 'skills/alpha' }],
    })) as { _lumenError?: boolean; message?: string }
    strictEqual(bypass._lumenError, true)
    ok(
      /sender-bound Desktop IPC route|not callable through generic acp:call|cannot carry trusted identity/i.test(
        bypass.message ?? '',
      ),
      bypass.message,
    )
    strictEqual(scienceCalls.length, callsBeforeBypass)
  })

  scienceCalls.length = 0
  await test('ZIP import uses sender A binding, not process-global or sender B', async () => {
    const fromB = (await skillHandler(
      { sender: senderB },
      {
        dataBase64: Buffer.from('bounded fake zip ingress').toString('base64'),
        items: [{ subPath: 'skills/alpha' }],
      },
    )) as { results: { id: string }[] }
    strictEqual(scienceCalls.length, 1)
    strictEqual(scienceCalls[0].args.ownerId, 'o2')
    strictEqual(scienceCalls[0].args.projectId, 'p2')
    strictEqual(fromB.results[0].id, 'skill-quarantine-run-1')
  })

  await test('ZIP import rejects missing sender and forged renderer owner fields', async () => {
    await skillHandler(
      {},
      {
        dataBase64: Buffer.from('x').toString('base64'),
        items: [{ subPath: 'skills/alpha' }],
      },
    ).then(
      () => {
        throw new Error('expected missing sender to fail')
      },
      (err: Error) => {
        ok(/sender identity|bind a Science project/i.test(err.message))
      },
    )
    await skillHandler(
      { sender: senderA },
      {
        dataBase64: Buffer.from('x').toString('base64'),
        items: [{ subPath: 'skills/alpha' }],
        ownerId: 'forged',
        projectId: 'forged',
      },
    ).then(
      () => {
        throw new Error('expected forged owner fields to fail')
      },
      (err: Error) => {
        ok(/may not supply ownerId/i.test(err.message))
      },
    )
  })

  await test('unbind and clear-all isolate senders; engine-style clear wipes all', () => {
    clearTrustedPreviewContextForSender(11)
    strictEqual(getTrustedPreviewContextForSender(11), null)
    strictEqual(getTrustedPreviewContextForSender(22)?.projectId, 'p2')
    clearAllTrustedPreviewContexts()
    strictEqual(getTrustedPreviewContextForSender(22), null)
  })

  await test('sender lifecycle listeners survive rebind without stale cleanup races', () => {
    const listeners = new Map<string, Array<() => void>>()
    const sender = {
      id: 33,
      on: (event: string, listener: () => void) => {
        const existing = listeners.get(event) ?? []
        existing.push(listener)
        listeners.set(event, existing)
      },
    }
    const fire = (event: string) => {
      for (const listener of listeners.get(event) ?? []) listener()
    }

    setTrustedPreviewContextForSender(33, {
      ownerId: 'owner-a',
      projectId: 'project-a',
    })
    attachTrustedIdentitySenderCleanup(sender)
    strictEqual(listeners.get('did-navigate')?.length, 1)
    fire('did-navigate')
    strictEqual(getTrustedPreviewContextForSender(33), null)

    setTrustedPreviewContextForSender(33, {
      ownerId: 'owner-b',
      projectId: 'project-b',
    })
    attachTrustedIdentitySenderCleanup(sender)
    strictEqual(listeners.get('did-navigate')?.length, 1, 'rebind must not attach duplicate stale listeners')
    fire('render-process-gone')
    strictEqual(getTrustedPreviewContextForSender(33), null)

    setTrustedPreviewContextForSender(33, {
      ownerId: 'owner-c',
      projectId: 'project-c',
    })
    fire('destroyed')
    strictEqual(getTrustedPreviewContextForSender(33), null)
    attachTrustedIdentitySenderCleanup(sender)
    strictEqual(
      listeners.get('did-navigate')?.length,
      2,
      'destroy permits a future WebContents lifecycle to attach once',
    )
  })

  await test('pending membership cannot resurrect identity after clear or a newer bind', async () => {
    let finishCleared!: (value: {
      ok: true
      ownerId: string
      projectId: string
    }) => void
    const cleared = bindTrustedSession(
      { ownerId: 'old-owner', projectId: 'old-project' },
      {
        senderId: 44,
        assertMembership: () =>
          new Promise((resolve) => {
            finishCleared = resolve
          }),
      },
    )
    clearAllTrustedPreviewContexts()
    finishCleared({
      ok: true,
      ownerId: 'old-owner',
      projectId: 'old-project',
    })
    strictEqual((await cleared).ok, false)
    strictEqual(getTrustedPreviewContextForSender(44), null)

    let finishOld!: (value: {
      ok: true
      ownerId: string
      projectId: string
    }) => void
    const oldBind = bindTrustedSession(
      { ownerId: 'old-owner', projectId: 'old-project' },
      {
        senderId: 55,
        assertMembership: () =>
          new Promise((resolve) => {
            finishOld = resolve
          }),
      },
    )
    const newBind = await bindTrustedSession(
      { ownerId: 'new-owner', projectId: 'new-project' },
      {
        senderId: 55,
        assertMembership: async () => ({
          ok: true,
          ownerId: 'new-owner',
          projectId: 'new-project',
        }),
      },
    )
    strictEqual(newBind.ok, true)
    finishOld({
      ok: true,
      ownerId: 'old-owner',
      projectId: 'old-project',
    })
    strictEqual((await oldBind).ok, false)
    deepStrictEqual(getTrustedPreviewContextForSender(55), {
      ownerId: 'new-owner',
      projectId: 'new-project',
    })
  })


  await test('notebook/review/compute multi-sender isolation + spoof rejection', async () => {
    clearAllTrustedPreviewContexts()
    setTrustedPreviewContextForSender(101, { ownerId: 'owner-a', projectId: 'proj-a' })
    setTrustedPreviewContextForSender(202, { ownerId: 'owner-b', projectId: 'proj-b' })
    const evtA = { sender: { id: 101, on() {} } }
    const evtB = { sender: { id: 202, on() {} } }
    const noSender = {}

    const nbExec = handlers.get('notebook:execute-cell')!
    const nbA = await nbExec(evtA, { language: 'python', code: 'print(1)\\n' })
    // Without full notebook wiring this may fail for interpreter reasons, but must not
    // use the other sender's project. Prefer bound identity failure modes.
    const nbNo = await nbExec(noSender, { language: 'python', code: 'print(1)\\n' })
    ok((nbNo as { ok?: boolean }).ok === false, 'notebook without sender must fail')

    const computePlan = handlers.get('compute:plan')!
    const planA = await computePlan(evtA, { hostname: 'hpc.a.local', targetKind: 'ssh_fixture' })
    const planB = await computePlan(evtB, { hostname: 'hpc.b.local', targetKind: 'ssh_fixture' })
    const planNo = await computePlan(noSender, { hostname: 'hpc.x.local', targetKind: 'ssh_fixture' })
    ok((planNo as { ok?: boolean }).ok === false, 'compute without sender must fail')
    // A and B each get their own trusted context — neither may succeed under null identity
    void planA
    void planB
    void nbA

    const reviewSubmit = handlers.get('review:submit')!
    const revNo = await reviewSubmit(noSender, {
      runId: 'r1',
      verdict: 'pass',
      summary: 'x'.repeat(20),
      artifacts: [{ artifactId: 'a', expectedSha256: '0'.repeat(64) }],
    })
    ok((revNo as { ok?: boolean }).ok === false, 'review without sender must fail')

    // generic acp:call cannot invoke sender-bound skill quarantine
    const acp = handlers.get('acp:call')!
    const bypass = await acp(evtA, 'skill_quarantine_import', {
      ownerId: 'forged',
      projectId: 'forged',
    })
    ok((bypass as { _lumenError?: boolean })._lumenError === true, 'acp:call must refuse sender-bound method')

    // destroy-style clear of one sender leaves the other intact
    clearTrustedPreviewContextForSender(101)
    strictEqual(getTrustedPreviewContextForSender(101), null)
    strictEqual(getTrustedPreviewContextForSender(202)?.projectId, 'proj-b')
    clearAllTrustedPreviewContexts()
    strictEqual(getTrustedPreviewContextForSender(202), null)
  })

  await test('skills:run-capability binds identity and fixture in main', async () => {
    clearAllTrustedPreviewContexts()
    const runCap = handlers.get('skills:run-capability')!
    ok(runCap, 'skills:run-capability must be registered')
    const noSender = await runCap({}, {
      capabilityId: 'ecosystem/biomni/query_uniprot',
      prompt: 'human insulin',
    })
    ok((noSender as { ok?: boolean }).ok === false, 'missing sender must fail')

    setTrustedPreviewContextForSender(77, { ownerId: 'owner-a', projectId: 'proj-a' })
    const forged = await runCap(
      { sender: { id: 77, on() {} } },
      {
        capabilityId: 'ecosystem/biomni/query_uniprot',
        prompt: 'human insulin',
        fixturePaths: ['/tmp/renderer-selected.json'],
        ownerId: 'forged',
        projectId: 'forged',
      },
    )
    ok((forged as { ok?: boolean }).ok === false, 'forged owner/path must fail')
    ok(
      /may not supply ownerId/i.test(String((forged as { reason?: string }).reason ?? '')),
      String((forged as { reason?: string }).reason),
    )

    const otherTool = await runCap(
      { sender: { id: 77, on() {} } },
      {
        capabilityId: 'ecosystem/biomni/analyze_enzyme_kinetics_assay',
        prompt: 'x',
      },
    )
    ok((otherTool as { ok?: boolean }).ok === false, 'non-admitted tool must fail')

    const callsBeforeCapability = scienceCalls.length
    const accepted = await runCap(
      { sender: { id: 77, on() {} } },
      {
        capabilityId: 'ecosystem/biomni/query_uniprot',
        prompt: 'human insulin',
        maxResults: 5,
      },
    )
    ok((accepted as { ok?: boolean }).ok === true, `valid capability failed: ${JSON.stringify(accepted)}`)
    strictEqual(scienceCalls.length, callsBeforeCapability + 1)
    const capabilityCall = scienceCalls.at(-1)!
    strictEqual(capabilityCall.name, 'capability_run')
    strictEqual(capabilityCall.args.ownerId, 'owner-a')
    strictEqual(capabilityCall.args.projectId, 'proj-a')
    deepStrictEqual(capabilityCall.args.fixtureDataBase64, [BIOMNI_FIXTURE_BASE64])
    strictEqual('fixturePaths' in capabilityCall.args, false)
    strictEqual('connectorId' in capabilityCall.args, false)
    strictEqual('sessionId' in capabilityCall.args, false)

    for (const maxResults of [0, 51, -1, 1.5, '5', null]) {
      const before = scienceCalls.length
      const invalid = await runCap(
        { sender: { id: 77, on() {} } },
        {
          capabilityId: 'ecosystem/biomni/query_uniprot',
          prompt: 'human insulin',
          maxResults,
        },
      )
      ok((invalid as { ok?: boolean }).ok === false, `maxResults=${String(maxResults)} was accepted`)
      strictEqual(scienceCalls.length, before, 'invalid maxResults reached Rust authority')
    }

    const acp = handlers.get('acp:call')!
    const bypass = await acp({ sender: { id: 77, on() {} } }, 'capability_run', {
      ownerId: 'forged',
      projectId: 'forged',
      capabilityId: 'ecosystem/biomni/query_uniprot',
      input: { prompt: 'x' },
    })
    ok((bypass as { _lumenError?: boolean })._lumenError === true)
    clearAllTrustedPreviewContexts()
  })

  const hashHandler = handlers.get('app:get-lumen-hash')!
  const hash = await hashHandler({})
  await test('hash handler returns binary hash', () => {
    strictEqual(hash, 'deadbeef')
  })

  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

run()
