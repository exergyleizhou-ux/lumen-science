#!/usr/bin/env npx tsx
/**
 * OSF-8 / office / live-smoke unit tests (shipped modules).
 * Run: npx tsx scripts/test-osf8-office-live.mts
 */
import { strictEqual, ok } from 'node:assert/strict'
import path from 'node:path'
import fs from 'node:fs'
import {
  assertOfficePreviewAdmission,
  listOfficeAdmissions,
  OFFICE_ADMISSION_TABLE,
} from '../src/main/files/office-preview-admission.js'
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
  // ── Office admission fail-closed ─────────────────────────────
  const list = listOfficeAdmissions()
  await test('admission table has 4 formats', () => ok(list.length >= 4))

  const denied = assertOfficePreviewAdmission({
    format: 'docx',
    artifactId: 'a1',
    expectedSha256: 'abc123def4567890abc123de',
  })
  await test('docx open denied until hostile tests', () => {
    ok(!denied.ok)
    ok((denied as { reason: string }).reason.includes('hostile'))
  })

  const noArt = assertOfficePreviewAdmission({ format: 'pdf' })
  await test('requires artifact binding', () => {
    ok(!noArt.ok)
  })

  // Admitted only when all gates true
  const fakeAdmitted = OFFICE_ADMISSION_TABLE.map((a) =>
    a.format === 'pdf'
      ? { ...a, hostileDocTestsPass: true, admitted: true }
      : a,
  )
  const okPdf = assertOfficePreviewAdmission(
    {
      format: 'pdf',
      artifactId: 'pdf-1',
      expectedSha256: 'abc123def4567890abc123de',
    },
    fakeAdmitted,
  )
  await test('pdf opens when fully admitted', () => ok(okPdf.ok))

  // ── Policy channels ──────────────────────────────────────────
  for (const ch of [
    'office:admission-list',
    'office:preview-open',
    'release:checklist-status',
  ]) {
    await test(`policy allows ${ch}`, () => ok(validateIpcChannel(ch)))
  }

  // ── IPC ──────────────────────────────────────────────────────
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
  })
  await test('ipc registers office + release channels', () => {
    ok(handlers.has('office:admission-list'))
    ok(handlers.has('office:preview-open'))
    ok(handlers.has('release:checklist-status'))
  })

  const openH = handlers.get('office:preview-open')!
  const openRes = await openH({}, {
    format: 'docx',
    artifactId: 'a1',
    expectedSha256: 'abc123def4567890abc123de',
  })
  await test('ipc preview-open denied by default', () => {
    ok(openRes.ok === false)
  })

  const listH = handlers.get('office:admission-list')!
  const listRes = await listH({})
  await test('ipc admission list', () => {
    ok(Array.isArray(listRes.admissions))
    ok(listRes.admissions.length >= 4)
  })

  const relH = handlers.get('release:checklist-status')!
  const rel = await relH({})
  await test('release checklist status honest', () => {
    ok(rel.ok)
    ok(rel.binariesUploaded === false)
    ok(rel.notarizationComplete === false)
    ok(rel.checklistPath)
  })

  // ── Checklist file ───────────────────────────────────────────
  const checklist = path.resolve(process.cwd(), '../../docs/science/RELEASE_1.0.1_CHECKLIST.md')
  await test('RELEASE checklist exists', () => ok(fs.existsSync(checklist)))

  // ── OSF-8 script exists ──────────────────────────────────────
  await test('osf8-release-check script exists', () => {
    ok(fs.existsSync('scripts/osf8-release-check.mts'))
  })
  await test('lumen-live-smoke script exists', () => {
    ok(fs.existsSync('scripts/lumen-live-smoke.mts'))
  })

  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

run()
