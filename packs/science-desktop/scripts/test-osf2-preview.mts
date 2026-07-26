#!/usr/bin/env npx tsx
/**
 * Tests for OSF-2 artifact_id file/preview isolation.
 *
 * Drives shipped resolvePreview + assertArtifactPreviewAccess with real
 * vectors. Owner/project must come from trusted main-process identity
 * compared to store-owned metadata — NOT client self-attestation.
 *
 * Run: npx tsx scripts/test-osf2-preview.mts
 */
import { strictEqual, ok } from 'node:assert/strict'
import fs from 'node:fs'
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

import { resolvePreview } from '../src/main/files/preview-resolver.js'
import type { PreviewFileStore } from '../src/main/files/preview-resolver.js'
import { assertArtifactPreviewAccess } from '../src/main/lumen-authority-policy.js'
import {
  getTrustedPreviewContext,
  setTrustedPreviewContext,
  clearTrustedPreviewContext,
} from '../src/main/files/session-identity.js'
import { loadArtifactPreview } from '../src/main/files/preview-service.js'

class MockStore implements PreviewFileStore {
  private records = new Map<
    string,
    { path: string; sha256: string; ownerId: string; projectId: string }
  >()

  constructor() {
    this.records.set('a1', {
      path: '/store/proj1/a1.csv',
      sha256: 'abc123def',
      ownerId: 'o1',
      projectId: 'p1',
    })
    this.records.set('a2', {
      path: '/store/proj2/a2.json',
      sha256: 'xyz789',
      ownerId: 'o2',
      projectId: 'p2',
    })
  }

  async resolveById(artifactId: string) {
    return this.records.get(artifactId) ?? null
  }
}

const store = new MockStore()

