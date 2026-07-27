#!/usr/bin/env npx tsx
/**
 * describeOpen — the open-project outcome message.
 *
 * The property under test is that nothing is SWALLOWED. It is easy to make the
 * ugly banner go away by dropping the engine's words; that would be a product
 * that claims to have opened cleanly while a real seed failure went unreported.
 *
 *   npx tsx scripts/test-describe-open.mts
 */
import { describeOpen } from '../src/renderer/src/pages/research/describe-open.js'

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

console.log('test-describe-open')

const STRUCTURAL =
  "science method 'artifact_list' rejected by registry: Go MCP tool, not a Rust ACP " +
  'extension method. The Rust engine dispatches only x.ai/science/* ' +
  '(extensions/science.rs); this call site needs the Go MCP client, not this bridge.'

{
  const out = describeOpen({ seeded: 0 })
  check('an empty new project is not reported as a failure', out.expected)
  check('and it does not lead with a zero', !/^Open: seeded 0/.test(out.headline), out.headline)
  check('nothing technical is invented', out.detail === undefined)
}

{
  const out = describeOpen({ seeded: 3 })
  check('a seeded project says how many', out.headline.includes('3 artifact'))
}

{
  const out = describeOpen({ seeded: 1 })
  check('one artifact is singular', out.headline.includes('1 artifact ready'), out.headline)
}

{
  const out = describeOpen({ seeded: 0, seedError: STRUCTURAL })
  check('a structural absence is expected, not an alarm', out.expected)
  check('the headline is a plain sentence', !out.headline.includes('artifact_list'), out.headline)
  check('the headline names no source file', !out.headline.includes('.rs'), out.headline)
  // The whole point: quieter, NOT quieter by deletion.
  check('the engine text is kept verbatim', out.detail === STRUCTURAL)
}

{
  // An unrecognised failure must not be dressed up as a known absence.
  const out = describeOpen({ seeded: 0, seedError: 'disk I/O error reading run manifest' })
  check('an unrecognised seed failure is flagged unexpected', !out.expected)
  check('and its text is kept too', out.detail === 'disk I/O error reading run manifest')
  check('and the headline says artifacts are missing', /could not be loaded/i.test(out.headline))
}

if (failures.length > 0) {
  console.error(`\nFAILED: ${failures.length} of ${passed + failures.length}`)
  process.exit(1)
}
console.log(`\nALL DESCRIBE-OPEN TESTS PASSED (${passed} checks)`)
