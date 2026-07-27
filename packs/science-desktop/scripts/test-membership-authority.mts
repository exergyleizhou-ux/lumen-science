#!/usr/bin/env npx tsx
/**
 * Membership authority — negative tests (LS5-D2-02).
 *
 * The defect: `createHybridMembershipAsserter` called ACP, and on ANY non-grant
 * fell through to a local JSON catalog the desktop itself writes. Its comment
 * claimed it distinguished an explicit denial from a missing tool; the code did
 * not, because `MembershipResult` had no way to express the difference. So an
 * ACP denial and a crashed engine both ended up granting from local state — and
 * since the bridge never connected, that was the only path anything took.
 *
 * These tests exist because the fix is invisible in normal operation: with a
 * healthy engine that grants, the old and new code behave identically. Only the
 * failure paths differ, so only failure paths can prove the fix holds.
 *
 *   npx tsx scripts/test-membership-authority.mts
 */
import { strictEqual, ok } from 'node:assert/strict'

import {
  createAcpAuthoritativeMembershipAsserter,
  createOfflineCatalogMembershipAsserter
} from '../src/main/files/hybrid-membership.js'
import {
  createAcpMembershipAsserter,
  listArtifactsViaAcp,
} from '../src/main/files/acp-membership.js'
import type { MembershipAsserter, MembershipResult } from '../src/main/files/session-binding.js'

let passed = 0
const check = (label: string, condition: boolean, detail = ''): void => {
  if (condition) {
    passed += 1
    console.log(`  ok    ${label}`)
  } else {
    console.error(`  FAIL  ${label}${detail ? ` — ${detail}` : ''}`)
    process.exitCode = 1
  }
}

const CLAIM = { ownerId: 'owner-a', projectId: 'project-1' }

// A catalog that grants EVERYTHING. If any test below passes because of this,
// the authority has been bypassed — which is exactly the regression to catch.
const permissiveCatalog = {
  hasMembership: () => true
} as unknown as Parameters<typeof createOfflineCatalogMembershipAsserter>[0]['catalog']

const acpReturning = (result: MembershipResult): MembershipAsserter => async () => result

console.log('test-membership-authority')

// ── the core regression ──────────────────────────────────────────

{
  // An ACP denial must be final. Previously the catalog overrode it.
  const assert = createAcpAuthoritativeMembershipAsserter({
    acp: acpReturning({ ok: false, failure: 'denied', reason: 'not a member' })
  })
  const result = await assert(CLAIM)
  check('ACP denial is final', result.ok === false)
  check(
    'ACP denial reports failure=denied',
    result.ok === false && result.failure === 'denied'
  )
}

{
  // ACP unreachable must deny. Not knowing is not permission.
  const assert = createAcpAuthoritativeMembershipAsserter({
    acp: acpReturning({ ok: false, failure: 'unavailable', reason: 'ECONNREFUSED' })
  })
  const result = await assert(CLAIM)
  check('ACP unavailable fails closed', result.ok === false)
  check(
    'ACP unavailable is reported as unavailable, not denied',
    result.ok === false && result.failure === 'unavailable'
  )
}

{
  // THE regression test. It must construct the exact situation the defect
  // needed: an ACP that says no, AND a catalog that says yes, passed together.
  //
  // An earlier version of this test built the denying asserter WITHOUT a
  // catalog, so a reintroduced fall-through had nothing to fall through to and
  // the test passed against the very bug it was written for. Verified by
  // re-injecting the fall-through and confirming these two checks now fail.
  //
  // `as unknown as` is deliberate: the production factory's type has no catalog
  // parameter, so this forces one in the way a careless refactor would.
  const factory = createAcpAuthoritativeMembershipAsserter as unknown as (o: {
    acp: MembershipAsserter
    catalog: unknown
  }) => MembershipAsserter

  const deniedWithPermissiveCatalog = await factory({
    acp: acpReturning({ ok: false, failure: 'denied', reason: 'not a member' }),
    catalog: permissiveCatalog
  })(CLAIM)
  check(
    'a permissive catalog cannot rescue an ACP denial',
    deniedWithPermissiveCatalog.ok === false,
    'local state overrode the authority'
  )

  const unavailableWithPermissiveCatalog = await factory({
    acp: acpReturning({ ok: false, failure: 'unavailable', reason: 'ECONNREFUSED' }),
    catalog: permissiveCatalog
  })(CLAIM)
  check(
    'a permissive catalog cannot rescue an unreachable ACP',
    unavailableWithPermissiveCatalog.ok === false,
    'a crashed engine became an allow'
  )
}

{
  const assert = createAcpAuthoritativeMembershipAsserter({
    acp: acpReturning({ ok: true, ...CLAIM })
  })
  const result = await assert(CLAIM)
  check('ACP grant is honoured', result.ok === true)
}

// ── classification at the source ─────────────────────────────────