async function runTests() {
  // ── Policy-level tests ─────────────────────────────────────────
  const rOwner = assertArtifactPreviewAccess(
    { artifactId: 'a1', ownerId: 'oX', projectId: 'p1' },
    { ownerId: 'o1', projectId: 'p1' },
  )
  await test('policy: rejects wrong owner', () => {
    ok(!rOwner.ok)
    ok(rOwner.reason!.includes('owner'))
  })

  const rProj = assertArtifactPreviewAccess(
    { artifactId: 'a1', ownerId: 'o1', projectId: 'pX' },
    { ownerId: 'o1', projectId: 'p1' },
  )
  await test('policy: rejects wrong project', () => {
    ok(!rProj.ok)
    ok(rProj.reason!.includes('project'))
  })

  const rHash = assertArtifactPreviewAccess(
    { artifactId: 'a1', ownerId: 'o1', projectId: 'p1', expectedSha256: 'wrong' },
    { ownerId: 'o1', projectId: 'p1', digest: 'correct' },
  )
  await test('policy: rejects hash mismatch', () => {
    ok(!rHash.ok)
    ok(rHash.reason!.includes('sha256'))
  })

  const rEmpty = assertArtifactPreviewAccess(
    { artifactId: '', ownerId: 'o1', projectId: 'p1' },
    { ownerId: 'o1', projectId: 'p1' },
  )
  await test('policy: rejects empty artifact_id', () => {
    ok(!rEmpty.ok)
    ok(rEmpty.reason!.includes('required'))
  })

  const rValid = assertArtifactPreviewAccess(
    { artifactId: 'a1', ownerId: 'o1', projectId: 'p1' },
    { ownerId: 'o1', projectId: 'p1' },
  )
  await test('policy: allows valid', () => ok(rValid.ok))

  // ── Resolver: trusted context vs store ownership ───────────────
  // Client cannot self-attest into another owner's artifact.

  const okResolve = await resolvePreview(
    { artifactId: 'a1', expectedSha256: 'abc123def', mimeType: 'text/csv' },
    store,
    { ownerId: 'o1', projectId: 'p1' },
  )
  await test('resolve: trusted owner+project + hash match', () => {
    ok(okResolve.access.ok)
    strictEqual(okResolve.path, '/store/proj1/a1.csv')
    strictEqual(okResolve.mimeType, 'text/csv')
  })

  const crossOwner = await resolvePreview(
    { artifactId: 'a1' },
    store,
    { ownerId: 'attacker', projectId: 'p1' },
  )
  await test('resolve: rejects cross-owner even if client only sends artifactId', () => {
    ok(!crossOwner.access.ok)
    ok(crossOwner.access.reason!.includes('owner'))
    strictEqual(crossOwner.path, undefined)
  })

  const crossProj = await resolvePreview(
    { artifactId: 'a1' },
    store,
    { ownerId: 'o1', projectId: 'other-project' },
  )
  await test('resolve: rejects cross-project', () => {
    ok(!crossProj.access.ok)
    ok(crossProj.access.reason!.includes('project'))
  })

  const badHash = await resolvePreview(
    { artifactId: 'a1', expectedSha256: 'deadbeef' },
    store,
    { ownerId: 'o1', projectId: 'p1' },
  )
  await test('resolve: rejects hash mismatch after store load', () => {
    ok(!badHash.access.ok)
    ok(badHash.access.reason!.includes('sha256'))
  })

  const missing = await resolvePreview(
    { artifactId: 'nonexistent' },
    store,
    { ownerId: 'o1', projectId: 'p1' },
  )
  await test('resolve: not found returns error', () => {
    ok(!missing.access.ok)
    ok(missing.access.reason!.includes('not found'))
  })

  // ── Session identity (trusted context source) ──────────────────
  clearTrustedPreviewContext()
  await test('session-identity: default null', () => {
    strictEqual(getTrustedPreviewContext(), null)
  })
  setTrustedPreviewContext({ ownerId: 'o1', projectId: 'p1' })
  await test('session-identity: set/get', () => {
    const ctx = getTrustedPreviewContext()
    ok(ctx)
    strictEqual(ctx!.ownerId, 'o1')
    strictEqual(ctx!.projectId, 'p1')
  })
  clearTrustedPreviewContext()

  // ── Product path: loadArtifactPreview ──────────────────────────
  setTrustedPreviewContext({ ownerId: 'o1', projectId: 'p1' })
  const product = await loadArtifactPreview(
    { artifactId: 'a1', expectedSha256: 'abc123def', mimeType: 'text/csv' },
    { store },
  )
  await test('product: loadArtifactPreview ok with trusted session', () => {
    ok(product.access.ok, `expected ok got ${JSON.stringify(product)}`)
    strictEqual(product.path, '/store/proj1/a1.csv')
  })

  clearTrustedPreviewContext()
  const noSession = await loadArtifactPreview({ artifactId: 'a1' }, { store })
  await test('product: rejects when no trusted session identity', () => {
    ok(!noSession.access.ok, `expected deny got ${JSON.stringify(noSession)}`)
    ok(
      (noSession.access.reason ?? '').includes('session'),
      `reason: ${noSession.access.reason}`,
    )
  })

  setTrustedPreviewContext({ ownerId: 'o2', projectId: 'p2' })
  const wrongSession = await loadArtifactPreview({ artifactId: 'a1' }, { store })
  await test('product: rejects artifact owned by other session identity', () => {
    ok(!wrongSession.access.ok, `expected deny got ${JSON.stringify(wrongSession)}`)
    ok(
      (wrongSession.access.reason ?? '').includes('owner'),
      `reason: ${wrongSession.access.reason}`,
    )
  })
  clearTrustedPreviewContext()

  // ── Source constraints ─────────────────────────────────────────
  const src = fs.readFileSync('src/main/files/preview-resolver.ts', 'utf-8')
  await test('preview-resolver: imports shipped policy', () => {
    ok(src.includes('assertArtifactPreviewAccess'))
    ok(src.includes('../lumen-authority-policy'))
  })
  await test('preview-resolver: requires trusted context param', () => {
    ok(src.includes('TrustedPreviewContext') || src.includes('trusted:'))
    // Must not compare request owner to itself (the prior theater bug)
    ok(!/assertArtifactPreviewAccess\(\s*\{[^}]*ownerId:\s*req\.ownerId[^}]*\},\s*\{\s*ownerId:\s*req\.ownerId/s.test(src))
  })

  const ipcSrc = fs.readFileSync('src/main/ipc.ts', 'utf-8')
  await test('ipc.ts: wires registerScienceIpcHandlers (OSF-2 product path)', () => {
    ok(ipcSrc.includes('registerScienceIpcHandlers'))
    ok(ipcSrc.includes('AcpPreviewStore') || ipcSrc.includes('previewStore'))
  })
  const scienceSrc = fs.readFileSync('src/main/files/science-ipc.ts', 'utf-8')
  await test('science-ipc: registers files:preview-by-artifact', () => {
    ok(scienceSrc.includes("'files:preview-by-artifact'"))
    ok(scienceSrc.includes('loadArtifactPreview'))
  })

  const policySrc = fs.readFileSync('src/main/lumen-authority-policy.ts', 'utf-8')
  await test('policy: allows files:preview-by-artifact', () => {
    ok(policySrc.includes("'files:preview-by-artifact'"))
  })

  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

runTests()
