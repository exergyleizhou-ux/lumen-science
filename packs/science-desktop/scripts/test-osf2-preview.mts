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

import os from 'node:os'
import { createHash as createHashFix } from 'node:crypto'
import fsSync from 'node:fs'
import pathMod from 'node:path'

// Real files, because the resolver now reads the BYTES: a record whose file
// does not exist or whose content drifted from its digest must fail closed,
// and these fixtures passed for months precisely because nothing ever looked.
const FIXTURE_DIR = fsSync.mkdtempSync(pathMod.join(os.tmpdir(), 'preview-fixture-'))
const A1_PATH = pathMod.join(FIXTURE_DIR, 'a1.csv')
const A2_PATH = pathMod.join(FIXTURE_DIR, 'a2.json')
fsSync.writeFileSync(A1_PATH, 'a1,csv,fixture\n')
fsSync.writeFileSync(A2_PATH, '{"a2": "fixture"}\n')
const A1_SHA = '9b4aca952a616872cadde825ea698de8184542ce3be9d93a280dd2a4dc42eab7'
const A2_SHA = '5595adc2610db1d3874d2e2a3aeb2b574cef9553a3b5976f7cdff014908dd6d2'

class MockStore implements PreviewFileStore {
  private records = new Map<
    string,
    { path: string; sha256: string; ownerId: string; projectId: string }
  >()

  constructor() {
    this.records.set('a1', {
      path: A1_PATH,
      sha256: A1_SHA,
      ownerId: 'o1',
      projectId: 'p1',
    })
    this.records.set('a2', {
      path: A2_PATH,
      sha256: A2_SHA,
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
    { artifactId: 'a1', expectedSha256: A1_SHA, mimeType: 'text/csv' },
    store,
    { ownerId: 'o1', projectId: 'p1' },
  )
  await test('resolve: trusted owner+project + hash match', () => {
    ok(okResolve.access.ok)
    strictEqual(okResolve.mimeType, 'text/csv')
    ok(!('path' in okResolve), 'verified previews must never expose a reopenable path')
    strictEqual(okResolve.byteLength, Buffer.byteLength('a1,csv,fixture\n'))
    strictEqual(
      Buffer.from(okResolve.contentBase64 ?? '', 'base64').toString(),
      'a1,csv,fixture\n',
    )
  })

  const crossOwner = await resolvePreview(
    { artifactId: 'a1' },
    store,
    { ownerId: 'attacker', projectId: 'p1' },
  )
  await test('resolve: rejects cross-owner even if client only sends artifactId', () => {
    ok(!crossOwner.access.ok)
    ok(crossOwner.access.reason!.includes('owner'))
    strictEqual(crossOwner.contentBase64, undefined)
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
    { artifactId: 'a1', expectedSha256: A1_SHA, mimeType: 'text/csv' },
    { store },
  )
  await test('product: loadArtifactPreview ok with trusted session', () => {
    ok(product.access.ok, `expected ok got ${JSON.stringify(product)}`)
    strictEqual(
      Buffer.from(product.contentBase64 ?? '', 'base64').toString(),
      'a1,csv,fixture\n',
    )
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
  await test('preview result is bytes-only with no post-hash reopen seam', () => {
    ok(src.includes('contentBase64'))
    ok(src.includes('handle.readFile()'))
    ok(!src.includes('path: resolved.path'))
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

  // ── the bytes must match their record ────────────────────────────
  // The resolver re-hashes the file. Without this, a record CLAIMING a digest was
  // enough: every fixture in this pack pointed at /store/... paths that never
  // existed, and every preview test passed. An artifact that has been modified,
  // truncated or deleted since it was recorded must fail closed HERE, where the
  // reason is still available, not later in whatever consumed it.
  {
    const tamperDir = fsSync.mkdtempSync(pathMod.join(os.tmpdir(), 'preview-tamper-'))
    const tamperPath = pathMod.join(tamperDir, 'drifted.csv')
    fsSync.writeFileSync(tamperPath, 'original\n')
    const recordedSha = createHashFix('sha256').update('original\n').digest('hex')

    const driftStore: PreviewFileStore = {
      async resolveById() {
        return { path: tamperPath, sha256: recordedSha, ownerId: 'o1', projectId: 'p1' }
      },
    }
    const trusted = { ownerId: 'o1', projectId: 'p1' }

    const before = await resolvePreview({ artifactId: 'x' }, driftStore, trusted)
    await test('an intact artifact resolves', () => {
      ok(before.access.ok, JSON.stringify(before))
    })

    fsSync.writeFileSync(tamperPath, 'tampered\n')
    const after = await resolvePreview({ artifactId: 'x' }, driftStore, trusted)
    await test('an artifact modified since recording fails closed', () => {
      ok(!after.access.ok)
      ok((after.access.reason ?? '').includes('do not match their record'), after.access.reason)
    })

    fsSync.rmSync(tamperPath)
    const gone = await resolvePreview({ artifactId: 'x' }, driftStore, trusted)
    await test('a deleted artifact is not previewable', () => {
      ok(!gone.access.ok)
      ok((gone.access.reason ?? '').includes('unavailable'), gone.access.reason)
    })
  }

  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

runTests()
