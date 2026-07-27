#!/usr/bin/env npx tsx
/**
 * Desktop production source graph (LS5-D1-01).
 *
 * This pack was absorbed from Open Science and kept far more source than it
 * runs: 568 source files, of which only a minority are reachable from the
 * production entry points. The rest still typecheck, still get linted, still
 * appear in searches, and still carry ~1400 type errors that made a real
 * typecheck impossible — which is why desktop-ci ran it with
 * `continue-on-error: true`.
 *
 * Rather than hand-maintaining a list of dead directories, compute the graph:
 * walk imports from the real entry points and classify every file as
 *
 *   adopted           reachable from a production entry
 *   dead-upstream     unreachable; inherited from Open Science
 *
 * The output drives tsconfig exclusion and gives CI something enforceable: a
 * file cannot quietly become reachable, and an adopted file cannot quietly
 * start importing a dead one.
 *
 *   npx tsx scripts/desktop-source-graph.mts            # human summary
 *   npx tsx scripts/desktop-source-graph.mts --json     # machine report
 *   npx tsx scripts/desktop-source-graph.mts --check    # CI gate
 */
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = path.dirname(fileURLToPath(import.meta.url))
const PACK = path.resolve(HERE, '..')
const SRC = path.join(PACK, 'src')

// Production entry points. `index.ts` is what package.json `main` resolves to
// after electron-vite builds; the renderer entry is referenced by index.html.
const ENTRIES = [
  'src/main/index.ts',
  'src/preload/index.ts',
  'src/renderer/src/main.tsx'
]

const SOURCE_RE = /\.(ts|tsx)$/
const TEST_RE = /\.(test|spec)\.(ts|tsx)$/
// Ambient declaration files are PROGRAM INPUTS, not nodes in the import graph: nothing imports
// them, they contribute `declare module` / `declare global` blocks to whatever program includes
// them. Reachability is therefore meaningless for them, and excluding one removes real typing from
// the build. This cost us three of them — src/renderer/src/env.d.ts (the vite/client reference,
// whose absence turned every `import icon from './x.svg'` into TS2307), src/main/skills/js-yaml.d.ts
// and src/preload/index.d.ts — all swept into `exclude` by an earlier --write-tsconfig run.
const DECLARATION_RE = /\.d\.ts$/

const aliases: Array<[RegExp, string]> = [
  [/^@\//, 'src/renderer/src/'],
  [/^@renderer\//, 'src/renderer/src/']
]

function resolveImport(fromFile: string, spec: string): string | null {
  let rel: string | null = null

  for (const [pattern, replacement] of aliases) {
    if (pattern.test(spec)) {
      rel = path.join(PACK, spec.replace(pattern, replacement))
      break
    }
  }
  if (rel === null) {
    // Bare specifiers are npm packages — not part of our source graph.
    if (!spec.startsWith('.')) return null
    rel = path.resolve(path.dirname(fromFile), spec)
  }

  // electron-vite source imports frequently carry a .js extension that refers
  // to the .ts file (NodeNext style). Try the TypeScript forms first.
  const candidates = [
    rel,
    rel.replace(/\.js$/, '.ts'),
    rel.replace(/\.js$/, '.tsx'),
    `${rel}.ts`,
    `${rel}.tsx`,
    path.join(rel, 'index.ts'),
    path.join(rel, 'index.tsx')
  ]
  for (const candidate of candidates) {
    if (SOURCE_RE.test(candidate) && fs.existsSync(candidate) && fs.statSync(candidate).isFile()) {
      return candidate
    }
  }
  return null
}

const IMPORT_RE =
  /(?:^|\n)\s*(?:import|export)\s[^'"]*?from\s*['"]([^'"]+)['"]|(?:^|\n)\s*import\s*['"]([^'"]+)['"]|\bimport\(\s*['"]([^'"]+)['"]\s*\)/g

function importsOf(file: string): string[] {
  const text = fs.readFileSync(file, 'utf8')
  const specs: string[] = []
  for (const m of text.matchAll(IMPORT_RE)) {
    const spec = m[1] ?? m[2] ?? m[3]
    if (spec) specs.push(spec)
  }
  return specs
}

function walkSource(dir: string): string[] {
  const out: string[] = []
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) out.push(...walkSource(full))
    else if (SOURCE_RE.test(entry.name)) out.push(full)
  }
  return out
}

// ── build the graph ──────────────────────────────────────────────

const reachable = new Set<string>()
const queue: string[] = []
const missing: Array<{ from: string; spec: string }> = []

for (const entry of ENTRIES) {
  const full = path.join(PACK, entry)
  if (!fs.existsSync(full)) {
    console.error(`FAIL: entry point missing: ${entry}`)
    process.exit(2)
  }
  reachable.add(full)
  queue.push(full)
}

while (queue.length > 0) {
  const file = queue.pop()!
  for (const spec of importsOf(file)) {
    const resolved = resolveImport(file, spec)
    if (resolved === null) {
      // Only unresolved *source* imports matter. Vite resolves asset imports
      // (.css/.svg/.png/...) through its own pipeline; they are not part of
      // the TypeScript graph and their absence here is expected.
      // Strip Vite's query suffix (`icon.png?asset`, `worker.ts?worker`) before
      // testing the extension.
      const bare = spec.split('?')[0]
      const isAsset = /\.(css|scss|svg|png|jpe?g|gif|webp|woff2?|json)$/.test(bare)
      if (!isAsset && (spec.startsWith('.') || /^@(\/|renderer\/)/.test(spec))) {
        missing.push({ from: path.relative(PACK, file), spec })
      }
      continue
    }
    if (!reachable.has(resolved)) {
      reachable.add(resolved)
      queue.push(resolved)
    }
  }
}

const allSource = walkSource(SRC)
const rel = (f: string): string => path.relative(PACK, f)

