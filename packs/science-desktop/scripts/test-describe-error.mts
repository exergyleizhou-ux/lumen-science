#!/usr/bin/env npx tsx
/**
 * describeError — the honesty constraint, tested.
 *
 * The temptation with a friendly error message is to REPLACE the technical one.
 * That is how a product ends up saying "something went wrong" while the cause
 * sits in a log nobody reads. So the property under test is not "the headline
 * is nice" but "the original text always survives, unmodified".
 *
 *   npx tsx scripts/test-describe-error.mts
 */
import { describeError } from '../src/renderer/src/pages/research/describe-error.js'

let passed = 0
const failures: string[] = []
const check = (label: string, ok: boolean, detail = ''): void => {
  if (ok) { passed += 1; console.log(`  ok    ${label}`) }
  else { failures.push(label); console.log(`  FAIL  ${label}${detail ? ` — ${detail}` : ''}`) }
}

console.log('test-describe-error')

const REAL =
  "membership undetermined, failing closed: membership ACP error: science method " +
  "'project_assert_membership' rejected by registry: no such method in either engine; " +
  "files/acp-membership.ts invented it."

{
  const d = describeError(REAL)
  check('the original text survives verbatim', d.detail === REAL.trim(), d.detail.slice(0, 60))
  check('a headline is added, not substituted', d.headline.length > 0 && d.detail.includes('acp-membership.ts'))
  check('a registry refusal is recognised', d.headline.includes('not available yet'))
  check('a refusal by design is not alarmed', d.expected === true)
}

// Every branch must keep the detail. This is the one invariant that must not
// depend on which rule matched.
for (const raw of [
  'ECONNREFUSED 127.0.0.1',
  'membership denied by ACP: not a member',
  'no answer within 300000ms',
  'membership undetermined, failing closed: x',
  'no such method in either engine',
  'some entirely novel failure nobody predicted',
]) {
  const d = describeError(raw)
  check(`detail preserved: ${raw.slice(0, 34)}…`, d.detail === raw)
  check(`headline present: ${raw.slice(0, 34)}…`, d.headline.length > 10)
}

{
  // An unrecognised failure must NOT get a reassuring headline — it is exactly
  // the one worth looking at, and it must render as an alert rather than a
  // neutral notice.
  const d = describeError('kernel panic: unexpected state 0x7f')
  check('an unknown failure says it is unrecognised', d.headline.toLowerCase().includes('not recognise'))
  check('an unknown failure is flagged unexpected', d.expected === false)
}

{
  const offline = describeError('the transport is not wired')
  check('an unreachable engine is distinguished from a denial', offline.headline.includes('not running'))
  const denied = describeError('membership denied by ACP')
  check('a denial says the engine refused', denied.headline.includes('refused'))
  check(
    'the two are different headlines',
    offline.headline !== denied.headline,
    'a user who cannot tell "unreachable" from "refused" cannot fix either',
  )
}

if (failures.length > 0) {
  console.error(`\nFAILED: ${failures.length} of ${passed + failures.length}`)
  process.exit(1)
}
console.log(`\nALL DESCRIBE-ERROR TESTS PASSED (${passed} checks)`)
