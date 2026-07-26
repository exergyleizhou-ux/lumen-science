#!/usr/bin/env npx tsx
/**
 * Permission IPC — can a renderer manufacture an approval?
 *
 * The renderer answers permission asks, so it sits on the path to an allow.
 * These tests exist to show it cannot get one on its own: the main process
 * issues the request id, and a reply naming any other id must not resolve a
 * pending ask.
 *
 *   npx tsx scripts/test-permission-ipc.mts
 */
import { registerPermissionIpc } from '../src/main/permission-ipc.js'
import { validateIpcChannel } from '../src/main/lumen-authority-policy.js'

let passed = 0
const failures: string[] = []
const check = (label: string, ok: boolean, detail = ''): void => {
  if (ok) { passed += 1; console.log(`  ok    ${label}`) }
  else { failures.push(label); console.log(`  FAIL  ${label}${detail ? ` — ${detail}` : ''}`) }
}

type Handler = (event: unknown, ...args: unknown[]) => Promise<unknown>

function harness(windowPresent = true) {
  const handlers = new Map<string, Handler>()
  const sent: { channel: string; payload: unknown }[] = []
  const listeners = new Map<string, () => void>()

  const window = {
    isDestroyed: () => false,
    webContents: {
      send: (channel: string, payload: unknown) => sent.push({ channel, payload }),
      once: (event: string, fn: () => void) => listeners.set(`wc:${event}`, fn),
    },
    once: (event: string, fn: () => void) => listeners.set(event, fn),
  }

  const ask = registerPermissionIpc(
    { handle: (c, h) => handlers.set(c, h as Handler) },
    {
      safeHandle: (ipc, channel, handler) => {
        // Mirrors the real safeHandle: the policy decides what may register.
        if (!validateIpcChannel(channel)) throw new Error(`channel not allowed: ${channel}`)
        ipc.handle(channel, handler)
      },
      getWindow: () => (windowPresent ? (window as never) : null),
    },
  )
  return { ask, handlers, sent, listeners }
}

console.log('test-permission-ipc')

check('permission:respond is allowlisted', validateIpcChannel('permission:respond'))
check(
  'there is no channel for the renderer to ORIGINATE an ask',
  !validateIpcChannel('permission:request'),
  'only the engine may ask; the renderer may only answer',
)

{
  const h = harness()
  check('the response channel is registered', h.handlers.has('permission:respond'))

  const pending = h.ask({ requestId: 'perm-1', operation: 'workflow_execute', target: 'proj' })
  check('the ask reaches the renderer', h.sent[0]?.channel === 'permission:ask')

  // The attack: answer an id nobody issued.
  const forged = await h.handlers.get('permission:respond')!({}, 'perm-999', 'allow_once')
  check('a reply to an unissued id is rejected', (forged as { ok: boolean }).ok === false)

  // And the real request is still waiting — not resolved by the forgery.
  let settled = false
  void pending.then(() => { settled = true })
  await new Promise((r) => setTimeout(r, 20))
  check('the forged reply did not resolve the real request', !settled)

  // The genuine reply settles it.
  const real = await h.handlers.get('permission:respond')!({}, 'perm-1', 'allow_once')
  check('the genuine reply is accepted', (real as { ok: boolean }).ok === true)
  check('the genuine reply resolves the ask', (await pending) === 'allow_once')
}

{
  const h = harness()
  const pending = h.ask({ requestId: 'perm-2', operation: 'x', target: 'y' })
  const bad = await h.handlers.get('permission:respond')!({}, 'perm-2', 'yes-please')
  check('a decision outside the enum is rejected', (bad as { ok: boolean }).ok === false)
  let settled = false
  void pending.then(() => { settled = true })
  await new Promise((r) => setTimeout(r, 20))
  check('a malformed decision does not settle the ask', !settled)
  await h.handlers.get('permission:respond')!({}, 'perm-2', 'reject')
  check('the ask can still be answered properly afterwards', (await pending) === 'reject')
}

{
  const h = harness()
  const pending = h.ask({ requestId: 'perm-3', operation: 'x', target: 'y' })
  h.listeners.get('closed')?.()
  check('closing the window denies rather than hanging', (await pending) === 'reject')
}

{
  const h = harness(false)
  let threw = ''
  try { await h.ask({ requestId: 'perm-4', operation: 'x', target: 'y' }) }
  catch (e) { threw = (e as Error).message }
  check('no window throws, so the broker denies', threw.includes('no window'))
}

{
  const h = harness()
  const first = h.ask({ requestId: 'perm-5', operation: 'x', target: 'y' })
  await h.handlers.get('permission:respond')!({}, 'perm-5', 'allow_once')
  await first
  const replay = await h.handlers.get('permission:respond')!({}, 'perm-5', 'allow_once')
  check('an already-settled id cannot be answered twice', (replay as { ok: boolean }).ok === false)
}

if (failures.length > 0) {
  console.error(`\nFAILED: ${failures.length} of ${passed + failures.length}`)
  process.exit(1)
}
console.log(`\nALL PERMISSION IPC TESTS PASSED (${passed} checks)`)