{
  // A thrown transport error is `unavailable`. Classifying it `denied` would be
  // a different lie: it would report a decision the authority never made.
  const assert = createAcpMembershipAsserter(async () => {
    throw new Error('ECONNREFUSED 127.0.0.1:17000')
  })
  const result = await assert(CLAIM)
  check(
    'transport failure classifies as unavailable',
    result.ok === false && result.failure === 'unavailable'
  )
}

{
  const assert = createAcpMembershipAsserter(async () => ({ ok: false, reason: 'nope' }))
  const result = await assert(CLAIM)
  check(
    'explicit ok:false classifies as denied',
    result.ok === false && result.failure === 'denied'
  )
}

{
  // The engine granting a DIFFERENT identity than was claimed is a denial of
  // the claim actually made — not a grant, and not an outage.
  const assert = createAcpMembershipAsserter(async () => ({
    ok: true,
    owner_id: 'someone-else',
    project_id: CLAIM.projectId
  }))
  const result = await assert(CLAIM)
  check(
    'identity mismatch is denied, not granted',
    result.ok === false && result.failure === 'denied'
  )
}

{
  // An unreadable answer is not a denial: we never learned the decision.
  const assert = createAcpMembershipAsserter(async () => null)
  const result = await assert(CLAIM)
  check(
    'empty response classifies as unavailable',
    result.ok === false && result.failure === 'unavailable'
  )
}

// ── artifact-list authority binding ─────────────────────────────

{
  let calledMethod = ''
  let calledArgs: Record<string, unknown> = {}
  const items = await listArtifactsViaAcp(
    async (method, args) => {
      calledMethod = method
      calledArgs = args
      return {
        artifacts: [
          {
            artifact_id: 'a'.repeat(64),
            path: '/bound/workspace/science-store/runs/run-1/artifacts/report.md',
            sha256: 'a'.repeat(64),
            owner_id: 'owner-a',
            project_id: 'project-1',
            run_id: 'run-1',
          },
        ],
      }
    },
    { ownerId: 'owner-a', projectId: 'project-1', runId: 'run-1' },
  )
  check('artifact listing uses the Rust artifact_list method', calledMethod === 'artifact_list')
  check(
    'artifact listing binds owner/project/run and confined store root',
    JSON.stringify(calledArgs) ===
      JSON.stringify({
        ownerId: 'owner-a',
        projectId: 'project-1',
        runId: 'run-1',
        storeRoot: 'science-store',
      }),
  )
  check('artifact listing preserves the verified engine row', items.length === 1)
}

{
  for (const [label, response] of [
    ['non-array response', { ok: true }],
    [
      'owner mismatch',
      [
        {
          artifact_id: 'a'.repeat(64),
          path: '/bound/workspace/report.md',
          sha256: 'a'.repeat(64),
          owner_id: 'other-owner',
          project_id: 'project-1',
          run_id: 'run-1',
        },
      ],
    ],
    [
      'hash mismatch',
      [
        {
          artifact_id: 'a'.repeat(64),
          path: '/bound/workspace/report.md',
          sha256: 'b'.repeat(64),
          owner_id: 'owner-a',
          project_id: 'project-1',
          run_id: 'run-1',
        },
      ],
    ],
  ] as const) {
    try {
      await listArtifactsViaAcp(
        async () => response,
        { ownerId: 'owner-a', projectId: 'project-1', runId: 'run-1' },
      )
      check(`artifact listing rejects ${label}`, false)
    } catch {
      check(`artifact listing rejects ${label}`, true)
    }
  }
}

// ── the offline asserter is honestly scoped ──────────────────────

{
  const assert = createOfflineCatalogMembershipAsserter({ catalog: permissiveCatalog })
  const result = await assert(CLAIM)
  check('offline asserter grants from the catalog (its stated purpose)', result.ok === true)
}

{
  const empty = { hasMembership: () => false } as unknown as Parameters<
    typeof createOfflineCatalogMembershipAsserter
  >[0]['catalog']
  const assert = createOfflineCatalogMembershipAsserter({ catalog: empty })
  const result = await assert(CLAIM)
  check(
    'offline asserter denies an unknown project',
    result.ok === false && result.failure === 'no-record'
  )
}

// ── the production wiring uses the authoritative factory ─────────

{
  const ipc = await import('node:fs').then(({ readFileSync }) =>
    readFileSync(new URL('../src/main/ipc.ts', import.meta.url), 'utf8')
  )
  check(
    'ipc.ts wires the ACP-authoritative asserter',
    ipc.includes('createAcpAuthoritativeMembershipAsserter')
  )
  check(
    'ipc.ts does not wire the offline catalog asserter',
    !ipc.includes('createOfflineCatalogMembershipAsserter'),
    'production must never grant from local state'
  )
}

console.log(`\n${process.exitCode ? 'FAILED' : 'ALL TESTS PASSED'} (${passed} checks)`)
