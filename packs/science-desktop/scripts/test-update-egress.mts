#!/usr/bin/env npx tsx
/**
 * Update egress test — EXECUTES the shipped update policy (LS5-R1-02).
 *
 * This desktop was adapted from Open Science, which shipped an update feed on
 * statics.aipoch.com. Inheriting it would let a third party serve executable
 * code to Lumen users. These tests assert three separate things:
 *
 *   1. the policy is off unless Lumen-owned signing material is configured,
 *   2. it cannot be pointed at the upstream host even deliberately,
 *   3. no third-party update or runtime URL survives anywhere in shipped source.
 *
 * (3) matters because (1) and (2) only govern the code paths that ask the
 * policy. A hardcoded URL elsewhere would bypass both.
 *
 * Run: npx tsx scripts/test-update-egress.mts
 */
import { ok, strictEqual } from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

import {
  ALLOWED_UPDATE_HOSTS,
  FORBIDDEN_UPDATE_HOSTS,
  requireUpdateFeedUrl,
  resolveUpdatePolicy
} from '../src/shared/update-policy.js'
import { APP } from '../src/shared/app-config.js'

let failures = 0

function test(name: string, fn: () => void): void {
  try {
    fn()
    console.log(`OK  ${name}`)
  } catch (e: unknown) {
    failures++
    console.log(`FAIL ${name}: ${(e as Error).message}`)
  }
}

console.log('UPDATE-EGRESS: executing shipped update-policy + app-config')

// ── 1. off by default ────────────────────────────────────────────

test('no configuration => disabled', () => {
  const policy = resolveUpdatePolicy({})
  strictEqual(policy.enabled, false)
  ok(!policy.enabled && policy.reason.includes('no Lumen-owned update feed'), policy.enabled ? '' : policy.reason)
})

test('feed without public key => disabled (unverifiable code)', () => {
  const policy = resolveUpdatePolicy({
    LUMEN_UPDATE_FEED_URL: 'https://github.com/exergyleizhou-ux/lumen-science/releases'
  })
  strictEqual(policy.enabled, false)
})

test('public key without feed => disabled', () => {
  const policy = resolveUpdatePolicy({ LUMEN_UPDATE_PUBLIC_KEY: 'RWQf6L...' })
  strictEqual(policy.enabled, false)
})

// ── 2. cannot be aimed at the upstream host ──────────────────────

for (const host of FORBIDDEN_UPDATE_HOSTS) {
  test(`forbidden host rejected even when fully configured: ${host}`, () => {
    const policy = resolveUpdatePolicy({
      LUMEN_UPDATE_FEED_URL: `https://${host}/open-science/app/stable/version.json`,
      LUMEN_UPDATE_PUBLIC_KEY: 'RWQf6L...'
    })
    strictEqual(policy.enabled, false)
  })
}

test('subdomain of a forbidden host is also rejected', () => {
  const policy = resolveUpdatePolicy({
    LUMEN_UPDATE_FEED_URL: 'https://cdn.aipoch.com/version.json',
    LUMEN_UPDATE_PUBLIC_KEY: 'RWQf6L...'
  })
  strictEqual(policy.enabled, false)
})

test('plaintext http rejected', () => {
  const policy = resolveUpdatePolicy({
    LUMEN_UPDATE_FEED_URL: 'http://github.com/exergyleizhou-ux/lumen-science/releases',
    LUMEN_UPDATE_PUBLIC_KEY: 'RWQf6L...'
  })
  strictEqual(policy.enabled, false)
})

test('host outside the allowlist rejected', () => {
  const policy = resolveUpdatePolicy({
    LUMEN_UPDATE_FEED_URL: 'https://evil.example.com/version.json',
    LUMEN_UPDATE_PUBLIC_KEY: 'RWQf6L...'
  })
  strictEqual(policy.enabled, false)
})

test('malformed URL rejected', () => {
  const policy = resolveUpdatePolicy({
    LUMEN_UPDATE_FEED_URL: 'not-a-url',
    LUMEN_UPDATE_PUBLIC_KEY: 'RWQf6L...'
  })
  strictEqual(policy.enabled, false)
})

