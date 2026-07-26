import { createRequire } from 'node:module'
import { resolve } from 'path'
import { defineConfig } from 'electron-vite'
// PluginOption is Vite's type, re-exported by nothing in electron-vite; importing it from
// 'electron-vite' never resolved. electron-vite passes plugin arrays straight through to Vite, so
// this is the same type the config actually needs.
import type { PluginOption } from 'vite'
import react from '@vitejs/plugin-react'

const require = createRequire(import.meta.url)

/**
 * Optional plugins from the Open Science absorb. Missing packages must not
 * block an honest Lumen pack proof (`electron-builder --dir`).
 */
function optionalPlugins(): PluginOption[] {
  const plugins: PluginOption[] = []

  try {
    // Spreadsheet worker interop (optional monorepo package).
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { fileViewerRenderers } = require('@file-viewer/vite-plugin') as {
      fileViewerRenderers: (opts: Record<string, unknown>) => PluginOption
    }
    plugins.push(
      fileViewerRenderers({
        formats: ['xls', 'xlsx'],
        inject: false,
        chunkStrategy: 'none',
      }),
    )
  } catch {
    // Pack / CI without file-viewer: skip
  }

  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const tailwindcss = require('@tailwindcss/vite').default as () => PluginOption
    plugins.push(tailwindcss())
  } catch {
    // Tailwind v4 vite plugin optional when not installed
  }

  plugins.push(react())
  return plugins
}

export default defineConfig({
  main: {},
  preload: {
    build: {
      rollupOptions: {
        input: {
          index: resolve('src/preload/index.ts'),
        },
      },
    },
  },
  renderer: {
    // Regenerate lazy optimized chunks so a persisted Electron page cannot request stale hashes.
    optimizeDeps: { force: true },
    resolve: {
      alias: {
        '@': resolve('src/renderer/src'),
        '@renderer': resolve('src/renderer/src'),
      },
    },
    server: {
      // Don't watch git worktrees under .claude/worktrees — full source copies would trigger
      // needless rescans/HMR churn during dev.
      watch: { ignored: ['**/.claude/**'] },
    },
    plugins: optionalPlugins(),
    build: {
      rollupOptions: {
        input: {
          index: resolve('src/renderer/index.html'),
          'office-preview': resolve('src/renderer/office-preview.html'),
        },
      },
    },
  },
})
