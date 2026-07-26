#!/usr/bin/env npx tsx
/**
 * Tests for OSF-2 artifact_id file/preview isolation.
 *
 * Drives the shipped resolvePreview + assertArtifactPreviewAccess
 * (from lumen-authority-policy.ts) with real input vectors.
 *
 * Run: npx tsx scripts/test-osf2-preview.mts
 */
import { strictEqual, ok } from 'node:assert/strict'
import fs from 'node:fs'
let failures = 0

function test(name: string, fn: () => void | Promise<void>) {
  Promise.resolve()
    .then(() => fn())
    .then(() => console.log(`OK  ${name}`))
    .catch((e: unknown) => { failures++; console.log(`FAIL ${name}: ${(e as Error).message}`) })
    .catch(() => {}) // suppress unhandled rejection
}

// ── Shipped modules ──────────────────────────────────────────────
import { resolvePreview } from '../src/main/files/preview-resolver.js'
import type { PreviewFileStore } from '../src/main/files/preview-resolver.js'
import { assertArtifactPreviewAccess } from '../src/main/lumen-authority-policy.js'

// ── Mock store ──────────────────────────────────────────────────

class MockStore implements PreviewFileStore {
  private records = new Map<string, { path: string; sha256: string }>()

  constructor() {
    this.records.set('a1', { path: '/store/proj1/a1.csv', sha256: 'abc123def' })
    this.records.set('a2', { path: '/store/proj2/a2.json', sha256: 'xyz789' })
  }

  async resolveById(artifactId: string) {
    return this.records.get(artifactId) ?? null
  }
}

const store = new MockStore()

async function runTests() {
  // ── Policy-level tests (shipped assertArtifactPreviewAccess) ──
  // These verify the pure policy rejects invalid access BEFORE
  // any file/store resolution happens.

  const rOwner = assertArtifactPreviewAccess(
    { artifactId: 'a1', ownerId: 'oX', projectId: 'p1' },
    { ownerId: 'o1', projectId: 'p1' }
  )
  test('policy: rejects wrong owner', () => {
    ok(!rOwner.ok, 'access should be denied for wrong owner')
    ok(rOwner.reason!.includes('owner'), `reason should mention owner: ${rOwner.reason}`)
  })

  const rProj = assertArtifactPreviewAccess(
    { artifactId: 'a1', ownerId: 'o1', projectId: 'pX' },
    { ownerId: 'o1', projectId: 'p1' }
  )
  test('policy: rejects wrong project', () => {
    ok(!rProj.ok, 'access should be denied for wrong project')
    ok(rProj.reason!.includes('project'), `reason should mention project: ${rProj.reason}`)
  })

  const rHash = assertArtifactPreviewAccess(
    { artifactId: 'a1', ownerId: 'o1', projectId: 'p1', expectedSha256: 'wrong' },
    { ownerId: 'o1', projectId: 'p1', digest: 'correct' }
  )
  test('policy: rejects hash mismatch', () => {
    ok(!rHash.ok, 'access should be denied for hash mismatch')
    ok(rHash.reason!.includes('sha256'), `reason should mention sha256: ${rHash.reason}`)
  })

  const rEmpty = assertArtifactPreviewAccess(
    { artifactId: '', ownerId: 'o1', projectId: 'p1' },
    { ownerId: 'o1', projectId: 'p1' }
  )
  test('policy: rejects empty artifact_id', () => {
    ok(!rEmpty.ok, 'access should be denied for empty id')
    ok(rEmpty.reason!.includes('required'), `reason: ${rEmpty.reason}`)
  })

  const rValid = assertArtifactPreviewAccess(
    { artifactId: 'a1', ownerId: 'o1', projectId: 'p1' },
    { ownerId: 'o1', projectId: 'p1' }
  )
  test('policy: allows valid', () => ok(rValid.ok))

  // ── Resolver-level tests (resolvePreview) ─────────────────────

  const r7 = await resolvePreview(
    { artifactId: 'a1', ownerId: 'o1', projectId: 'p1', mimeType: 'text/csv' },
    store
  )
  test('resolve: valid returns path', () => {
    ok(r7.access.ok)
    strictEqual(r7.path, '/store/proj1/a1.csv')
    strictEqual(r7.mimeType, 'text/csv')
  })

  const r8 = await resolvePreview(
    { artifactId: 'nonexistent', ownerId: 'o1', projectId: 'p1' },
    store
  )
  test('resolve: not found returns error', () => {
    ok(!r8.access.ok)
    ok(r8.access.reason!.includes('not found'), `reason: ${r8.access.reason}`)
  })

  const r9 = await resolvePreview(
    { artifactId: 'a1', ownerId: 'o1', projectId: 'p1', expectedSha256: 'abc123def' },
    store
  )
  test('resolve: valid + hash match', () => {
    ok(r9.access.ok)
    strictEqual(r9.path, '/store/proj1/a1.csv')
  })

  // ── Source constraints ─────────────────────────────────────────

  const src = fs.readFileSync('src/main/files/preview-resolver.ts', 'utf-8')
  test('preview-resolver: imports shipped policy', () => {
    ok(src.includes('assertArtifactPreviewAccess'))
    ok(src.includes('../lumen-authority-policy'))
  })

  test('preview-resolver: calls assertArtifactPreviewAccess', () => {
    ok(src.includes('assertArtifactPreviewAccess('), 'must call policy function')
  })

  // Wait for async tests to complete
  await new Promise(r => setTimeout(r, 500))
  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

runTests()
