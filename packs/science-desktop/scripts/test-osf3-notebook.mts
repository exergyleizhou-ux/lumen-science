#!/usr/bin/env npx tsx
/**
 * OSF-3 Notebook plan / dry-run / execute-via-ACP tests.
 * Run: npx tsx scripts/test-osf3-notebook.mts
 */
import { strictEqual, ok } from 'node:assert/strict'
import fs from 'node:fs'
import {
  planNotebookCell,
  assertNotebookExecuteAccess,
  exportHistoryToIpynb,
  hashNotebookCode,
} from '../src/main/files/notebook-plan.js'
import { createNotebookService } from '../src/main/files/notebook-service.js'
import type { TrustedPreviewContext } from '../src/main/files/session-identity.js'
import {
  registerScienceIpcHandlers,
  type IpcMainLike,
  type SafeHandleFn,
} from '../src/main/files/science-ipc.js'
import { validateIpcChannel } from '../src/main/lumen-authority-policy.js'
import { LocalProjectCatalog } from '../src/main/files/local-project-catalog.js'
import { createOfflineCatalogMembershipAsserter } from '../src/main/files/hybrid-membership.js'
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

async function run() {
  // ── Pure plan ────────────────────────────────────────────────
  const bad = planNotebookCell({ language: 'python', code: '' })
  await test('plan rejects empty code', () => {
    ok('ok' in bad && bad.ok === false)
  })

  const banned = planNotebookCell({
    language: 'python',
    code: 'import os\nos.system("rm -rf /")\n',
  })
  await test('plan rejects os.system', () => {
    ok('ok' in banned && banned.ok === false)
  })

  const good = planNotebookCell({
    language: 'python',
    code: 'print(1+1)\n',
    dryRun: true,
  })
  await test('plan accepts simple python dry-run', () => {
    ok(!('ok' in good))
    const p = good as { dryRun: boolean; tool: string; codeHash: string }
    ok(p.dryRun)
    strictEqual(p.tool, 'workflow_execute')
    strictEqual(p.codeHash, hashNotebookCode('print(1+1)\n'))
  })

    const livePlan = planNotebookCell({ language: 'python', code: 'x=1\n' }) as {
    dryRun: boolean
    tool: string
    authority: string
  }
  await test('execute access denied without session', () => {
    const a = assertNotebookExecuteAccess(
      { ...livePlan, dryRun: false, planId: 'p', cellId: 'c', language: 'python', codeHash: 'h', codeLength: 1, requiresAdmittedKernel: true, warnings: [], createdAt: 0 },
      null,
    )
    ok(!a.ok)
  })

    await test('execute access ok with session', () => {
    const a = assertNotebookExecuteAccess(
      {
        planId: 'p',
        cellId: 'c',
        language: 'python',
        codeHash: 'h',
        codeLength: 1,
        dryRun: false,
        tool: 'workflow_execute',
        authority: 'SessionActor/KernelAdapter',
        requiresAdmittedKernel: true,
        warnings: [],
        createdAt: 0,
      },
      { ownerId: 'o1', projectId: 'p1' },
    )
    ok(a.ok)
  })

  // ── Service dry-run + mock ACP execute ───────────────────────
  // The mock asserts the REAL contract: a one-cell workflowSpec sent to
  // workflow_execute. The previous mock accepted 'notebook_execute', a method
  // the registry rejects — so this suite stayed green while the button it
  // covers could not work against any engine.
  let acpCalls = 0
  const seenArgs: Record<string, unknown>[] = []
  const svc = createNotebookService({
    acpCall: async (tool, args) => {
      acpCalls++
      strictEqual(tool, 'workflow_execute')
      seenArgs.push(args)
      return { state: 'succeeded', operationId: args.operationId }
    },
    resolveInterpreter: async () => ({ ok: true, interpreterPath: '/usr/bin/python3' }),
    defaultOwnerId: 'o1',
    storeRoot: 'science-store',
  })

  const dry = svc.dryRun({ language: 'python', code: 'print(2)\n' })
  await test('service dry-run does not call ACP', () => {
    ok(dry.ok)
    if (dry.ok) strictEqual(dry.wouldCall.tool, 'workflow_execute')
    strictEqual(acpCalls, 0)
  })

    const noSess = await svc.execute({ language: 'python', code: 'print(1)\n' }, null)
  await test('service execute fails without session', () => {
    ok(noSess && (noSess as { ok?: boolean }).ok === false)
    strictEqual(acpCalls, 0)
  })

    const exec = await svc.execute({ language: 'python', code: 'print(2)\n' }, { ownerId: 'o1', projectId: 'p1' })
  await test('service execute via ACP', () => {
    ok((exec as { ok?: boolean }).ok, JSON.stringify(exec))
    strictEqual(acpCalls, 1)
    strictEqual(svc.history().length, 1)
  })

  await test('the execute request is one the engine can honour', () => {
    const args = seenArgs[0] as {
      operationId?: string
      interpreterPath?: string
      allowKernelSteps?: boolean
      ownerId?: string
      workflowSpec?: {
        project_id?: string
        schema_version?: number
        steps?: { kind?: string; notebook_cell?: string }[]
      }
    }
    // Idempotency: a retried IPC must not run the cell twice.
    ok(typeof args.operationId === 'string' && args.operationId.length > 0)
    // The engine refuses a relative path — which binary ran is evidence.
    ok(args.interpreterPath?.startsWith('/'))
    // Explicit opt-in: kernel steps are refused by default policy.
    strictEqual(args.allowKernelSteps, true)
    // Bound to the trusted session's project, not a default.
    strictEqual(args.workflowSpec?.project_id, 'p1')
    strictEqual(args.ownerId, 'o1')
    const step = args.workflowSpec?.steps?.[0]
    strictEqual(step?.kind, 'NotebookCell')
    // The step carries the SOURCE — the executor hashes this as the cell.
    strictEqual(step?.notebook_cell, 'print(2)\n')
  })

  await test('a run that did not succeed is not recorded as success', async () => {
    const failing = createNotebookService({
      acpCall: async () => ({ state: 'denied' }),
      resolveInterpreter: async () => ({ ok: true, interpreterPath: '/usr/bin/python3' }),
    })
    await failing.execute({ language: 'python', code: 'print(3)\n' }, { ownerId: 'o1', projectId: 'p1' })
    strictEqual(failing.history()[0]?.ok, false)
  })

  await test('no interpreter means refusal, and the engine is never called', async () => {
    let called = 0
    const bare = createNotebookService({
      acpCall: async () => {
        called++
        return { state: 'succeeded' }
      },
      resolveInterpreter: async () => ({ ok: false, reason: 'no runnable Python' }),
    })
    const out = (await bare.execute({ language: 'python', code: 'print(4)\n' }, { ownerId: 'o1', projectId: 'p1' })) as {
      ok?: boolean
      reason?: string
    }
    strictEqual(out.ok, false)
    ok(out.reason?.includes('no runnable Python'))
    strictEqual(called, 0)
  })

  const ipynb = svc.exportIpynb({ ownerId: 'o1', projectId: 'p1' })
  await test('export ipynb projection', () => {
    ok(!('ok' in ipynb && (ipynb as { ok: boolean }).ok === false))
    const nb = ipynb as { nbformat: number; cells: unknown[] }
    strictEqual(nb.nbformat, 4)
    ok(nb.cells.length >= 1)
  })

  // ── Stubs remain stubs ───────────────────────────────────────
  const ke = fs.readFileSync('src/main/notebook/kernel-executor.ts', 'utf-8')
  await test('kernel-executor still STUB', () => {
    ok(ke.includes('EXECUTION AUTHORITY REMOVED'))
    ok(ke.includes('Promise.reject'))
  })
  const ipcStub = fs.readFileSync('src/main/notebook/ipc.ts', 'utf-8')
  await test('notebook/ipc still STUB register', () => {
    ok(ipcStub.includes('EXECUTION AUTHORITY REMOVED'))
  })

  // ── IPC registration ─────────────────────────────────────────
  for (const ch of [
    'notebook:plan-cell',
    'notebook:dry-run-cell',
    'notebook:execute-cell',
    'notebook:history',
    'notebook:export-ipynb',
  ]) {
    await test(`policy allows ${ch}`, () => ok(validateIpcChannel(ch)))
  }

  const handlers = new Map<string, Function>()
  const ipc: IpcMainLike = {
    handle(ch, h) {
      if (handlers.has(ch)) throw new Error(`dup ${ch}`)
      handlers.set(ch, h)
    },
  }
  const catalog = new LocalProjectCatalog()
    registerScienceIpcHandlers(ipc, {
    safeHandle,
    getLumenBinaryHash: () => 'h',
    previewStore: new AcpPreviewStore(),
    assertMembership: createOfflineCatalogMembershipAsserter({ catalog }),
    projectCatalog: catalog,
    // Creation is an engine mutation now; this suite is about the notebook, so
    // it stands in a permissive engine rather than exercising that path.
    // test-osf2-ui-projects.mts covers what happens when the engine declines.
    callScienceTool: async (tool: string) => {
      if (tool !== 'project_create') throw new Error(`unexpected tool ${tool}`)
      return { projectId: 'nb-engine-project' }
    },
    notebookService: svc,
  })

  await test('ipc registers notebook channels', () => {
    ok(handlers.has('notebook:plan-cell'))
    ok(handlers.has('notebook:dry-run-cell'))
    ok(handlers.has('notebook:execute-cell'))
    ok(handlers.has('notebook:export-ipynb'))
  })

  const dryH = handlers.get('notebook:dry-run-cell')!
  const dryRes = await dryH({}, { language: 'python', code: 'print(3)\n' })
  await test('ipc dry-run works', () => {
    ok(dryRes.ok)
  })

  // create+open project then execute (sender-bound identity)
  const senderEvt = { sender: { id: 1, on() {} } }
  const create = handlers.get('files:create-ui-project')!
  const created = await create(senderEvt, { name: 'nb-proj' })
  const open = handlers.get('files:open-ui-project')!
  await open(senderEvt, { projectId: created.project.id, ownerId: 'local-user' })
  const exH = handlers.get('notebook:execute-cell')!
  const exRes = await exH(senderEvt, { language: 'python', code: 'print(9)\n' })
  await test('ipc execute after project open', () => {
    ok(exRes.ok, JSON.stringify(exRes))
  })

  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

run()
