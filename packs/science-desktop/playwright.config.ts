/**
 * Headed Electron E2E config.
 *
 * Serial and single-worker on purpose: these launch a real application, and two
 * instances would race on the same userData directory and on the engine
 * process. A flaky E2E is worse than none — it teaches people to re-run until
 * green.
 *
 * No retries. A retry would hide exactly the intermittent wiring faults this
 * suite exists to catch.
 */
import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 60_000,
  reporter: [['list']],
  use: {
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
})