const adopted = allSource.filter((f) => reachable.has(f)).map(rel).sort()
const unreachable = allSource.filter((f) => !reachable.has(f)).map(rel).sort()
const unreachableNonTest = unreachable.filter((f) => !TEST_RE.test(f))

// Directories in which NOTHING is reachable. These are what tsconfig excludes:
// per-file exclusion of ~370 paths would be unreadable and would rot.
function fullyDeadDirs(): string[] {
  const byDir = new Map<string, { total: number; dead: number }>()
  for (const f of allSource) {
    const dir = path.dirname(rel(f))
    const stat = byDir.get(dir) ?? { total: 0, dead: 0 }
    stat.total += 1
    if (!reachable.has(f)) stat.dead += 1
    byDir.set(dir, stat)
  }
  const dead = [...byDir.entries()]
    .filter(([, s]) => s.total === s.dead)
    .map(([d]) => d)
    .sort()
  // Collapse children into a dead parent so the list stays short.
  return dead.filter((d) => !dead.some((other) => other !== d && d.startsWith(`${other}/`)))
}

const deadDirs = fullyDeadDirs()

const report = {
  entries: ENTRIES,
  totals: {
    sourceFiles: allSource.length,
    adopted: adopted.length,
    unreachable: unreachable.length,
    unreachableNonTest: unreachableNonTest.length,
    // Reported separately so the "unreachable" figure is not read as "dead": these are always kept.
    ambientDeclarations: allSource.filter((f) => DECLARATION_RE.test(rel(f))).length
  },
  deadDirectories: deadDirs,
  unresolvedRelativeImports: missing,
  adoptedFiles: adopted
}

const args = process.argv.slice(2)

if (args.includes('--json')) {
  console.log(JSON.stringify(report, null, 2))
  process.exit(0)
}

/**
 * tsconfig exclusions, derived from the graph.
 *
 * Directory-level exclusion is not sufficient: an unreachable file that the
 * include glob still matches (e.g. session-persistence/coordinator.ts) pulls
 * its own unreachable imports into the program, so errors reappear from
 * directories that were supposedly excluded. Only excluding the unreachable
 * set itself is airtight.
 *
 * Excluding unreachable files is safe precisely because nothing reachable
 * imports them — and an adopted file living inside an excluded directory is
 * still typechecked, because it enters the program through its importer
 * rather than through the include glob.
 */
function buildExclusions(scope: 'node' | 'web'): string[] {
  const prefixes =
    scope === 'node'
      ? ['src/main/', 'src/preload/', 'src/shared/']
      : ['src/renderer/', 'src/shared/']
  const files = unreachable
    .filter((f) => prefixes.some((p) => f.startsWith(p)))
    .filter((f) => !TEST_RE.test(f))
    // Never exclude an ambient declaration: it is unreachable BY CONSTRUCTION (see DECLARATION_RE),
    // so "unreachable" carries no signal for it, and dropping one silently removes typing.
    .filter((f) => !DECLARATION_RE.test(f))
  // Tests are covered by a glob rather than 400+ literal paths.
  return ['**/*.test.ts', '**/*.test.tsx', '**/*.spec.ts', '**/*.spec.tsx', ...files.sort()]
}

if (args.includes('--write-tsconfig')) {
  for (const scope of ['node', 'web'] as const) {
    const file = path.join(PACK, `tsconfig.${scope}.json`)
    const cfg = JSON.parse(fs.readFileSync(file, 'utf8')) as Record<string, unknown>
    cfg.exclude = buildExclusions(scope)
    fs.writeFileSync(file, `${JSON.stringify(cfg, null, 2)}\n`)
    console.log(`wrote tsconfig.${scope}.json exclude (${(cfg.exclude as string[]).length} entries)`)
  }
  process.exit(0)
}

console.log('desktop-source-graph')
console.log(`  entries            ${ENTRIES.length}`)
console.log(`  source files       ${allSource.length}`)
console.log(`  adopted            ${adopted.length}`)
console.log(`  unreachable        ${unreachable.length} (${unreachableNonTest.length} non-test)`)
console.log(`  fully dead dirs    ${deadDirs.length}`)
for (const d of deadDirs) console.log(`      ${d}`)
if (missing.length > 0) {
  console.log(`  unresolved imports ${missing.length}`)
  for (const m of missing.slice(0, 10)) console.log(`      ${m.from} -> ${m.spec}`)
}

if (args.includes('--check')) {
  let failures = 0

  // An unresolved relative import from reachable code means the graph is wrong
  // or the tree is broken — either way the exclusion list cannot be trusted.
  if (missing.length > 0) {
    console.error(`\nFAIL: ${missing.length} unresolved relative import(s) from reachable code`)
    failures += 1
  }

  // Every directory tsconfig excludes must still be fully dead. If a file in
  // one becomes reachable, exclusion would hide a real type error.
  const tsconfigPath = path.join(PACK, 'tsconfig.node.json')
  const tsconfig = JSON.parse(
    fs.readFileSync(tsconfigPath, 'utf8').replace(/^\s*\/\/.*$/gm, '')
  ) as { exclude?: string[] }
  for (const pattern of tsconfig.exclude ?? []) {
    const base = pattern.replace(/\/\*\*\/\*$/, '').replace(/\/\*\*$/, '')
    if (!base.startsWith('src/')) continue
    const live = adopted.filter((f) => f === base || f.startsWith(`${base}/`))
    if (live.length > 0) {
      console.error(
        `\nFAIL: tsconfig excludes ${pattern} but these files are reachable:\n  ${live.join('\n  ')}`
      )
      failures += 1
    }
  }

  if (failures > 0) process.exit(1)
  console.log('\nOK: source graph consistent with tsconfig exclusions')
}
