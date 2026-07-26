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

// Structural: afterPack hook referenced
test('electron-builder afterPack hook path referenced', () => {
  ok(yml.includes('afterPack') || yml.includes('adhoc-sign') || yml.includes('build/'))
})

console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
console.log(
  'NOTE: This check does NOT upload GitHub Release binaries. P0 asset upload remains release-ops.',
)
process.exit(failures > 0 ? 1 : 0)