// Control: a correct configuration must actually work, otherwise the checks
// above would pass simply because everything is rejected.
test('Lumen-owned host with a key is accepted', () => {
  const feedUrl = 'https://github.com/exergyleizhou-ux/lumen-science/releases/latest/version.json'
  const policy = resolveUpdatePolicy({
    LUMEN_UPDATE_FEED_URL: feedUrl,
    LUMEN_UPDATE_PUBLIC_KEY: 'RWQf6L...'
  })
  strictEqual(policy.enabled, true)
  if (policy.enabled) strictEqual(policy.feedUrl, feedUrl)
})

test('allowlist is non-empty and excludes every forbidden host', () => {
  ok(ALLOWED_UPDATE_HOSTS.length > 0, 'allowlist empty')
  for (const bad of FORBIDDEN_UPDATE_HOSTS) {
    ok(!ALLOWED_UPDATE_HOSTS.includes(bad), `${bad} is both allowed and forbidden`)
  }
})

test('requireUpdateFeedUrl throws when disabled', () => {
  let threw = false
  try {
    requireUpdateFeedUrl({})
  } catch {
    threw = true
  }
  ok(threw, 'a networked strategy could be constructed with no configured feed')
})

// ── 3. no third-party URL survives in shipped source ─────────────

const HERE = path.dirname(fileURLToPath(import.meta.url))
const SRC = path.resolve(HERE, '../src')

function walk(dir: string): string[] {
  const out: string[] = []
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) out.push(...walk(full))
    else if (/\.(ts|tsx|html)$/.test(entry.name)) out.push(full)
  }
  return out
}

const sourceFiles = walk(SRC)
// Test fixtures may legitimately name the upstream host to prove it is
// rejected — this very file does. Shipped source may not.
const shipped = sourceFiles.filter((f) => !/\.test\.(ts|tsx)$/.test(f))

test('no aipoch update/CDN URL in shipped source', () => {
  const offenders: string[] = []
  for (const file of shipped) {
    const text = fs.readFileSync(file, 'utf8')
    for (const [i, line] of text.split('\n').entries()) {
      // Match the host in a URL position only, so a comment explaining why the
      // host is banned does not trip the check.
      if (/https?:\/\/[^\s'"]*aipoch\.com/.test(line)) {
        offenders.push(`${path.relative(SRC, file)}:${i + 1}`)
      }
    }
  }
  strictEqual(offenders.length, 0, `third-party URL still present: ${offenders.join(', ')}`)
})

test('app-config carries no update manifest URL to inherit', () => {
  ok(!('manifestUrl' in APP.update), 'APP.update.manifestUrl still exists')
  const configPath = path.join(SRC, 'shared/app-config.ts')
  const text = fs.readFileSync(configPath, 'utf8')
  ok(!/https?:\/\/[^\s'"]*aipoch/.test(text), 'app-config still references aipoch')
})

test('app identity is Lumen, not the upstream project', () => {
  strictEqual(APP.name, 'Lumen Science')
  strictEqual(APP.githubOwner, 'exergyleizhou-ux')
  strictEqual(APP.githubRepo, 'lumen-science')
  ok(!APP.copyright.includes('AIPOCH'), `copyright still upstream: ${APP.copyright}`)
})

test('update IPC does not construct a strategy at import time', () => {
  // registerUpdateIpcHandlers must consult policy before create-strategy runs,
  // because ElectronUpdaterStrategy binds electron-updater's autoUpdater in its
  // constructor. Assert the ordering is expressed in the source.
  const ipcPath = path.join(SRC, 'main/update/ipc.ts')
  const text = fs.readFileSync(ipcPath, 'utf8')
  ok(
    !/strategy: UpdateStrategy = createUpdateStrategy\(\)/.test(text),
    'createUpdateStrategy() is still a default parameter — it runs before the policy is read'
  )
  ok(text.includes('resolveUpdatePolicy'), 'ipc.ts does not consult the update policy')
})

console.log(failures === 0 ? `\nALL UPDATE-EGRESS PASSED` : `\n${failures} FAILED`)
process.exit(failures === 0 ? 0 : 1)
