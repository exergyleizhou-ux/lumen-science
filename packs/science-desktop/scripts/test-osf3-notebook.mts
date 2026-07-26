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
import {
  setTrustedPreviewContext,
  clearTrustedPreviewContext,
} from '../src/main/files/session-identity.js'
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
    strictEqual(p.tool, 'notebook_execute')
    strictEqual(p.codeHash, hashNotebookCode('print(1+1)\n'))
  })

  clearTrustedPreviewContext()
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

  setTrustedPreviewContext({ ownerId: 'o1', projectId: 'p1' })
  await test('execute access ok with session', () => {
    const a = assertNotebookExecuteAccess(
      {
        planId: 'p',
        cellId: 'c',
        language: 'python',
        codeHash: 'h',
        codeLength: 1,
        dryRun: false,
        tool: 'notebook_execute',
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
  let acpCalls = 0
  const svc = createNotebookService({
    acpCall: async (tool, args) => {
      acpCalls++
      strictEqual(tool, 'notebook_execute')
      return { OK: true, Stdout: '2\n', code: args.code }
    },
  })

  const dry = svc.dryRun({ language: 'python', code: 'print(2)\n' })
  await test('service dry-run does not call ACP', () => {
    ok(dry.ok)
    if (dry.ok) strictEqual(dry.wouldCall.tool, 'notebook_execute')
    strictEqual(acpCalls, 0)
  })

  clearTrustedPreviewContext()
  const noSess = await svc.execute({ language: 'python', code: 'print(1)\n' })
  await test('service execute fails without session', () => {
    ok(noSess && (noSess as { ok?: boolean }).ok === false)
    strictEqual(acpCalls, 0)
  })

  setTrustedPreviewContext({ ownerId: 'o1', projectId: 'p1' })
  const exec = await svc.execute({ language: 'python', code: 'print(2)\n' })
  await test('service execute via ACP', () => {
    ok((exec as { ok?: boolean }).ok)
    strictEqual(acpCalls, 1)
    strictEqual(svc.history().length, 1)
  })

  const ipynb = svc.exportIpynb()
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
  clearTrustedPreviewContext()
  registerScienceIpcHandlers(ipc, {
    safeHandle,
    getLumenBinaryHash: () => 'h',
    previewStore: new AcpPreviewStore(),
    assertMembership: createOfflineCatalogMembershipAsserter({ catalog }),
    projectCatalog: catalog,
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

  // create+open project then execute
  const create = handlers.get('files:create-ui-project')!
  const created = await create({}, { name: 'nb-proj' })
  const open = handlers.get('files:open-ui-project')!
  await open({}, { projectId: created.project.id, ownerId: 'local-user' })
  // re-bind trust for our svc which uses global identity — open set it for local-user
  // defaultOwner is local-user but catalog owner is local-user; hybrid ok
  setTrustedPreviewContext({
    ownerId: created.project.ownerId,
    projectId: created.project.id,
  })
  const exH = handlers.get('notebook:execute-cell')!
  const exRes = await exH({}, { language: 'python', code: 'print(9)\n' })
  await test('ipc execute after project open', () => {
    ok(exRes.ok, JSON.stringify(exRes))
  })

  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

run()
