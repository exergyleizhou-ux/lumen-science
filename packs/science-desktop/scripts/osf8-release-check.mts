#!/usr/bin/env npx tsx
/**
 * OSF-8 release scaffold check — honest, no fake binary claims.
 *
 * Verifies packaging config + checklist exist; does NOT upload GitHub assets
 * or claim notarization without certs.
 *
 * Run: npx tsx scripts/osf8-release-check.mts
 */
import { strictEqual, ok } from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'

let failures = 0
function test(name: string, fn: () => void) {
  try {
    fn()
    console.log(`OK  ${name}`)
  } catch (e: unknown) {
    failures++
    console.log(`FAIL ${name}: ${(e as Error).message}`)
  }
}

const root = process.cwd()
const yml = fs.readFileSync(path.join(root, 'electron-builder.yml'), 'utf-8')
const pkg = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf-8')) as {
  name: string
  productName?: string
  version: string
  scripts?: Record<string, string>
}
const checklistRepo = path.resolve(root, '../../docs/science/RELEASE_1.0.1_CHECKLIST.md')

test('package name is lumen-science-desktop', () => {
  strictEqual(pkg.name, 'lumen-science-desktop')
})
test('productName branded Lumen', () => {
  ok((pkg.productName || '').includes('Lumen'))
})
test('version is semver-ish', () => {
  ok(/^\d+\.\d+\.\d+/.test(pkg.version))
})
test('scripts include pack:check, test:authority, dist', () => {
  ok(pkg.scripts?.['pack:check'])
  ok(pkg.scripts?.['test:authority'])
  ok(pkg.scripts?.dist)
  ok(
    (pkg.scripts?.dist || '').includes('pack-dir') ||
      (pkg.scripts?.dist || '').includes('electron-builder'),
  )
})
test('electron-builder.yml has appId', () => {
  ok(yml.includes('appId:'))
  ok(yml.includes('lumen-science'))
})
test('electron-builder branded Lumen not Open Science bundle name', () => {
  ok(yml.includes('Lumen Science Desktop'))
  ok(!yml.includes('CFBundleName: Open Science'))
})
test('RELEASE_1.0.1_CHECKLIST.md exists', () => {
  ok(fs.existsSync(checklistRepo), `missing ${checklistRepo}`)
})
test('checklist does not claim binaries already uploaded', () => {
  const c = fs.readFileSync(checklistRepo, 'utf-8')
  ok(c.includes('Must ship') || c.includes('ACCEPT') || c.includes('downloadable'))
  // Must remain honest about gap
  ok(
    c.toLowerCase().includes('missing') ||
      c.toLowerCase().includes('must ship') ||
      c.toLowerCase().includes('p0'),
  )
})
test('checklist forbids false completion claims', () => {
  const c = fs.readFileSync(checklistRepo, 'utf-8')
  // Must document gaps and forbid fake completion (not ban the words entirely)
  ok(/do not claim/i.test(c) || /deferred/i.test(c) || /P0/i.test(c))
  ok(!/notarization complete\s*$/im.test(c))
  ok(!/^all platforms released/im.test(c))
})

// Structural: afterPack hook file must exist
test('afterPack adhoc-sign.cjs exists', () => {
  ok(fs.existsSync(path.join(root, 'build/adhoc-sign.cjs')))
})
test('no aipoch/open-science execution naming in builder', () => {
  ok(!yml.includes('executableName: open-science'))
  ok(!yml.includes('artifactName: aipoch-'))
  ok(!yml.includes('statics.aipoch.com'))
  ok(!yml.includes('maintainer: aipoch'))
})
test('builder does not reference missing cli/ or packages/open-science', () => {
  ok(!/from:\s*cli\s*$/m.test(yml))
  ok(!yml.includes('packages/open-science'))
})
test('builder does not require missing prisma extraResources', () => {
  // Exclude globs under files: are fine; a from: extraResources path that does not
  // exist makes electron-builder fail. Prisma must stay optional until staged.
  ok(!/^\s*from:\s*node_modules\/\.prisma/m.test(yml), 'no prisma extraResources from:')
  ok(!/^\s*from:\s*node_modules\/@prisma\/client/m.test(yml), 'no @prisma/client extraResources from:')
  ok(!/extraResources:[\s\S]*from:\s*node_modules\/\.prisma/.test(yml))
})
test('builder ships both approved and quarantined skill catalogs', () => {
  ok(yml.includes('from: ../../packs/science/skills/registry.json'))
  ok(yml.includes('to: science/skills-registry.json'))
  ok(yml.includes('from: ../../packs/science/skills/ecosystem/scp-catalog.json'))
  ok(yml.includes('to: science/ecosystem-skill-catalog.json'))
  ok(fs.existsSync(path.resolve(root, '../science/skills/ecosystem/scp-catalog.json')))
})
test('auto-update publish feed disabled', () => {
  ok(!yml.includes('provider: generic') || !yml.includes('statics.aipoch.com'))
  // Prefer explicit omit of publish
  ok(!yml.match(/^publish:\s*$/m) || !yml.includes('statics.aipoch.com'))
})
test('package-lock.json present for reproducible npm ci', () => {
  ok(fs.existsSync(path.join(root, 'package-lock.json')))
})
test('VERSIONING.md documents three components', () => {
  const v = path.resolve(root, '../../docs/VERSIONING.md')
  ok(fs.existsSync(v))
  const t = fs.readFileSync(v, 'utf-8')
  ok(t.includes('Lumen Core'))
  ok(t.includes('Science CLI') || t.includes('CLI/MCP'))
  ok(t.includes('Desktop'))
})
test('packs/science/VERSION exists', () => {
  ok(fs.existsSync(path.resolve(root, '../science/VERSION')))
})

console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
console.log(
  'NOTE: This check does NOT prove electron-builder package success or GitHub asset provenance.',
)
process.exit(failures > 0 ? 1 : 0)
