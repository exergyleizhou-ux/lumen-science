#!/usr/bin/env npx tsx
/**
 * OSF-5 Skills admission tests — shipped skill-plan + skill-service.
 * Run: npx tsx scripts/test-osf5-skills.mts
 */
import { strictEqual, ok } from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import {
  planSkillImport,
  planSkillAdmit,
  rejectBulkAutoApprove,
  hashSkillContent,
} from '../src/main/files/skill-plan.js'
import { createSkillService } from '../src/main/files/skill-service.js'
import {
  setTrustedPreviewContext,
  clearTrustedPreviewContext,
} from '../src/main/files/session-identity.js'
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

const REGISTRY = path.resolve(process.cwd(), '../../packs/science/skills/registry.json')
const ECOSYSTEM_CATALOG = path.resolve(
  process.cwd(),
  '../../packs/science/skills/ecosystem/scp-catalog.json',
)
const BIOMNI_CATALOG = path.resolve(
  process.cwd(),
  '../../packs/science/skills/ecosystem/biomni-tool-catalog.json',
)
const BIOMNI_RESOURCE_CATALOG = path.resolve(
  process.cwd(),
  '../../packs/science/skills/ecosystem/biomni-resource-catalog.json',
)

async function run() {
  // ── Pure import plan ─────────────────────────────────────────
  const empty = planSkillImport({ skillId: '', content: 'x' })
  await test('import rejects empty id', () => ok('ok' in empty && empty.ok === false))

  const shell = planSkillImport({
    skillId: 'evil',
    content: 'run os.system("rm -rf /")',
    fileLicense: 'MIT',
  })
  await test('import rejects shell patterns', () => ok('ok' in shell && shell.ok === false))

  const gpl = planSkillImport({
    skillId: 'gpl-skill',
    content: '# safe skill body',
    fileLicense: 'GPL-3.0',
  })
  await test('import rejects GPL', () => ok('ok' in gpl && gpl.ok === false))

  const good = planSkillImport({
    skillId: 'science/demo-skill',
    content: '# Demo\nUse for charts only.\n',
    fileLicense: 'Apache-2.0',
    sourceRepository: 'https://github.com/exergyleizhou-ux/lumen-science',
    exactCommit: 'abc123',
    sourcePath: 'skills/demo/SKILL.md',
  })
  await test('import plans quarantine disposition', () => {
    ok(!('ok' in good))
    const r = good as {
      disposition: string
      contentHash: string
      ds43: { complete: boolean }
    }
    strictEqual(r.disposition, 'quarantined')
    strictEqual(r.contentHash, hashSkillContent('# Demo\nUse for charts only.\n'))
    ok(!r.ds43.complete)
    ok(!r.ds43.promptInjectionPass)
  })

  // ── Admit gates ──────────────────────────────────────────────
  const rec = good as {
    skillId: string
    displayName: string
    contentHash: string
    fileLicense: string
    sourceRepository: string
    exactCommit: string
    sourcePath: string
    disposition: 'quarantined'
    quarantinedAt: number
    ds43: {
      hasSourceRepo: boolean
      hasExactCommit: boolean
      hasSourcePath: boolean
      hasSha256: boolean
      hasLicense: boolean
      promptInjectionPass: boolean
      runtimePermissionsReviewed: boolean
      complete: boolean
    }
  }

  const noExplicit = planSkillAdmit(rec, {
    skillId: rec.skillId,
    reviewer: 'human',
    promptInjectionPass: true,
    runtimePermissionsReviewed: true,
    explicitApprove: false,
  })
  await test('admit requires explicitApprove', () => ok(!noExplicit.ok))

  const noInject = planSkillAdmit(rec, {
    skillId: rec.skillId,
    reviewer: 'human',
    promptInjectionPass: false,
    runtimePermissionsReviewed: true,
    explicitApprove: true,
  })
  await test('admit requires injection pass', () => ok(!noInject.ok))

  const admitted = planSkillAdmit(rec, {
    skillId: rec.skillId,
    reviewer: 'human',
    promptInjectionPass: true,
    runtimePermissionsReviewed: true,
    explicitApprove: true,
  })
  await test('admit succeeds with DS-43 + explicit', () => {
    ok(admitted.ok)
    strictEqual(admitted.record?.disposition, 'approved')
    ok(admitted.record?.ds43.complete)
  })

  // ── Bulk deny ────────────────────────────────────────────────
  const bulk = rejectBulkAutoApprove(['a', 'b', 'c'])
  await test('bulk auto-approve denied', () => {
    ok(!bulk.ok)
    ok((bulk.reason ?? '').includes('bulk'))
  })

  // ── Service + real registry ──────────────────────────────────
  const svc = createSkillService({
    registryPath: REGISTRY,
    ecosystemCatalogPaths: [
      ECOSYSTEM_CATALOG,
      BIOMNI_CATALOG,
      BIOMNI_RESOURCE_CATALOG,
    ],
  })
  clearTrustedPreviewContext()
  const invNoSession = svc.listInventory()
  await test('inventory reads registry without session', () => {
    strictEqual(invNoSession.ok, true)
    ok(invNoSession.summary.approved >= 10)
    strictEqual(invNoSession.summary.approved, 10)
    strictEqual(invNoSession.summary.pending, 17)
  })
  await test('inventory exposes all 704 ecosystem candidates as quarantine-only', () => {
    strictEqual(invNoSession.ecosystem.summary.total, 704)
    strictEqual(invNoSession.ecosystem.summary.approved, 0)
    strictEqual(invNoSession.ecosystem.summary.quarantined, 704)
    strictEqual(invNoSession.summary.ecosystemQuarantined, 704)
    strictEqual(invNoSession.ecosystem.candidates.length, 704)
    strictEqual(invNoSession.ecosystem.authority, 'catalog-only; Rust SessionActor required')
    ok(
      invNoSession.ecosystem.candidates.every(
        (candidate) => candidate.disposition === 'quarantined',
      ),
    )
  })
  await test('ecosystem candidates expose metadata, never executable permissions', () => {
    const candidate = invNoSession.ecosystem.candidates[0]
    ok(candidate.skillId.startsWith('ecosystem/scp/'))
    ok(candidate.sourceRepository === 'https://github.com/InternScience/scp.git')
    ok(/^[0-9a-f]{40}$/.test(candidate.exactCommit))
    ok(/^[0-9a-f]{64}$/.test(candidate.sourceSha256))
    ok(!Object.hasOwn(candidate, 'runtimePermissions'))
    ok(!Object.hasOwn(candidate, 'requiredUpstreamTools'))
    ok(!Object.hasOwn(candidate, 'sourceEndpointsNotAdmitted'))
  })
  await test('Biomni descriptors expose typed discovery metadata, not execution', () => {
    const biomni = invNoSession.ecosystem.candidates.filter(
      (candidate) => candidate.sourceKind === 'tool-descriptor',
    )
    strictEqual(biomni.length, 224)
    strictEqual(
      invNoSession.ecosystem.candidates.filter(
        (candidate) => candidate.sourceKind === 'skill-document',
      ).length,
      207,
    )
    const uniprot = biomni.find((candidate) => candidate.displayName === 'query_uniprot')
    ok(uniprot)
    ok(uniprot.parameterCount > 0)
    ok(uniprot.riskFlags.includes('caller-supplied-network-target'))
    ok(uniprot.candidateLumenRoutes.includes('x.ai/science/connector_fetch:uniprot'))
    strictEqual(uniprot.admissionTrack, 'map-to-existing-lumen-connector')
    ok(!Object.hasOwn(uniprot, 'parameterContract'))
  })
  await test('Biomni data, software, protocol, and knowledge resources stay metadata-only', () => {
    const byKind = (kind: string) =>
      invNoSession.ecosystem.candidates.filter(
        (candidate) => candidate.sourceKind === kind,
      )
    strictEqual(byKind('data-resource').length, 76)
    strictEqual(byKind('software-resource').length, 113)
    strictEqual(byKind('protocol-reference').length, 82)
    strictEqual(byKind('knowledge-document').length, 2)
    ok(
      byKind('protocol-reference').every((candidate) =>
        candidate.riskFlags.includes('content-not-vendored'),
      ),
    )
    ok(
      byKind('software-resource').every(
        (candidate) =>
          candidate.admissionTrack ===
          'dependency-identity-license-and-sandbox-review',
      ),
    )
  })

  const tamperDir = fs.mkdtempSync(path.join(os.tmpdir(), 'lumen-scp-catalog-'))
  const tamperedCatalog = path.join(tamperDir, 'scp-catalog.json')
  const tampered = JSON.parse(fs.readFileSync(ECOSYSTEM_CATALOG, 'utf-8')) as {
    summary: { approved: number; quarantined: number }
    skills: Array<{ final_disposition: string }>
  }
  tampered.summary.approved = 1
  tampered.summary.quarantined = 206
  tampered.skills[0].final_disposition = 'approved'
  fs.writeFileSync(tamperedCatalog, `${JSON.stringify(tampered, null, 2)}\n`)
  const tamperedInventory = createSkillService({
    registryPath: REGISTRY,
    ecosystemCatalogPath: tamperedCatalog,
  }).listInventory()
  fs.rmSync(tamperDir, { recursive: true, force: true })
  await test('catalog approval tamper fails closed with no candidates', () => {
    strictEqual(tamperedInventory.ok, false)
    strictEqual(tamperedInventory.ecosystem.candidates.length, 0)
    strictEqual(tamperedInventory.summary.ecosystemQuarantined, 0)
    ok(tamperedInventory.reason?.includes('authority or summary invariant failed'))
  })

  const impNoSess = svc.import({
    skillId: 'science/test-import',
    content: 'body',
    fileLicense: 'MIT',
    sourceRepository: 'https://example.com/r',
    exactCommit: 'c1',
    sourcePath: 'x.md',
  })
  await test('import requires session', () => {
    ok((impNoSess as { ok?: boolean }).ok === false)
  })

  setTrustedPreviewContext({ ownerId: 'o1', projectId: 'p1' })
  const imp = svc.import({
    skillId: 'science/test-import',
    content: '# ok skill',
    fileLicense: 'MIT',
    sourceRepository: 'https://example.com/r',
    exactCommit: 'c1',
    sourcePath: 'x.md',
  })
  await test('import to quarantine', () => {
    ok((imp as { ok?: boolean }).ok)
    strictEqual((imp as { disposition?: string }).disposition, 'quarantined')
  })

  // OS skills must not be in approved set
  await test('OS alphafold2 not approved', () => {
    ok(!invNoSession.approved.includes('alphafold2'))
    ok(!invNoSession.approved.includes('science/alphafold2'))
  })

  const bulkSvc = svc.bulkAdmit(['s1', 's2'])
  await test('service bulkAdmit denied', () => {
    ok((bulkSvc as { ok?: boolean }).ok === false)
  })

  // ── IPC ──────────────────────────────────────────────────────
  for (const ch of ['skills:list', 'skills:quarantine-list', 'skills:bulk-admit']) {
    await test(`policy allows ${ch}`, () => ok(validateIpcChannel(ch)))
  }
  for (const ch of ['skills:import', 'skills:admit', 'skills:reject']) {
    await test(`policy rejects legacy local mutation ${ch}`, () => ok(!validateIpcChannel(ch)))
  }

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
    skillService: svc,
    skillsRegistryPath: REGISTRY,
    skillsEcosystemCatalogPaths: [
      ECOSYSTEM_CATALOG,
      BIOMNI_CATALOG,
      BIOMNI_RESOURCE_CATALOG,
    ],
  })
  await test('ipc registers skills channels', () => {
    ok(handlers.has('skills:list'))
    ok(handlers.has('skills:bulk-admit'))
    ok(!handlers.has('skills:import'))
    ok(!handlers.has('skills:admit'))
    ok(!handlers.has('skills:reject'))
  })

  const listH = handlers.get('skills:list')!
  const listed = await listH({})
  await test('ipc list inventory', () => {
    strictEqual(listed.summary.approved, 10)
    strictEqual(listed.ecosystem.summary.total, 704)
    strictEqual(listed.ecosystem.summary.approved, 0)
  })

  const bulkH = handlers.get('skills:bulk-admit')!
  const bulkRes = await bulkH({}, { skillIds: ['x', 'y'] })
  await test('ipc bulk admit denied', () => {
    ok(bulkRes.ok === false)
  })

  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

run()
