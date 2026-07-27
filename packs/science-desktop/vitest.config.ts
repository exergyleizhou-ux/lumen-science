/**
 * Unit-test configuration — the file whose absence hid 4,800 tests.
 *
 * This pack carried 365 vitest files from upstream and NO config and NO script
 * that ran them. `test:authority` runs tsx scripts, `test:e2e` runs Playwright;
 * the vitest suites ran only if someone typed `npx vitest` by hand, which
 * nobody did — so 597 of them were failing silently, some against behaviour
 * this fork deliberately changed months ago.
 *
 * ## What is excluded, and by what authority
 *
 * The production build already declares which sources are NOT part of this
 * product: the per-file exclusion list in tsconfig.node.json, enforced on
 * every CI run by `desktop-source-graph.mts --check`. Tests whose subject is
 * on that list are excluded HERE BY DERIVATION from the same list — one
 * authority, not a second hand-maintained copy that drifts.
 *
 * The explicit list below covers tests the derivation cannot map: integration
 * tests spanning several excluded modules, and tests that import packages this
 * build removed outright (@prisma/client, @agentclientprotocol/*,
 * @bokuweb/zstd-wasm). Each entry says why.
 *
 * Nothing else is excluded. A test of adopted code that fails is a failure.
 */
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { defineConfig } from 'vitest/config'

/** Test globs derived from the production exclusion list. */
function excludedSubjectTests(): string[] {
  const raw = readFileSync(resolve(__dirname, 'tsconfig.node.json'), 'utf-8')
  const json = JSON.parse(raw.replace(/\/\/[^\n]*/g, '')) as { exclude?: string[] }
  return (json.exclude ?? [])
    .filter((e) => e.endsWith('.ts') && !e.startsWith('**'))
    .flatMap((source) => {
      const base = source.replace(/\.ts$/, '')
      // X.test.ts plus dotted variants (X.integration.test.ts, X.startup.test.ts …)
      return [`${base}.test.ts`, `${base}.*.test.ts`]
    })
}

export default defineConfig({
  resolve: {
    alias: {
      // tsconfig.web.json's paths, which vitest does not read on its own —
      // without these, every renderer test importing '@/…' fails to resolve.
      '@': resolve(__dirname, 'src/renderer/src'),
      '@renderer': resolve(__dirname, 'src/renderer/src'),
      // The spreadsheet cache-policy tests drive a FAKE table (adopted with
      // the tests; upstream test/fixtures/fake-e-virt-table.ts) so they can
      // steer scroll state deterministically. What is under test is the
      // window cache in @file-viewer/renderer-spreadsheet — which is the
      // patch-package patch, not the table.
      'e-virt-table/dist/index.es.js': resolve(__dirname, 'test/fixtures/fake-e-virt-table.ts'),
    },
  },
  esbuild: {
    // The automatic JSX runtime, matching the app build. Classic transform
    // leaves `React is not defined` in any TSX test that does not import it.
    jsx: 'automatic',
  },
  test: {
    setupFiles: ['./vitest.setup.ts'],
    server: {
      deps: {
        // e-virt-table ships an ES module inside a CommonJS package; vitest's
        // externalized require of it dies. Inlining makes vitest transform it
        // like project code. (The app's own build is unaffected — Vite always
        // bundles it.)
        // The wrapper ships ESM in a CommonJS package; inlining lets vitest
        // transform it (and honour the fake-table alias inside it).
        inline: ['@file-viewer/renderer-spreadsheet'],
      },
    },
    exclude: [
      '**/node_modules/**',
      '**/dist/**',
      '**/out/**',
      // Playwright specs. Vitest collects them, cannot run them, and reports
      // the resulting crash as a test failure.
      'e2e/**',

      ...excludedSubjectTests(),

      // ── Tests of subsystems this build removed ─────────────────────────
      // The agent-framework layer is stubbed: no Claude Code / Codex backend
      // is admitted as a peer authority, and its packages are uninstalled.
      'src/main/acp/agent-process.test.ts',
      'src/main/reviewer/fix-loop.test.ts',
      'src/main/reviewer/lifecycle.test.ts',
      'src/main/settings/responses-bridge.integration.test.ts',
      'src/main/settings/service.test.ts',
      // Prisma-backed persistence was not adopted (project state lives in the
      // Rust SessionActor; @prisma/client is excluded from the package too).
      'src/main/compute/prisma-client.test.ts',
      'src/main/compute/compute-jobs.integration.test.ts',
      'src/main/compute/concurrency-integration.test.ts',
      'src/main/session-persistence/deletion-integration.test.ts',
      'src/main/reviewer/log-capture.test.ts',
      'src/main/reviewer/orchestrator-prompt-prefix.test.ts',
      // Generates and diffs an API map by spawning the repo's own build
      // tooling; meaningful in upstream's monorepo layout, not here.
      'src/renderer/web/api-map.generated.test.ts',
    ],
  },
})
