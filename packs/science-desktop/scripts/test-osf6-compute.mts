#!/usr/bin/env npx tsx
/**
 * OSF-6 Remote Compute plan/dry-run tests — shipped modules only.
 * Run: npx tsx scripts/test-osf6-compute.mts
 */
import { strictEqual, ok } from 'node:assert/strict'
import fs from 'node:fs'
import {
  planRemoteCompute,
  assertComputePlanAccess,
  rejectDesktopLiveExecute,
} from '../src/main/files/compute-plan.js'
import { createComputeService } from '../src/main/files/compute-service.js'
import type { TrustedPreviewContext } from '../src/main/files/session-identity.js'
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

async function run() {
  const noHost = planRemoteCompute({ hostname: '' })
  await test('plan rejects empty host', () => ok('ok' in noHost && noHost.ok === false))

  const shell = planRemoteCompute({
    hostname: 'hpc.example.com',
    command: 'bash -c "rm -rf /"',
  })
  await test('plan rejects generic shell', () => ok('ok' in shell && shell.ok === false))

  const unauth = planRemoteCompute({
    hostname: 'hpc.example.com',
    targetKind: 'ssh_authorized',
    operatorAuthorized: false,
    requestLive: true,
  })
  await test('ssh_authorized without operator denied', () =>
    ok('ok' in unauth && unauth.ok === false),
  )

  const fixture = planRemoteCompute({
    hostname: 'hpc.example.com',
    targetKind: 'ssh_fixture',
    command: 'lumen-science pipeline offline ...',
  })
  await test('ssh_fixture dry-run plan', () => {
    ok(!('ok' in fixture))
    const p = fixture as { canSchedule: boolean; dryRun: boolean; authority: string }
    ok(p.dryRun)
    ok(!p.canSchedule)
    strictEqual(p.authority, 'SessionActor/ToolAdapter')
  })

  const local = planRemoteCompute({
    hostname: 'localhost',
    targetKind: 'local_process',
  })
  await test('local_process plan canSchedule false', () => {
    ok(!('ok' in local))
    ok(!(local as { canSchedule: boolean }).canSchedule)
  })

    const plan = fixture as {
    planId: string
    planHash: string
    clusterId: string
    hostname: string
    scheduler: string
    targetKind: 'ssh_fixture'
    jobs: unknown[]
    canSchedule: boolean
    dryRun: true
    authority: 'SessionActor/ToolAdapter'
    tool: 'compute_plan'
    notes: string[]
    createdAt: number
  }
  await test('access denied without session', () => {
    ok(!assertComputePlanAccess(plan, null).ok)
  })
    await test('access ok with session', () => {
    ok(assertComputePlanAccess(plan, { ownerId: 'o1', projectId: 'p1' }).ok)
  })

  await test('desktop live execute always denied', () => {
    const r = rejectDesktopLiveExecute()
    ok(!r.ok)
    ok((r.reason ?? '').includes('denied'))
  })

  // Service
  let acpCalls = 0
  const svc = createComputeService({
    acpCall: async (tool, args) => {
      acpCalls++
      strictEqual(tool, 'compute_plan')
      ok(args.plan_hash)
      ok(args.dry_run === true)
      return { status: 'registered', can_schedule: false }
    },
  })

    const noSess = svc.plan({ hostname: 'hpc.example.com', targetKind: 'ssh_fixture' }, null)
  await test('service plan needs session', () => ok((noSess as { ok?: boolean }).ok === false))

    const planned = svc.plan({
    hostname: 'hpc.example.com',
    targetKind: 'ssh_fixture',
    command: 'lumen-science pipeline offline ...',
  }, { ownerId: "o1", projectId: "p1" })
  await test('service plan ok', () => {
    ok((planned as { ok?: boolean }).ok)
    ok((planned as { plan?: { canSchedule: boolean } }).plan?.canSchedule === false)
  })

  const submitted = await svc.submitPlan({
    hostname: 'hpc.example.com',
    targetKind: 'ssh_fixture',
  }, { ownerId: "o1", projectId: "p1" })
  await test('service submitPlan calls ACP dry-run', () => {
    ok((submitted as { ok?: boolean }).ok)
    strictEqual(acpCalls, 1)
  })

  await test('service executeLive denied', () => {
    const r = svc.executeLive('any')
    ok((r as { ok?: boolean }).ok === false)
  })

  // Stubs remain
  const ssh = fs.readFileSync('src/main/compute/ssh-runner.ts', 'utf-8')
  await test('ssh-runner still STUB', () => {
    ok(ssh.includes('EXECUTION AUTHORITY REMOVED'))
  })
  const cipc = fs.readFileSync('src/main/compute/ipc.ts', 'utf-8')
  await test('compute/ipc still STUB', () => {
    ok(cipc.includes('EXECUTION AUTHORITY REMOVED'))
  })

  // Greenfield must not import runners
  const sci = fs.readFileSync('src/main/files/science-ipc.ts', 'utf-8')
  await test('science-ipc no SystemSshRunner', () => {
    ok(!sci.includes('SystemSshRunner'))
    ok(!sci.includes('SystemScpRunner'))
    ok(!sci.includes('JobDispatcher'))
  })

  for (const ch of [
    'compute:plan',
    'compute:submit-plan',
    'compute:execute-live',
    'compute:history',
  ]) {
    await test(`policy allows ${ch}`, () => ok(validateIpcChannel(ch)))
  }
  await test('still bans compute:job-updated', () =>
    strictEqual(validateIpcChannel('compute:job-updated'), false),
  )

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
    computeService: svc,
  })
  await test('ipc registers compute channels', () => {
    ok(handlers.has('compute:plan'))
    ok(handlers.has('compute:execute-live'))
  })

  const liveH = handlers.get('compute:execute-live')!
  const liveRes = await liveH({}, { planId: 'x' })
  await test('ipc live execute denied', () => ok(liveRes.ok === false))

  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

run()
