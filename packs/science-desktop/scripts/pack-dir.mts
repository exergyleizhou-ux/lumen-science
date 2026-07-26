#!/usr/bin/env npx tsx
/**
 * Honest pack-dir proof for Lumen Science Desktop (1.1.0-dev).
 *
 * Builds a minimal main/preload/renderer into `out/`, then runs
 * `electron-builder --dir`. Does NOT claim notarization, auto-update, or GA.
 *
 * Run: npx tsx scripts/pack-dir.mts
 * Or:  npm run dist
 */
import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { createRequire } from 'node:module'
import { build as esbuild } from 'esbuild'

const root = process.cwd()
const outMain = path.join(root, 'out/main')
const outPreload = path.join(root, 'out/preload')
const outRenderer = path.join(root, 'out/renderer')

function fail(msg: string): never {
  console.error(`FAIL pack-dir: ${msg}`)
  process.exit(1)
}

async function main() {
  // Prefer local esbuild from electron-vite / vite graph; fall back to require
  let build = esbuild
  try {
    // already imported
  } catch {
    const require = createRequire(import.meta.url)
    build = require('esbuild').build
  }

  fs.rmSync(path.join(root, 'out'), { recursive: true, force: true })
  fs.mkdirSync(outMain, { recursive: true })
  fs.mkdirSync(outPreload, { recursive: true })
  fs.mkdirSync(outRenderer, { recursive: true })

  console.log('BUILD main (pack-main.ts)')
  await build({
    entryPoints: [path.join(root, 'src/main/pack-main.ts')],
    outfile: path.join(outMain, 'index.js'),
    bundle: true,
    platform: 'node',
    format: 'cjs',
    target: 'node20',
    external: ['electron'],
    sourcemap: true,
  })

  console.log('BUILD preload (pack-preload.ts)')
  await build({
    entryPoints: [path.join(root, 'src/preload/pack-preload.ts')],
    outfile: path.join(outPreload, 'index.js'),
    bundle: true,
    platform: 'node',
    format: 'cjs',
    target: 'node20',
    external: ['electron'],
    sourcemap: true,
  })

  console.log('COPY renderer pack shell')
  fs.copyFileSync(
    path.join(root, 'src/renderer/pack-index.html'),
    path.join(outRenderer, 'index.html'),
  )

  // package.json main must resolve for electron-builder
  for (const p of [
    path.join(outMain, 'index.js'),
    path.join(outPreload, 'index.js'),
    path.join(outRenderer, 'index.html'),
  ]) {
    if (!fs.existsSync(p)) fail(`missing build output ${p}`)
  }
  console.log('OK  out/ artifacts written')

  console.log('RUN electron-builder --dir')
  const r = spawnSync(
    path.join(root, 'node_modules/.bin/electron-builder'),
    ['--dir'],
    { cwd: root, encoding: 'utf-8', env: process.env },
  )
  process.stdout.write(r.stdout || '')
  process.stderr.write(r.stderr || '')
  if (r.status !== 0) fail(`electron-builder exited ${r.status}`)

  // Verify branding on macOS output if present
  const candidates = [
    path.join(root, 'dist/mac-arm64/Lumen Science Desktop.app/Contents/Info.plist'),
    path.join(root, 'dist/mac/Lumen Science Desktop.app/Contents/Info.plist'),
    path.join(root, 'dist/mac-x64/Lumen Science Desktop.app/Contents/Info.plist'),
  ]
  const plist = candidates.find((p) => fs.existsSync(p))
  if (plist) {
    const text = fs.readFileSync(plist, 'utf-8')
    if (!text.includes('Lumen Science Desktop')) fail('Info.plist missing product name')
    if (!text.includes('com.exergyleizhou-ux.lumen-science-desktop')) {
      fail('Info.plist missing appId')
    }
    if (text.includes('com.aipoch.open-science')) fail('Info.plist still has aipoch id')
    console.log(`OK  branded app: ${plist}`)
  } else {
    // Linux/Windows dir layouts
    const dist = path.join(root, 'dist')
    if (!fs.existsSync(dist)) fail('dist/ missing after electron-builder')
    console.log('OK  dist/ present (non-mac layout)')
  }

  console.log('ALL PACK-DIR PASSED (unsigned; notarization not claimed)')
}

main().catch((e) => {
  console.error(e)
  process.exit(1)
})
