#!/usr/bin/env npx tsx
/**
 * Permission broker — negative tests.
 *
 * The property worth testing is not "an allow reaches the engine". It is that
 * NOTHING produces an allow except a human clicking allow. Every other path —
 * no window, a closed dialog, a timeout, an unreadable request, a reply to an
 * id nobody issued — must deny.
 *
 * That matters because the seam was left unused on purpose: auto-approving in
 * the main process grants execution authority with nobody in the loop. A future
 * change that "helpfully" defaults to allow would be invisible in normal use,
 * because normal use is a person clicking allow anyway.
 *
 *   npx tsx scripts/test-permission-broker.mts
 */
import {
  PermissionBroker,
  describeAsk,
  type PermissionAsk,
} from '../src/main/permission-broker.js'

let passed = 0
const failures: string[] = []

function check(label: string, condition: boolean, detail = ''): void {
  if (condition) {
    passed += 1
    console.log(`  ok    ${label}`)
  } else {
    failures.push(label)
    console.log(`  FAIL  ${label}${detail ? ` — ${detail}` : ''}`)
  }
}

const REQUEST = {
  method: 'x.ai/science/workflow_execute',
  target: 'project "Restriction mapping"',
  detail: 'runs 1 notebook cell',
}

const allowed = (o: { outcome: string }): boolean => o.outcome === 'selected'

console.log('test-permission-broker')

// ── the only path that allows ────────────────────────────────────
{
  const broker = new PermissionBroker({ ask: async () => 'allow_once' })
  const outcome = await broker.handle('req-1', REQUEST)
  check('a human clicking allow produces an allow', allowed(outcome))
}

// ── everything else denies ───────────────────────────────────────
{
  const broker = new PermissionBroker({ ask: async () => 'reject' })
  check('a human clicking reject denies', !allowed(await broker.handle('req-2', REQUEST)))
}

{
  // No window: `ask` cannot present anything and throws.
  const denials: string[] = []
  const broker = new PermissionBroker({
    ask: async () => {
      throw new Error('no window is open')
    },
    onDenied: (_ask, reason) => denials.push(reason),
  })
  check('no window to ask denies', !allowed(await broker.handle('req-3', REQUEST)))
  check('the denial reason is reported', denials.some((d) => d.includes('no window')))
}

{
  // The user closed the dialog without choosing.
  const broker = new PermissionBroker({
    ask: async () => {
      throw new Error('dialog dismissed')
    },
  })
  check('a dismissed dialog denies', !allowed(await broker.handle('req-4', REQUEST)))
}

{
  // Nobody ever answers. Silence is not consent.
  const denials: string[] = []
  const broker = new PermissionBroker({
    ask: () => new Promise(() => {}),
    timeoutMs: 60,
    onDenied: (_a, reason) => denials.push(reason),
  })
  const started = Date.now()
  const outcome = await broker.handle('req-5', REQUEST)
  check('an unanswered request times out and denies', !allowed(outcome))
  check('the timeout is a denial reason, not a crash', denials.some((d) => d.includes('no answer')))
  check('the timeout actually bounded the wait', Date.now() - started < 5_000)
}

{
  // An unreadable request must not reach a human at all: a dialog whose text
  // we could not parse invites a click on something nobody understood.
  let asked = false
  const broker = new PermissionBroker({
    ask: async () => {
      asked = true
      return 'allow_once'
    },
  })
  check('an unparseable request denies', !allowed(await broker.handle('req-6', null)))
  check('an unparseable request is never shown to a human', !asked)
  check('a request with no operation denies', !allowed(await broker.handle('req-7', { target: 'x' })))
}

{
  // The broker must not leak pending state, or a long session accumulates
  // entries that a shutdown would have to guess about.
  const broker = new PermissionBroker({ ask: async () => 'reject' })
  await broker.handle('req-8', REQUEST)
  check('a settled request is no longer pending', broker.pendingCount() === 0)

  const slow = new PermissionBroker({ ask: () => new Promise(() => {}), timeoutMs: 40 })
  const inFlight = slow.handle('req-9', REQUEST)
  check('an in-flight request is tracked', slow.pendingCount() === 1)
  await inFlight
  check('a timed-out request is cleaned up', slow.pendingCount() === 0)
}

// ── the ask a human sees describes the real operation ────────────
{
  const asks: PermissionAsk[] = []
  const broker = new PermissionBroker({
    ask: async (a) => {
      asks.push(a)
      return 'reject'
    },
  })
  await broker.handle('req-10', REQUEST)
  check('the operation is shown', asks[0]?.operation === 'x.ai/science/workflow_execute')
  check('the target is shown', asks[0]?.target.includes('Restriction mapping'))
  check('the detail is shown', asks[0]?.detail === 'runs 1 notebook cell')
  check('the request id is carried', asks[0]?.requestId === 'req-10')
}

{
  // A toolCall-shaped request (what the Rust permission manager sends for a
  // science mutation) must also be readable.
  const ask = describeAsk('req-11', { toolCall: { title: 'Lumen Science project mutation', kind: 'other' } })
  check('a toolCall-shaped request is readable', ask?.operation === 'Lumen Science project mutation')
}

if (failures.length > 0) {
  console.error(`\nFAILED: ${failures.length} of ${passed + failures.length}`)
  process.exit(1)
}
console.log(`\nALL PERMISSION BROKER TESTS PASSED (${passed} checks)`)
